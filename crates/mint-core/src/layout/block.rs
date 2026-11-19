use super::abi::Endianness;
use super::entry::{LeafEntry, SizeSource, Slot};
use super::error::{LayoutError, in_field_path};
use super::header::Header;
use super::resolved::{LeafBinding, PlannedBlock, ResolvedLeafEntry};
use super::settings::MintConfig;
use super::used_values::ValueCollector;
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use crate::output::checksum;
use std::borrow::Cow;

use indexmap::IndexMap;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

pub(crate) struct BuildConfig {
    pub(crate) endianness: Endianness,
    pub(crate) strict: bool,
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

const HEADER_CONSTS: [&str; 2] = ["start_address", "length"];

impl Config {
    fn header_const(&self, name: &str) -> Option<u32> {
        let (block, field) = name.rsplit_once('.')?;
        let header = &self.blocks.get(block)?.header;
        match field {
            "start_address" => Some(header.start_address),
            "length" => Some(header.length),
            _ => None,
        }
    }

    pub(crate) fn const_value(&self, name: &str) -> Result<Cow<'_, ValueSource>, LayoutError> {
        if name.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Const name must not be empty.".into(),
            ));
        }
        if let Some(value) = self.header_const(name) {
            return Ok(Cow::Owned(ValueSource::Single(DataValue::U64(u64::from(
                value,
            )))));
        }
        self.mint
            .consts
            .get(name)
            .map(Cow::Borrowed)
            .ok_or_else(|| {
                let mut available = self.mint.consts.keys().cloned().collect::<Vec<_>>();
                available.extend(
                    self.blocks
                        .keys()
                        .flat_map(|block| HEADER_CONSTS.map(|field| format!("{block}.{field}"))),
                );
                available.sort_unstable();
                LayoutError::DataValueExportFailed(format!(
                    "Const '{name}' not found in [mint.const]. Available: [{}]",
                    available.join(", ")
                ))
            })
    }
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
                let config = Config { mint, blocks };
                if let Some(key) = config
                    .mint
                    .consts
                    .keys()
                    .filter(|key| config.header_const(key).is_some())
                    .min()
                {
                    return Err(M::Error::custom(format!(
                        "[mint.const] key '{key}' collides with auto-promoted block header const"
                    )));
                }
                Ok(config)
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
        plan: &PlannedBlock<'_>,
        fingerprints: &IndexMap<&str, u64>,
        data_source: Option<&dyn DataSource>,
        strict: bool,
        value_sink: &mut ValueCollector,
    ) -> Result<BuildOutput, LayoutError> {
        let resolved = &plan.resolved;
        let total_size = resolved.total_size();
        let config = BuildConfig {
            endianness: resolved.abi().endianness(),
            strict,
        };
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(total_size).map_err(|error| {
            LayoutError::DataValueExportFailed(format!(
                "failed to allocate {total_size}-byte block buffer: {error}"
            ))
        })?;
        buffer.resize(total_size, self.header.padding);
        let mut checksum_values = Vec::new();

        for field in &plan.fields {
            let ResolvedLeafEntry {
                path,
                coordinates,
                scalar_abi,
                leaf,
            } = &field.leaf;
            let (prefix, rest) = buffer.split_at_mut(coordinates.offset);
            let mut slot = Slot::new(&mut rest[..coordinates.size], *scalar_abi);
            let mut emit = || -> Result<(), LayoutError> {
                let value = match &field.binding {
                    LeafBinding::Source => {
                        return leaf.emit_from_source(
                            &mut slot,
                            data_source,
                            &config,
                            value_sink,
                            path,
                        );
                    }
                    LeafBinding::Literal(value) => {
                        return leaf.emit_literal(&mut slot, value, &config, value_sink, path);
                    }
                    LeafBinding::RefList(addresses) => {
                        let Some(SizeSource::OneD(capacity)) = leaf.size else {
                            unreachable!("ref list shape was validated during resolution");
                        };
                        for (index, &address) in addresses.iter().enumerate() {
                            let encoded = DataValue::U64(address).encode(
                                leaf.scalar_type,
                                config.endianness,
                                true,
                            )?;
                            slot.write_element(index, encoded);
                        }
                        // Unused entries encode address zero.
                        let zero =
                            DataValue::U64(0).encode(leaf.scalar_type, config.endianness, true)?;
                        for index in addresses.len()..capacity {
                            slot.write_element(index, zero);
                        }
                        return value_sink.record_value(path, || Ok(addresses.as_slice().into()));
                    }
                    LeafBinding::Checksum(config) => {
                        let crc = checksum::calculate_crc(prefix, config);
                        checksum_values.push(crc);
                        u64::from(crc)
                    }
                    LeafBinding::Ref(address) => *address,
                    LeafBinding::Fingerprint(target) => fingerprints[*target],
                };
                let encoded =
                    DataValue::U64(value).encode(leaf.scalar_type, config.endianness, true)?;
                slot.write_scalar(encoded);
                value_sink.record_value(path, || Ok(value.into()))
            };
            emit().map_err(|error| in_field_path(path, error))?;
        }

        Ok(BuildOutput {
            bytestream: buffer,
            checksum_values,
        })
    }
}
