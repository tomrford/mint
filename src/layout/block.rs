use super::entry::{EntrySource, LeafEntry};
use super::error::LayoutError;
use super::header::Header;
use super::settings::{Endianness, Settings};
use super::used_values::ValueSink;
use super::value::DataValue;
use crate::data::DataSource;

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;

/// A ref that needs to be resolved after the main traversal pass.
struct PendingRef {
    /// Position in the output buffer where placeholder bytes were written.
    buffer_position: usize,
    /// The dotted field path of the target being referenced.
    target_path: String,
    /// Scalar type for encoding the resolved address.
    scalar_type: super::entry::ScalarType,
    /// Field path of the ref entry itself (for value_sink and error messages).
    field_path: Vec<String>,
}

/// Mutable state tracked during recursive bytestream building
struct BuildState {
    buffer: Vec<u8>,
    offset: usize,
    padding_count: u32,
    /// Maps dotted field paths to their byte offsets within the block data.
    known_offsets: HashMap<String, usize>,
    /// Refs whose targets may not yet be known; resolved after traversal.
    pending_refs: Vec<PendingRef>,
}

/// Immutable configuration for bytestream building
pub struct BuildConfig<'a> {
    pub endianness: &'a Endianness,
    pub padding: u8,
    pub strict: bool,
    pub word_addressing: bool,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub settings: Settings,
    #[serde(flatten)]
    pub blocks: IndexMap<String, Block>,
}

/// Flash block.
#[derive(Debug, Deserialize)]
pub struct Block {
    pub header: Header,
    pub data: Entry,
}

/// Any entry - should always be either a leaf or a branch (more entries).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    Leaf(LeafEntry),
    Branch(IndexMap<String, Entry>),
}

impl Block {
    pub fn build_bytestream(
        &self,
        data_source: Option<&dyn DataSource>,
        settings: &Settings,
        strict: bool,
        value_sink: &mut dyn ValueSink,
    ) -> Result<(Vec<u8>, u32), LayoutError> {
        let mut state = BuildState {
            buffer: Vec::with_capacity((self.header.length as usize).min(64 * 1024)),
            offset: 0,
            padding_count: 0,
            known_offsets: HashMap::new(),
            pending_refs: Vec::new(),
        };
        let config = BuildConfig {
            endianness: &settings.endianness,
            padding: self.header.padding,
            strict,
            word_addressing: settings.word_addressing,
        };

        let mut field_path = Vec::new();
        Self::build_bytestream_inner(
            &self.data,
            data_source,
            &mut state,
            &config,
            value_sink,
            &mut field_path,
        )?;

        // Resolve pending refs now that all offsets are known.
        if !state.pending_refs.is_empty() {
            Self::resolve_pending_refs(
                &mut state,
                &config,
                &self.header,
                &settings.virtual_offset,
                value_sink,
            )?;
        }

        Ok((state.buffer, state.padding_count))
    }

    fn build_bytestream_inner(
        table: &Entry,
        data_source: Option<&dyn DataSource>,
        state: &mut BuildState,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &mut Vec<String>,
    ) -> Result<(), LayoutError> {
        match table {
            Entry::Leaf(leaf) => {
                let alignment = leaf.get_alignment();
                while !state.offset.is_multiple_of(alignment) {
                    state.buffer.push(config.padding);
                    state.offset += 1;
                    state.padding_count += 1;
                }

                // Record this field's offset for ref resolution.
                let path_key = field_path.join(".");
                state.known_offsets.insert(path_key, state.offset);

                // Handle ref entries: write placeholder bytes, defer resolution.
                if let EntrySource::Ref(target) = &leaf.source {
                    leaf.validate_ref(target)?;
                    let size = leaf.scalar_type.size_bytes();
                    let buffer_position = state.buffer.len();
                    // Write placeholder zeros.
                    state.buffer.extend(std::iter::repeat_n(0u8, size));
                    state.offset += size;
                    state.pending_refs.push(PendingRef {
                        buffer_position,
                        target_path: target.clone(),
                        scalar_type: leaf.scalar_type,
                        field_path: field_path.clone(),
                    });
                    return Ok(());
                }

                let bytes = leaf.emit_bytes(data_source, config, value_sink, field_path)?;
                state.offset += bytes.len();
                state.buffer.extend(bytes);
            }
            Entry::Branch(branch) => {
                for (field_name, v) in branch.iter() {
                    let path_len = field_path.len();
                    let segments = split_field_path(field_name)?;
                    field_path.extend(segments);

                    // Record branch offset for ref resolution (start of nested struct).
                    let path_key = field_path.join(".");
                    state.known_offsets.insert(path_key, state.offset);

                    let result = Self::build_bytestream_inner(
                        v,
                        data_source,
                        state,
                        config,
                        value_sink,
                        field_path,
                    );
                    field_path.truncate(path_len);
                    result.map_err(|e| LayoutError::InField {
                        field: field_name.clone(),
                        source: Box::new(e),
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Resolves all pending refs by looking up target offsets and patching the buffer.
    fn resolve_pending_refs(
        state: &mut BuildState,
        config: &BuildConfig,
        header: &Header,
        virtual_offset: &u32,
        value_sink: &mut dyn ValueSink,
    ) -> Result<(), LayoutError> {
        for pending in &state.pending_refs {
            let target_offset =
                state
                    .known_offsets
                    .get(&pending.target_path)
                    .ok_or_else(|| LayoutError::DataValueExportFailed(format!(
                        "Ref target '{}' not found in block. Available fields: [{}]",
                        pending.target_path,
                        state
                            .known_offsets
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))?;

            let address = header
                .start_address
                .checked_add(*virtual_offset)
                .and_then(|a| a.checked_add(*target_offset as u32))
                .ok_or_else(|| {
                    LayoutError::DataValueExportFailed(format!(
                        "Address overflow resolving ref to '{}'.",
                        pending.target_path
                    ))
                })?;

            let address_value = DataValue::U64(address as u64);
            let bytes = address_value.to_bytes(
                pending.scalar_type,
                config.endianness,
                config.strict,
            )?;

            // Patch the placeholder bytes in the buffer.
            let pos = pending.buffer_position;
            state.buffer[pos..pos + bytes.len()].copy_from_slice(&bytes);

            // Record the resolved address in value_sink.
            value_sink.record_value(
                &pending.field_path,
                serde_json::Value::Number(serde_json::Number::from(address as u64)),
            )?;
        }
        Ok(())
    }
}

fn split_field_path(field_name: &str) -> Result<Vec<String>, LayoutError> {
    let segments: Vec<&str> = field_name.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return Err(LayoutError::DataValueExportFailed(format!(
            "Invalid field path '{}'.",
            field_name
        )));
    }
    Ok(segments.into_iter().map(|s| s.to_string()).collect())
}
