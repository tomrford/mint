use super::entry::{EntrySource, LeafEntry};
use super::error::LayoutError;
use super::header::Header;
use super::scalar_type::ScalarType;
use super::settings::{Endianness, MintConfig};
use super::used_values::ValueSink;
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use crate::output::checksum;

use indexmap::IndexMap;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;

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
    pub consts: &'a HashMap<String, ValueSource>,
}

struct BuildContext<'a> {
    config: BuildConfig<'a>,
    block_name: &'a str,
    fingerprints: &'a HashMap<String, u64>,
}

pub struct BuildOutput {
    pub bytestream: Vec<u8>,
    pub padding_count: u32,
    pub checksum_values: Vec<u32>,
}

#[derive(Debug)]
pub struct Config {
    pub mint: MintConfig,
    pub blocks: IndexMap<String, Block>,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ConfigVisitor;

        impl<'de> Visitor<'de> for ConfigVisitor {
            type Value = Config;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a layout configuration table")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut mint = None;
                let mut blocks = IndexMap::new();

                while let Some(name) = map.next_key::<String>()? {
                    if name == "mint" {
                        if mint.is_some() {
                            return Err(M::Error::duplicate_field("mint"));
                        }
                        mint = Some(map.next_value()?);
                    } else {
                        super::validate_c_identifier(&name, "block").map_err(M::Error::custom)?;
                        blocks.insert(name, map.next_value()?);
                    }
                }

                let mint = mint.ok_or_else(|| M::Error::missing_field("mint"))?;
                Ok(Config { mint, blocks })
            }
        }

        deserializer.deserialize_map(ConfigVisitor)
    }
}

/// Flash block.
#[derive(Debug, Deserialize)]
pub struct Block {
    pub header: Header,
    pub data: Entry,
}

#[derive(Debug)]
pub enum Entry {
    Leaf(LeafEntry),
    Branch(IndexMap<String, Entry>),
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let table = toml::Table::deserialize(deserializer)?;
        if matches!(table.get("type"), Some(toml::Value::String(_))) {
            return toml::Value::Table(table)
                .try_into()
                .map(Entry::Leaf)
                .map_err(D::Error::custom);
        }

        let mut branch = IndexMap::with_capacity(table.len());
        for (name, value) in table {
            super::validate_c_identifier(&name, "field").map_err(D::Error::custom)?;
            let entry = value
                .try_into()
                .map_err(|error| D::Error::custom(format!("in data field '{name}': {error}")))?;
            branch.insert(name, entry);
        }
        Ok(Entry::Branch(branch))
    }
}

impl Entry {
    fn alignment(&self) -> usize {
        match self {
            Entry::Leaf(leaf) => leaf.get_alignment(),
            Entry::Branch(branch) => branch.values().map(Self::alignment).max().unwrap_or(1),
        }
    }
}

impl Block {
    pub fn build_bytestream(
        &self,
        block_name: &str,
        fingerprints: &HashMap<String, u64>,
        data_source: Option<&dyn DataSource>,
        settings: &MintConfig,
        strict: bool,
        value_sink: &mut dyn ValueSink,
    ) -> Result<BuildOutput, LayoutError> {
        let mut state = BuildState {
            buffer: Vec::with_capacity((self.header.length as usize).min(64 * 1024)),
            padding_count: 0,
            known_offsets: HashMap::new(),
            pending_refs: Vec::new(),
            pending_checksums: Vec::new(),
            resolved_checksum_values: Vec::new(),
        };
        let context = BuildContext {
            config: BuildConfig {
                endianness: &settings.endianness,
                padding: self.header.padding,
                strict,
                consts: &settings.consts,
            },
            block_name,
            fingerprints,
        };

        let mut field_path = Vec::new();
        let _ = Self::build_bytestream_inner(
            &self.data,
            data_source,
            settings,
            &mut state,
            &context,
            value_sink,
            &mut field_path,
        )?;

        // Resolve pending refs now that all offsets are known.
        if !state.pending_refs.is_empty() {
            Self::resolve_pending_refs(&mut state, &context.config, &self.header, value_sink)?;
        }

        // Resolve pending checksums now that the bytestream is complete.
        if !state.pending_checksums.is_empty() {
            Self::resolve_pending_checksums(&mut state, settings, &context.config, value_sink)?;
        }

        Ok(BuildOutput {
            bytestream: state.buffer,
            padding_count: state.padding_count,
            checksum_values: state.resolved_checksum_values,
        })
    }

