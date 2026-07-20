use super::entry::{EntrySource, LeafEntry};
use super::error::{LayoutError, in_field_path};
use super::header::Header;
use super::resolved::{ResolvedLayout, validate_static};
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

struct PendingChecksum {
    leaf_index: usize,
    buffer_position: usize,
    scalar_type: ScalarType,
    config_name: String,
    field_path: Vec<String>,
}

struct PendingValueRecord {
    leaf_index: usize,
    path: Vec<String>,
    value: serde_json::Value,
}

struct StagingValueSink<'a> {
    leaf_index: usize,
    records: &'a mut Vec<PendingValueRecord>,
}

impl ValueSink for StagingValueSink<'_> {
    fn record_value(
        &mut self,
        path: &[String],
        value: serde_json::Value,
    ) -> Result<(), LayoutError> {
        self.records.push(PendingValueRecord {
            leaf_index: self.leaf_index,
            path: path.to_vec(),
            value,
        });
        Ok(())
    }
}

pub(crate) struct BuildConfig<'a> {
    pub(crate) endianness: &'a Endianness,
    pub(crate) padding: u8,
    pub(crate) strict: bool,
    pub(crate) consts: &'a HashMap<String, ValueSource>,
}

pub(crate) struct BuildOutput {
    pub(crate) bytestream: Vec<u8>,
    pub(crate) checksum_values: Vec<u32>,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

impl Block {
    pub(crate) fn emit(
        &self,
        block_name: &str,
        fingerprints: &HashMap<String, u64>,
        data_source: Option<&dyn DataSource>,
        settings: &MintConfig,
        strict: bool,
        value_sink: &mut dyn ValueSink,
    ) -> Result<BuildOutput, LayoutError> {
        let resolved = validate_static(self, settings)?;
        let total_size = resolved.total_size();
        let config = BuildConfig {
            endianness: &settings.endianness,
            padding: self.header.padding,
            strict,
            consts: &settings.consts,
        };
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(total_size).map_err(|error| {
            LayoutError::DataValueExportFailed(format!(
                "failed to allocate {total_size}-byte block buffer: {error}"
            ))
        })?;
        buffer.resize(total_size, self.header.padding);
        let mut pending_checksums = Vec::new();
        let mut pending_values = Vec::new();

        for (leaf_index, (path, coordinates, leaf)) in resolved.emission_leaves().enumerate() {
            let field_path = path.split('.').map(str::to_owned).collect::<Vec<_>>();
            let mut staging_sink = StagingValueSink {
                leaf_index,
                records: &mut pending_values,
            };
            let bytes = (|| -> Result<Vec<u8>, LayoutError> {
                match &leaf.source {
                    EntrySource::Ref(target) => Self::emit_ref(
                        leaf,
                        target,
                        &resolved,
                        &self.header,
                        &config,
                        &mut staging_sink,
                        &field_path,
                    ),
                    EntrySource::Checksum(config_name) => {
                        settings.checksum_config(config_name)?;
                        pending_checksums.push(PendingChecksum {
                            leaf_index,
                            buffer_position: coordinates.offset,
                            scalar_type: leaf.scalar_type,
                            config_name: config_name.clone(),
                            field_path: field_path.clone(),
                        });
                        Ok(vec![0; leaf.scalar_type.size_bytes()])
                    }
                    EntrySource::Fingerprint(target) => {
                        let target_name = target.block_name(block_name);
                        let value = fingerprints.get(target_name).ok_or_else(|| {
                            LayoutError::BlockNotFound(format!(
                                "fingerprint target '{target_name}' from block '{block_name}'. Available blocks: {}",
                                fingerprints.keys().cloned().collect::<Vec<_>>().join(", ")
                            ))
                        })?;
                        let bytes = DataValue::U64(*value).to_bytes(
                            leaf.scalar_type,
                            config.endianness,
                            true,
                        )?;
                        staging_sink.record_value(
                            &field_path,
                            serde_json::Value::Number(serde_json::Number::from(*value)),
                        )?;
                        Ok(bytes)
                    }
                    _ => leaf.emit_bytes(data_source, &config, &mut staging_sink, &field_path),
                }
            })()
            .map_err(|error| in_field_path(path, error))?;

            if bytes.len() != coordinates.size {
                return Err(in_field_path(
                    path,
                    LayoutError::DataValueExportFailed(format!(
                        "emitted {} bytes but resolved size is {} bytes",
                        bytes.len(),
                        coordinates.size
                    )),
                ));
            }
            let end = coordinates
                .offset
                .checked_add(coordinates.size)
                .ok_or_else(|| {
                    in_field_path(
                        path,
                        LayoutError::DataValueExportFailed(
                            "resolved leaf range overflow during emission".to_owned(),
                        ),
                    )
                })?;
            let slot = buffer.get_mut(coordinates.offset..end).ok_or_else(|| {
                in_field_path(
                    path,
                    LayoutError::DataValueExportFailed(
                        "resolved leaf range exceeds output buffer".to_owned(),
                    ),
                )
            })?;
            slot.copy_from_slice(&bytes);
        }

        let checksum_values = Self::resolve_checksums(
            &mut buffer,
            &pending_checksums,
            settings,
            &config,
            &mut pending_values,
        )?;

        pending_values.sort_by_key(|record| record.leaf_index);
        for record in pending_values {
            value_sink.record_value(&record.path, record.value)?;
        }

        Ok(BuildOutput {
            bytestream: buffer,
            checksum_values,
        })
    }

