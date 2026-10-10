use super::abi::{Abi, ScalarAbi};
use super::entry::{EntrySource, LeafEntry, RefSource, SizeSource, append_array_element};
use super::error::{LayoutError, in_field_path};
use super::fingerprint::ResolvedBlocks;
use super::header::Header;
use super::resolved::ResolvedLayout;
use super::settings::MintConfig;
use super::used_values::ValueCollector;
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use crate::output::checksum;

use indexmap::IndexMap;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;

pub(crate) struct BuildConfig<'a> {
    pub(crate) abi: Abi,
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
        blocks: &ResolvedBlocks<'_>,
        data_source: Option<&dyn DataSource>,
        settings: &MintConfig,
        strict: bool,
        value_sink: &mut ValueCollector,
    ) -> Result<BuildOutput, LayoutError> {
        let resolved = &blocks.blocks[block_name];
        resolved.validate(self, settings)?;
        let total_size = resolved.total_size();
        let config = BuildConfig {
            abi: settings.abi,
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
        let mut checksum_values = Vec::new();

        for (path, coordinates, scalar_abi, leaf) in resolved.emission_leaves() {
            let field_path = path.split('.').map(str::to_owned).collect::<Vec<_>>();
            let bytes = (|| -> Result<Vec<u8>, LayoutError> {
                match &leaf.source {
                    EntrySource::Ref(_) => Self::emit_ref(
                        leaf,
                        resolved,
                        &self.header,
                        &config,
                        scalar_abi,
                        value_sink,
                        &field_path,
                    ),
                    EntrySource::Checksum(config_name) => {
                        // Every checksum covers only preceding fields, already emitted in order.
                        let crc = checksum::calculate_crc(
                            &buffer[..coordinates.offset], settings.checksum_config(config_name)?,
                        );
                        checksum_values.push(crc);
                        value_sink.record_value(&field_path, || Ok(crc.into()))?;
                        DataValue::U64(u64::from(crc)).to_bytes(
                            leaf.scalar_type, config.abi.endianness(), true,
                        )
                    }
                    EntrySource::Fingerprint(target) => {
                        let target_name = target.block_name(block_name);
                        let value = blocks.fingerprints.get(target_name).ok_or_else(|| {
                            LayoutError::BlockNotFound(format!(
                                "fingerprint target '{target_name}' from block '{block_name}'. Available blocks: {}",
                                blocks.fingerprints.keys().cloned().collect::<Vec<_>>().join(", ")
                            ))
                        })?;
                        let bytes = DataValue::U64(*value).to_bytes(
                            leaf.scalar_type,
                            config.abi.endianness(),
                            true,
                        )?;
                        value_sink.record_value(
                            &field_path,
                            || Ok((*value).into()),
                        )?;
                        Ok(bytes)
                    }
                    _ => leaf.emit_bytes(
                        data_source,
                        &config,
                        value_sink,
                        &field_path,
                        scalar_abi,
                    ),
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

        Ok(BuildOutput {
            bytestream: buffer,
            checksum_values,
        })
    }

    fn emit_ref(
        leaf: &LeafEntry,
        resolved: &ResolvedLayout<'_>,
        header: &Header,
        config: &BuildConfig<'_>,
        scalar_abi: ScalarAbi,
        value_sink: &mut ValueCollector,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        let EntrySource::Ref(source) = &leaf.source else {
            unreachable!("emit_ref requires a ref leaf");
        };
        let mut addresses = Vec::with_capacity(source.targets().len());
        for target in source.targets() {
            addresses.push(resolved.ref_address(target, header.start_address)?);
        }

        match source {
            RefSource::Scalar(_) => {
                let address = addresses[0];
                let bytes = DataValue::U64(address).to_bytes(
                    leaf.scalar_type,
                    config.abi.endianness(),
                    true,
                )?;
                value_sink.record_value(field_path, || Ok(address.into()))?;
                Ok(bytes)
            }
            RefSource::List(_) => {
                let Some(SizeSource::OneD(capacity)) = leaf.size else {
                    unreachable!("ref list shape was validated during resolution");
                };
                let total_bytes = capacity.checked_mul(scalar_abi.array_stride).ok_or(
                    LayoutError::DataValueExportFailed("Ref list size overflow.".to_owned()),
                )?;
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(total_bytes).map_err(|error| {
                    LayoutError::DataValueExportFailed(format!(
                        "failed to allocate {total_bytes}-byte ref list buffer: {error}"
                    ))
                })?;

                for address in &addresses {
                    let encoded = DataValue::U64(*address).to_bytes(
                        leaf.scalar_type,
                        config.abi.endianness(),
                        true,
                    )?;
                    append_array_element(&mut bytes, &encoded, scalar_abi, config.padding);
                }
                let zero =
                    DataValue::U64(0).to_bytes(leaf.scalar_type, config.abi.endianness(), true)?;
                for _ in addresses.len()..capacity {
                    append_array_element(&mut bytes, &zero, scalar_abi, config.padding);
                }

                value_sink.record_value(field_path, || Ok(addresses.into()))?;
                Ok(bytes)
            }
        }
    }
}
