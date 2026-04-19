use super::entry::{EntrySource, LeafEntry};
use super::error::LayoutError;
use super::header::Header;
use super::scalar_type::ScalarType;
use super::settings::{Endianness, MintConfig};
use super::used_values::ValueSink;
use super::value::DataValue;
use crate::data::DataSource;
use crate::output::checksum;

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
    scalar_type: ScalarType,
    /// Field path of the ref entry itself (for value_sink and error messages).
    field_path: Vec<String>,
}

/// A checksum that needs to be resolved after the main traversal pass.
struct PendingChecksum {
    /// Position in the output buffer where placeholder bytes were written.
    buffer_position: usize,
    /// Scalar type for encoding the checksum value.
    scalar_type: ScalarType,
    /// Name of the checksum config in `[mint.checksum]`.
    config_name: String,
    /// Field path of the checksum entry (for value_sink and error messages).
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
    /// Checksums to compute after the bytestream is fully built.
    pending_checksums: Vec<PendingChecksum>,
    /// Resolved checksum values in field-order for stats/visualization.
    resolved_checksum_values: Vec<u32>,
}

/// Immutable configuration for bytestream building
pub struct BuildConfig<'a> {
    pub endianness: &'a Endianness,
    pub padding: u8,
    pub strict: bool,
}

pub struct BuildOutput {
    pub bytestream: Vec<u8>,
    pub padding_count: u32,
    pub checksum_values: Vec<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mint: MintConfig,
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
        settings: &MintConfig,
        strict: bool,
        value_sink: &mut dyn ValueSink,
    ) -> Result<BuildOutput, LayoutError> {
        let mut state = BuildState {
            buffer: Vec::with_capacity((self.header.length as usize).min(64 * 1024)),
            offset: 0,
            padding_count: 0,
            known_offsets: HashMap::new(),
            pending_refs: Vec::new(),
            pending_checksums: Vec::new(),
            resolved_checksum_values: Vec::new(),
        };
        let config = BuildConfig {
            endianness: &settings.endianness,
            padding: self.header.padding,
            strict,
        };

        let mut field_path = Vec::new();
        let _ = Self::build_bytestream_inner(
            &self.data,
            data_source,
            settings,
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

        // Resolve pending checksums now that the bytestream is complete.
        if !state.pending_checksums.is_empty() {
            Self::resolve_pending_checksums(&mut state, settings, &config, value_sink)?;
        }

        Ok(BuildOutput {
            bytestream: state.buffer,
            padding_count: state.padding_count,
            checksum_values: state.resolved_checksum_values,
        })
    }

    /// Recursively builds the bytestream. Returns the byte offset of the
    /// first data byte emitted (post-alignment). The branch caller records
    /// each child's path in `known_offsets` on exit from recursion.
    fn build_bytestream_inner(
        table: &Entry,
        data_source: Option<&dyn DataSource>,
        settings: &MintConfig,
        state: &mut BuildState,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &mut Vec<String>,
    ) -> Result<usize, LayoutError> {
        match table {
            Entry::Leaf(leaf) => {
                let alignment = leaf.get_alignment();
                while !state.offset.is_multiple_of(alignment) {
                    state.buffer.push(config.padding);
                    state.offset += 1;
                    state.padding_count += 1;
                }

                let leaf_offset = state.offset;

                if let EntrySource::Ref(target) = &leaf.source {
                    leaf.validate_ref(target)?;
                    let size = leaf.scalar_type.size_bytes();
                    state.pending_refs.push(PendingRef {
                        buffer_position: state.buffer.len(),
                        target_path: target.clone(),
                        scalar_type: leaf.scalar_type,
                        field_path: field_path.clone(),
                    });
                    state.buffer.extend(std::iter::repeat_n(0u8, size));
                    state.offset += size;
                    return Ok(leaf_offset);
                }

                if let EntrySource::Checksum(config_name) = &leaf.source {
                    leaf.validate_checksum(config_name, settings)?;
                    let size = leaf.scalar_type.size_bytes();
                    state.pending_checksums.push(PendingChecksum {
                        buffer_position: state.buffer.len(),
                        scalar_type: leaf.scalar_type,
                        config_name: config_name.clone(),
                        field_path: field_path.clone(),
                    });
                    state.buffer.extend(std::iter::repeat_n(0u8, size));
                    state.offset += size;
                    return Ok(leaf_offset);
                }

                let bytes = leaf.emit_bytes(data_source, config, value_sink, field_path)?;
                state.offset += bytes.len();
                state.buffer.extend(bytes);
                Ok(leaf_offset)
            }
            Entry::Branch(branch) => {
                if branch.is_empty() {
                    let branch_path = if field_path.is_empty() {
                        "<root>".to_owned()
                    } else {
                        field_path.join(".")
                    };
                    return Err(LayoutError::DataValueExportFailed(format!(
                        "Empty branch '{}' is invalid.",
                        branch_path
                    )));
                }

                let mut branch_offset = None;
                for (field_name, v) in branch.iter() {
                    let path_len = field_path.len();
                    field_path.extend(split_field_path(field_name)?);

                    let offset = Self::build_bytestream_inner(
                        v,
                        data_source,
                        settings,
                        state,
                        config,
                        value_sink,
                        field_path,
                    );

                    if let Ok(o) = offset {
                        state.known_offsets.insert(field_path.join("."), o);
                        branch_offset.get_or_insert(o);
                    }

                    field_path.truncate(path_len);
                    offset.map_err(|e| LayoutError::InField {
                        field: field_name.clone(),
                        source: Box::new(e),
                    })?;
                }
                Ok(branch_offset.unwrap_or(state.offset))
            }
        }
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
            let target_offset = state
                .known_offsets
                .get(&pending.target_path)
                .ok_or_else(|| {
                    LayoutError::DataValueExportFailed(format!(
                        "Ref target '{}' not found in block. Available fields: [{}]",
                        pending.target_path,
                        state
                            .known_offsets
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;

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
            let bytes =
                address_value.to_bytes(pending.scalar_type, config.endianness, config.strict)?;

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

    /// Resolves all pending checksums by computing CRC over the buffer and patching in the result.
    fn resolve_pending_checksums(
        state: &mut BuildState,
        settings: &MintConfig,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
    ) -> Result<(), LayoutError> {
        for pending in &state.pending_checksums {
            let crc_config = settings.checksum.get(&pending.config_name).ok_or_else(|| {
                LayoutError::DataValueExportFailed(format!(
                    "Checksum config '{}' not found in [mint.checksum].",
                    pending.config_name
                ))
            })?;

            let crc_val =
                checksum::calculate_crc(&state.buffer[..pending.buffer_position], crc_config);

            // Convert CRC to bytes with proper endianness.
            let crc_bytes = match config.endianness {
                Endianness::Big => crc_val.to_be_bytes(),
                Endianness::Little => crc_val.to_le_bytes(),
            };

            // Patch the placeholder bytes in the buffer.
            let size = pending.scalar_type.size_bytes();
            state.buffer[pending.buffer_position..pending.buffer_position + size]
                .copy_from_slice(&crc_bytes[..size]);

            // Record the resolved value in value_sink.
            value_sink.record_value(
                &pending.field_path,
                serde_json::Value::Number(serde_json::Number::from(crc_val as u64)),
            )?;
            state.resolved_checksum_values.push(crc_val);
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
    Ok(segments.into_iter().map(|s| s.to_owned()).collect())
}