    /// Recursively builds the bytestream. Returns the aligned byte offset of
    /// the entry. The branch caller records each child's path in
    /// `known_offsets` on exit from recursion.
    fn build_bytestream_inner(
        table: &Entry,
        data_source: Option<&dyn DataSource>,
        settings: &MintConfig,
        state: &mut BuildState,
        context: &BuildContext,
        value_sink: &mut dyn ValueSink,
        field_path: &mut Vec<String>,
    ) -> Result<usize, LayoutError> {
        match table {
            Entry::Leaf(leaf) => {
                let alignment = leaf.get_alignment();
                pad_to_alignment(state, context.config.padding, alignment);

                let leaf_offset = state.buffer.len();

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
                    return Ok(leaf_offset);
                }

                if let EntrySource::Checksum(config_name) = &leaf.source {
                    leaf.validate_checksum(config_name, settings)?;
                    if state.buffer.is_empty() {
                        return Err(LayoutError::DataValueExportFailed(
                            "Checksum must follow at least one data byte.".to_owned(),
                        ));
                    }
                    let size = leaf.scalar_type.size_bytes();
                    state.pending_checksums.push(PendingChecksum {
                        buffer_position: state.buffer.len(),
                        scalar_type: leaf.scalar_type,
                        config_name: config_name.clone(),
                        field_path: field_path.clone(),
                    });
                    state.buffer.extend(std::iter::repeat_n(0u8, size));
                    return Ok(leaf_offset);
                }

                if let EntrySource::Fingerprint(target) = &leaf.source {
                    leaf.validate_fingerprint(target)?;
                    let target_name = target.block_name(context.block_name)?;
                    let value = context.fingerprints.get(target_name).ok_or_else(|| {
                        LayoutError::BlockNotFound(format!(
                            "fingerprint target '{target_name}' from block '{}'. Available blocks: {}",
                            context.block_name,
                            context.fingerprints.keys().cloned().collect::<Vec<_>>().join(", ")
                        ))
                    })?;
                    let bytes = DataValue::U64(*value).to_bytes(
                        leaf.scalar_type,
                        context.config.endianness,
                        true,
                    )?;
                    value_sink.record_value(
                        field_path,
                        serde_json::Value::Number(serde_json::Number::from(*value)),
                    )?;
                    state.buffer.extend(bytes);
                    return Ok(leaf_offset);
                }

                let bytes =
                    leaf.emit_bytes(data_source, &context.config, value_sink, field_path)?;
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

                let alignment = table.alignment();
                pad_to_alignment(state, context.config.padding, alignment);
                let branch_offset = state.buffer.len();

                for (field_name, v) in branch.iter() {
                    let path_len = field_path.len();
                    field_path.push(field_name.clone());

                    let offset = Self::build_bytestream_inner(
                        v,
                        data_source,
                        settings,
                        state,
                        context,
                        value_sink,
                        field_path,
                    );

                    if let Ok(o) = offset {
                        let joined = field_path.join(".");
                        state.known_offsets.insert(joined, o);
                    }

                    field_path.truncate(path_len);
                    offset.map_err(|e| LayoutError::InField {
                        field: field_name.clone(),
                        source: Box::new(e),
                    })?;
                }

                pad_to_alignment(state, context.config.padding, alignment);
                Ok(branch_offset)
            }
        }
    }

    /// Resolves all pending refs by looking up target offsets and patching the buffer.
    fn resolve_pending_refs(
        state: &mut BuildState,
        config: &BuildConfig,
        header: &Header,
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
                .checked_add(*target_offset as u32)
                .ok_or_else(|| {
                    LayoutError::DataValueExportFailed(format!(
                        "Address overflow resolving ref to '{}'.",
                        pending.target_path
                    ))
                })?;

            let address_value = DataValue::U64(address as u64);
            let bytes = address_value.to_bytes(pending.scalar_type, config.endianness, true)?;

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

fn pad_to_alignment(state: &mut BuildState, padding: u8, alignment: usize) {
    let padding_len = state.buffer.len().next_multiple_of(alignment) - state.buffer.len();
    state
        .buffer
        .extend(std::iter::repeat_n(padding, padding_len));
    state.padding_count += padding_len as u32;
}