    fn emit_ref(
        leaf: &LeafEntry,
        target: &str,
        resolved: &ResolvedLayout<'_>,
        header: &Header,
        config: &BuildConfig<'_>,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        let target_offset = resolved.coordinates(target).ok_or_else(|| {
            LayoutError::DataValueExportFailed(format!(
                "Ref target '{target}' not found in resolved layout."
            ))
        })?;
        let target_offset = u64::try_from(target_offset.offset).map_err(|_| {
            LayoutError::DataValueExportFailed(format!(
                "Address overflow resolving ref to '{target}'."
            ))
        })?;
        let address = u64::from(header.start_address)
            .checked_add(target_offset)
            .ok_or_else(|| {
                LayoutError::DataValueExportFailed(format!(
                    "Address overflow resolving ref to '{target}'."
                ))
            })?;
        let bytes = DataValue::U64(address).to_bytes(leaf.scalar_type, config.endianness, true)?;
        value_sink.record_value(
            field_path,
            serde_json::Value::Number(serde_json::Number::from(address)),
        )?;
        Ok(bytes)
    }

    fn resolve_checksums(
        buffer: &mut [u8],
        pending_checksums: &[PendingChecksum],
        settings: &MintConfig,
        config: &BuildConfig<'_>,
        pending_values: &mut Vec<PendingValueRecord>,
    ) -> Result<Vec<u32>, LayoutError> {
        let mut checksum_values = Vec::with_capacity(pending_checksums.len());
        for pending in pending_checksums {
            let crc_config = settings.checksum_config(&pending.config_name)?;
            let crc_val = checksum::calculate_crc(&buffer[..pending.buffer_position], crc_config);
            let crc_bytes = match config.endianness {
                Endianness::Big => crc_val.to_be_bytes(),
                Endianness::Little => crc_val.to_le_bytes(),
            };
            let size = pending.scalar_type.size_bytes();
            buffer[pending.buffer_position..pending.buffer_position + size]
                .copy_from_slice(&crc_bytes[..size]);
            pending_values.push(PendingValueRecord {
                leaf_index: pending.leaf_index,
                path: pending.field_path.clone(),
                value: serde_json::Value::Number(serde_json::Number::from(crc_val as u64)),
            });
            checksum_values.push(crc_val);
        }
        Ok(checksum_values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[derive(Default)]
    struct RecordingSink {
        paths: Vec<String>,
    }

    impl ValueSink for RecordingSink {
        fn record_value(&mut self, path: &[String], _value: Value) -> Result<(), LayoutError> {
            self.paths.push(path.join("."));
            Ok(())
        }
    }

    #[test]
    fn value_sink_records_values_in_declaration_order() {
        let config = crate::layout::parse_toml_layout(
            r#"
[mint]
endianness = "little"

[mint.checksum.crc32]
polynomial = 0x04C11DB7
start = 0xFFFFFFFF
xor_out = 0xFFFFFFFF
ref_in = true
ref_out = true

[block.header]
start_address = 0x1000
length = 0x40

[block.data]
first = { value = 1, type = "u16" }
pointer = { ref = "first", type = "u16" }
fingerprint = { fingerprint = true, type = "u64" }
checksum_one = { checksum = "crc32", type = "u32" }
after_checksum = { value = 2, type = "u32" }
checksum_two = { checksum = "crc32", type = "u32" }
"#,
        )
        .expect("layout parses");
        let mut fingerprints = HashMap::new();
        fingerprints.insert("block".to_owned(), 0x636c_a69e_b274_aafa);
        let mut sink = RecordingSink::default();

        let output = config.blocks["block"]
            .emit("block", &fingerprints, None, &config.mint, false, &mut sink)
            .expect("block emits");

        assert_eq!(
            sink.paths,
            [
                "first",
                "pointer",
                "fingerprint",
                "checksum_one",
                "after_checksum",
                "checksum_two",
            ]
        );
        assert_eq!(output.checksum_values.len(), 2);
    }

    #[test]
    fn short_fixed_size_leaves_pad_internally_with_the_padding_byte() {
        let config = crate::layout::parse_toml_layout(
            r#"
[mint]
endianness = "little"

[block.header]
start_address = 0x1000
length = 0x20
padding = 0xFF

[block.data]
text = { value = "A", type = "u8", size = 4 }
word = { value = 1, type = "u32" }
"#,
        )
        .expect("layout parses");
        let mut sink = super::super::used_values::NoopValueSink;

        let output = config.blocks["block"]
            .emit(
                "block",
                &HashMap::new(),
                None,
                &config.mint,
                false,
                &mut sink,
            )
            .expect("block emits");

        assert_eq!(
            output.bytestream,
            [b'A', 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00]
        );
    }
}
