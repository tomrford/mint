use super::abi::{EncodedScalar, EndianBytes, ScalarAbi};
use super::block::BuildConfig;
use super::conversions::clamp_bitfield_value;
use super::error::LayoutError;
use super::scalar_type::{ScalarType, fixed_point_unsupported_error};
use super::used_values::{
    ValueCollector, array_2d_to_json, array_to_json, data_value_to_json, i128_to_json,
};
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

const LEAF_SOURCE_KEYS: &[&str] = &[
    "name",
    "value",
    "bitmap",
    "ref",
    "checksum",
    "const",
    "fingerprint",
];
const LEAF_KEYS: &[&str] = &[
    "type",
    "size",
    "SIZE",
    "name",
    "value",
    "bitmap",
    "ref",
    "checksum",
    "const",
    "fingerprint",
];
const BITMAP_SOURCE_KEYS: &[&str] = &["name", "value"];
const BITMAP_KEYS: &[&str] = &["bits", "name", "value"];

#[derive(Debug)]
pub struct LeafEntry {
    pub scalar_type: ScalarType,
    pub size: Option<SizeSource>,
    strict_size: bool,
    pub source: EntrySource,
}

#[derive(Deserialize)]
struct RawLeafEntry {
    #[serde(rename = "type")]
    scalar_type: ScalarType,
    size: Option<SizeSource>,
    #[serde(rename = "SIZE")]
    strict_size: Option<SizeSource>,
    #[serde(flatten)]
    source: EntrySource,
}

impl<'de> Deserialize<'de> for LeafEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let table = toml::Table::deserialize(deserializer)?;
        validate_keys::<D::Error>(&table, "leaf", LEAF_KEYS, LEAF_SOURCE_KEYS)?;

        if table.contains_key("size") && table.contains_key("SIZE") {
            return Err(D::Error::custom(
                "leaf may contain only one size key; found 'size' and 'SIZE'",
            ));
        }

        let raw: RawLeafEntry = toml::Value::Table(table)
            .try_into()
            .map_err(D::Error::custom)?;
        Ok(Self {
            scalar_type: raw.scalar_type,
            size: raw.size.or(raw.strict_size),
            strict_size: raw.strict_size.is_some(),
            source: raw.source,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum SizeSource {
    OneD(usize),
    TwoD([usize; 2]),
}

#[derive(Debug, Deserialize)]
pub enum EntrySource {
    #[serde(rename = "name")]
    Name(String),
    #[serde(rename = "value")]
    Value(ValueSource),
    #[serde(rename = "bitmap")]
    Bitmap(Vec<BitmapField>),
    #[serde(rename = "ref")]
    Ref(RefSource),
    #[serde(rename = "checksum")]
    Checksum(String),
    #[serde(rename = "const")]
    Const(String),
    #[serde(rename = "fingerprint")]
    Fingerprint(FingerprintTarget),
}

#[derive(Debug, Clone)]
pub enum RefTarget {
    Path(String),
    Address(u64),
}

#[derive(Debug, Clone)]
pub enum RefSource {
    Scalar(RefTarget),
    List(Vec<RefTarget>),
}

impl<'de> Deserialize<'de> for RefSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        match value {
            toml::Value::Array(values) => values
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    ref_target(value).map_err(|message| {
                        D::Error::custom(format!("invalid ref target at index {index}: {message}"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Self::List),
            value => ref_target(value)
                .map(Self::Scalar)
                .map_err(D::Error::custom),
        }
    }
}

fn ref_target(value: toml::Value) -> Result<RefTarget, String> {
    match value {
        toml::Value::String(path) => Ok(RefTarget::Path(path)),
        toml::Value::Integer(address) if address >= 0 => Ok(RefTarget::Address(address as u64)),
        toml::Value::Integer(address) => Err(format!(
            "ref address must be an unsigned integer; got {address}"
        )),
        value => Err(format!(
            "ref target must be a path string or unsigned integer address; got {}",
            value.type_str()
        )),
    }
}

impl RefSource {
    pub(crate) fn targets(&self) -> &[RefTarget] {
        match self {
            Self::Scalar(target) => std::slice::from_ref(target),
            Self::List(targets) => targets,
        }
    }

    pub(crate) fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }
}

#[derive(Debug, Clone)]
pub enum FingerprintTarget {
    SelfBlock,
    Block(String),
}

impl FingerprintTarget {
    pub(crate) fn block_name<'a>(&'a self, current_block: &'a str) -> &'a str {
        match self {
            Self::SelfBlock => current_block,
            Self::Block(name) => name,
        }
    }
}

impl<'de> Deserialize<'de> for FingerprintTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TargetVisitor;

        impl serde::de::Visitor<'_> for TargetVisitor {
            type Value = FingerprintTarget;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("`true` for the containing block or a non-empty block name")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                if value {
                    Ok(FingerprintTarget::SelfBlock)
                } else {
                    Err(E::custom(
                        "fingerprint must be `true` for the containing block or a block name",
                    ))
                }
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.is_empty() {
                    Err(E::custom("fingerprint block name must not be empty"))
                } else {
                    Ok(FingerprintTarget::Block(value.to_owned()))
                }
            }
        }

        deserializer.deserialize_any(TargetVisitor)
    }
}

#[derive(Debug)]
pub struct BitmapField {
    pub bits: usize,
    pub source: BitmapFieldSource,
}

#[derive(Deserialize)]
struct RawBitmapField {
    bits: usize,
    #[serde(flatten)]
    source: BitmapFieldSource,
}

impl<'de> Deserialize<'de> for BitmapField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let table = toml::Table::deserialize(deserializer)?;
        validate_keys::<D::Error>(&table, "bitmap field", BITMAP_KEYS, BITMAP_SOURCE_KEYS)?;
        let raw: RawBitmapField = toml::Value::Table(table)
            .try_into()
            .map_err(D::Error::custom)?;
        Ok(Self {
            bits: raw.bits,
            source: raw.source,
        })
    }
}

fn validate_keys<E>(
    table: &toml::Table,
    kind: &str,
    valid_keys: &[&str],
    source_keys: &[&str],
) -> Result<(), E>
where
    E: serde::de::Error,
{
    let unknown = table
        .keys()
        .filter(|key| !valid_keys.contains(&key.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(E::custom(format!(
            "unknown {kind} key(s) {}; valid keys are {}, with exactly one source key from {}",
            quoted_list(&unknown),
            quoted_list(valid_keys),
            quoted_list(source_keys)
        )));
    }

    let sources = table
        .keys()
        .filter(|key| source_keys.contains(&key.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if sources.len() != 1 {
        let found = if sources.is_empty() {
            "none".to_owned()
        } else {
            quoted_list(&sources)
        };
        return Err(E::custom(format!(
            "{kind} must contain exactly one source key; found {found}; valid source keys are {}",
            quoted_list(source_keys)
        )));
    }
    Ok(())
}

fn quoted_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Deserialize)]
pub enum BitmapFieldSource {
    #[serde(rename = "name")]
    Name(String),
    #[serde(rename = "value")]
    Value(DataValue),
}

impl BitmapField {
    fn resolve_value(
        &self,
        data_source: Option<&dyn DataSource>,
    ) -> Result<DataValue, LayoutError> {
        match &self.source {
            BitmapFieldSource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Bitmap field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                Ok(ds.retrieve_single_value(name)?)
            }
            BitmapFieldSource::Value(v) => Ok(v.clone()),
        }
    }
}

impl LeafEntry {
    pub(crate) fn emit_into(
        &self,
        slot: &mut Slot<'_>,
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut ValueCollector,
        field_path: &str,
    ) -> Result<(), LayoutError> {
        if let EntrySource::Bitmap(fields) = &self.source {
            return self.emit_bitmap(slot, fields, data_source, config, value_sink, field_path);
        }

        match self.size {
            None => self.emit_single(slot, data_source, config, value_sink, field_path),
            Some(SizeSource::OneD(size)) => {
                self.emit_1d(slot, data_source, size, config, value_sink, field_path)
            }
            Some(SizeSource::TwoD(size)) => {
                self.emit_2d(slot, data_source, size, config, value_sink, field_path)
            }
        }
    }

    /// Checks a `value` or `const` literal against this leaf's type and size.
    /// Emission relies on these checks for layout-defined literals.
    pub(crate) fn validate_literal(
        &self,
        kind: &str,
        value: &ValueSource,
    ) -> Result<(), LayoutError> {
        let length = match (self.size, value) {
            (Some(SizeSource::TwoD(_)), _) => {
                return Err(LayoutError::InvalidLayout(
                    "2D arrays within the layout file are not supported.".to_owned(),
                ));
            }
            (None, ValueSource::Single(DataValue::Str(_))) => {
                return Err(LayoutError::InvalidLayout(format!(
                    "String {kind} requires size or SIZE; add a size of at least its byte length."
                )));
            }
            (None, ValueSource::Array(_)) => {
                return Err(LayoutError::InvalidLayout(format!(
                    "Array {kind} requires size or SIZE; add a size of at least its length."
                )));
            }
            (None, ValueSource::Single(_)) => return Ok(()),
            (Some(SizeSource::OneD(_)), ValueSource::Single(DataValue::Str(text))) => {
                if !matches!(self.scalar_type, ScalarType::U8 | ScalarType::U16) {
                    return Err(LayoutError::InvalidLayout(
                        "Strings should have type u8 or u16.".to_owned(),
                    ));
                }
                text.len()
            }
            (Some(SizeSource::OneD(_)), ValueSource::Single(_)) => {
                return Err(LayoutError::InvalidLayout(format!(
                    "size/SIZE keys are forbidden with scalar {kind}; remove size/SIZE or use an array or string {kind}."
                )));
            }
            (Some(SizeSource::OneD(_)), ValueSource::Array(values)) => {
                if values
                    .iter()
                    .any(|value| matches!(value, DataValue::Str(_)))
                {
                    return Err(LayoutError::DataValueExportFailed(
                        "Cannot convert string to scalar type.".to_owned(),
                    ));
                }
                values.len()
            }
        };
        let Some(SizeSource::OneD(size)) = self.size else {
            unreachable!("only one-dimensional literals reach the length check")
        };
        self.check_1d_length(length, size)
    }

    fn check_1d_length(&self, length: usize, size: usize) -> Result<(), LayoutError> {
        if length > size {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is larger than defined size.".to_owned(),
            ));
        }
        if self.strict_size && length < size {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is smaller than defined size (strict SIZE).".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn const_value<'a>(
        name: &str,
        consts: &'a HashMap<String, ValueSource>,
    ) -> Result<&'a ValueSource, LayoutError> {
        if name.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Const name must not be empty.".into(),
            ));
        }
        let value = consts.get(name).ok_or_else(|| {
            let mut available = consts.keys().map(String::as_str).collect::<Vec<_>>();
            available.sort_unstable();
            LayoutError::DataValueExportFailed(format!(
                "Const '{}' not found in [mint.const]. Available: [{}]",
                name,
                available.join(", ")
            ))
        })?;
        Ok(value)
    }

    /// Returns a layout-defined literal; its shape was checked by static validation.
    fn literal<'a>(
        &'a self,
        consts: &'a HashMap<String, ValueSource>,
    ) -> Result<&'a ValueSource, LayoutError> {
        match &self.source {
            EntrySource::Value(value) => Ok(value),
            EntrySource::Const(name) => Self::const_value(name, consts),
            _ => unreachable!("only value and const sources are literals"),
        }
    }

    pub fn validate_ref(&self, source: &RefSource) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Ref", self.scalar_type));
        }
        if !matches!(
            self.scalar_type,
            ScalarType::U16 | ScalarType::U32 | ScalarType::U64
        ) {
            return Err(LayoutError::InvalidLayout(
                "Ref requires unsigned integer storage type (u16, u32, u64).".into(),
            ));
        }

        let (size, strict_len) = (self.size, self.strict_size);
        match (source, size) {
            (RefSource::Scalar(_), None) => {}
            (RefSource::Scalar(_), Some(_)) => {
                return Err(LayoutError::InvalidLayout(
                    "size/SIZE keys require a ref list, not a scalar ref.".into(),
                ));
            }
            (RefSource::List(_), None) => {
                return Err(LayoutError::InvalidLayout(
                    "Ref list requires a one-dimensional size or SIZE.".into(),
                ));
            }
            (RefSource::List(_), Some(SizeSource::TwoD(_))) => {
                return Err(LayoutError::InvalidLayout(
                    "Ref list supports only one-dimensional size/SIZE.".into(),
                ));
            }
            (RefSource::List(targets), Some(SizeSource::OneD(capacity))) => {
                if targets.len() > capacity {
                    return Err(LayoutError::InvalidLayout(format!(
                        "Ref list has {} entries, which exceeds its declared size ({capacity}).",
                        targets.len()
                    )));
                }
                if strict_len && targets.len() < capacity {
                    return Err(LayoutError::InvalidLayout(format!(
                        "Ref list has {} entries, which is smaller than its declared strict SIZE ({capacity}).",
                        targets.len()
                    )));
                }
            }
        }

        for (index, target) in source.targets().iter().enumerate() {
            if matches!(target, RefTarget::Path(path) if path.is_empty()) {
                let location = if source.is_list() {
                    format!(" at index {index}")
                } else {
                    String::new()
                };
                return Err(LayoutError::InvalidLayout(format!(
                    "Ref target path{location} must not be empty."
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_checksum_storage(&self) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Checksum", self.scalar_type));
        }
        if self.size.is_some() {
            return Err(LayoutError::InvalidLayout(
                "size/SIZE keys are forbidden with checksum.".into(),
            ));
        }
        if !matches!(self.scalar_type, ScalarType::U32) {
            return Err(LayoutError::InvalidLayout(format!(
                "Checksum type must be u32 (4 bytes), got {} ({} bytes).",
                self.scalar_type.name(),
                self.scalar_type.size_bytes()
            )));
        }
        Ok(())
    }

    pub fn validate_fingerprint(&self) -> Result<(), LayoutError> {
        if self.size.is_some() {
            return Err(LayoutError::InvalidLayout(
                "size/SIZE keys are forbidden with fingerprint.".into(),
            ));
        }
        if !matches!(self.scalar_type, ScalarType::U64) {
            return Err(LayoutError::InvalidLayout(format!(
                "Fingerprint type must be u64 (8 bytes), got {} ({} bytes).",
                self.scalar_type.name(),
                self.scalar_type.size_bytes()
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_bitmap(
        &self,
        fields: &[BitmapField],
        scalar_abi: ScalarAbi,
    ) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Bitmap", self.scalar_type));
        }
        if self.size.is_some() {
            return Err(LayoutError::InvalidLayout(
                "size/SIZE keys are forbidden with bitmap.".into(),
            ));
        }

        if !self.scalar_type.is_integer() {
            return Err(LayoutError::InvalidLayout(
                "Bitmap requires integer storage type.".into(),
            ));
        }

        let expected_bits = scalar_abi.storage_size * 8;
        let mut total_bits = 0usize;
        for field in fields {
            if field.bits == 0 {
                return Err(LayoutError::InvalidLayout(
                    "Bitmap field bits must be > 0.".into(),
                ));
            }
            if field.bits > expected_bits {
                return Err(LayoutError::InvalidLayout(format!(
                    "Bitmap field bits ({}) exceed storage width ({}).",
                    field.bits, expected_bits
                )));
            }
            total_bits += field.bits;
        }

        if total_bits != expected_bits {
            return Err(LayoutError::InvalidLayout(format!(
                "Bitmap total bits ({}) must equal storage width ({}).",
                total_bits, expected_bits
            )));
        }

        Ok(())
    }

    fn emit_bitmap(
        &self,
        slot: &mut Slot<'_>,
        fields: &[BitmapField],
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut ValueCollector,
        field_path: &str,
    ) -> Result<(), LayoutError> {
        let signed = self.scalar_type.is_signed();
        let mut accumulator: u128 = 0;
        let mut offset: usize = 0;
        for field in fields {
            let value = field.resolve_value(data_source)?;
            let clamped = clamp_bitfield_value(&value, field.bits, signed, config.strict)?;

            let mask = (1u128 << field.bits) - 1;
            let pattern = (clamped as u128) & mask;
            accumulator |= pattern << offset;

            value_sink.record_member_value(
                field_path,
                || bitmap_field_key(field, offset),
                || i128_to_json(clamped),
            )?;

            offset += field.bits;
        }

        // Bitmap storage is an integer type, so its width is 1, 2, 4 or 8 octets.
        let endianness = config.abi.endianness();
        slot.write_scalar(match slot.abi.storage_size {
            1 => (accumulator as u8).to_endian_bytes(endianness),
            2 => (accumulator as u16).to_endian_bytes(endianness),
            4 => (accumulator as u32).to_endian_bytes(endianness),
            _ => (accumulator as u64).to_endian_bytes(endianness),
        });
        Ok(())
    }

    fn emit_single(
        &self,
        slot: &mut Slot<'_>,
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut ValueCollector,
        field_path: &str,
    ) -> Result<(), LayoutError> {
        match &self.source {
            EntrySource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                let value = ds.retrieve_single_value(name)?;
                let encoded =
                    value.encode(self.scalar_type, config.abi.endianness(), config.strict)?;
                value_sink.record_value(field_path, || data_value_to_json(&value))?;
                slot.write_scalar(encoded);
                Ok(())
            }
            EntrySource::Value(_) | EntrySource::Const(_) => {
                let ValueSource::Single(v) = self.literal(config.consts)? else {
                    unreachable!("unsized literal shape is validated before emission")
                };
                let encoded = v.encode(self.scalar_type, config.abi.endianness(), config.strict)?;
                value_sink.record_value(field_path, || data_value_to_json(v))?;
                slot.write_scalar(encoded);
                Ok(())
            }
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_into"),
            EntrySource::Ref(_) => unreachable!("ref handled by block emitter"),
            EntrySource::Checksum(_) => unreachable!("checksum handled by block emitter"),
            EntrySource::Fingerprint(_) => {
                unreachable!("fingerprint handled by block emitter")
            }
        }
    }

    fn emit_1d(
        &self,
        slot: &mut Slot<'_>,
        data_source: Option<&dyn DataSource>,
        size: usize,
        config: &BuildConfig,
        value_sink: &mut ValueCollector,
        field_path: &str,
    ) -> Result<(), LayoutError> {
        let retrieved;
        let value = match &self.source {
            EntrySource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                retrieved = ds.retrieve_1d_array_or_string(name)?;
                &retrieved
            }
            EntrySource::Value(_) | EntrySource::Const(_) => self.literal(config.consts)?,
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_into"),
            EntrySource::Ref(_) => unreachable!("ref handled by block emitter"),
            EntrySource::Checksum(_) => unreachable!("checksum handled by block emitter"),
            EntrySource::Fingerprint(_) => {
                unreachable!("fingerprint handled by block emitter")
            }
        };

        let length = match value {
            ValueSource::Array(values) => values.len(),
            ValueSource::Single(DataValue::Str(value)) => value.len(),
            _ => {
                return Err(LayoutError::DataValueExportFailed(
                    "Array or string expected for a field with size/SIZE.".into(),
                ));
            }
        };
        self.check_1d_length(length, size)?;

        match value {
            ValueSource::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    let encoded =
                        value.encode(self.scalar_type, config.abi.endianness(), config.strict)?;
                    slot.write_element(index, encoded);
                }
                value_sink.record_value(field_path, || array_to_json(values))?;
            }
            ValueSource::Single(value) => {
                if !matches!(self.scalar_type, ScalarType::U8 | ScalarType::U16) {
                    return Err(LayoutError::DataValueExportFailed(
                        "Strings should have type u8 or u16.".to_owned(),
                    ));
                }
                let elements = value.string_elements(self.scalar_type, config.abi.endianness())?;
                for (index, encoded) in elements.enumerate() {
                    slot.write_element(index, encoded);
                }
                value_sink.record_value(field_path, || data_value_to_json(value))?;
            }
        }
        Ok(())
    }

    fn emit_2d(
        &self,
        slot: &mut Slot<'_>,
        data_source: Option<&dyn DataSource>,
        size: [usize; 2],
        config: &BuildConfig,
        value_sink: &mut ValueCollector,
        field_path: &str,
    ) -> Result<(), LayoutError> {
        let EntrySource::Name(name) = &self.source else {
            unreachable!("2D source validated before block emission")
        };
        let Some(ds) = data_source else {
            return Err(LayoutError::MissingDataSheet(format!(
                "Field '{}' requires a value from a data source, but none was provided.",
                name
            )));
        };
        let data = ds.retrieve_2d_array(name)?;

        let rows = size[0];
        let cols = size[1];

        if data.iter().any(|row| row.len() != cols) {
            return Err(LayoutError::DataValueExportFailed(
                "2D array column count mismatch.".to_owned(),
            ));
        }

        if data.len() > rows {
            return Err(LayoutError::DataValueExportFailed(
                "2D array row count greater than defined size.".to_owned(),
            ));
        }

        if self.strict_size && data.len() < rows {
            return Err(LayoutError::DataValueExportFailed(
                "2D array row count smaller than defined size (strict SIZE).".to_owned(),
            ));
        }

        for (index, v) in data.iter().flatten().enumerate() {
            let encoded = v.encode(self.scalar_type, config.abi.endianness(), config.strict)?;
            slot.write_element(index, encoded);
        }
        value_sink.record_value(field_path, || array_2d_to_json(&data))?;
        Ok(())
    }
}

/// One leaf's resolved octets in the block buffer, already filled with the
/// block's padding byte. Anything a leaf does not write stays padding.
pub(crate) struct Slot<'a> {
    bytes: &'a mut [u8],
    abi: ScalarAbi,
}

impl<'a> Slot<'a> {
    pub(crate) fn new(bytes: &'a mut [u8], abi: ScalarAbi) -> Self {
        Self { bytes, abi }
    }

    /// Writes the scalar that occupies the whole slot.
    pub(crate) fn write_scalar(&mut self, value: EncodedScalar) {
        self.bytes.copy_from_slice(value.as_bytes());
    }

    /// Writes array element `index`; the stride gap after it keeps its padding.
    pub(crate) fn write_element(&mut self, index: usize, value: EncodedScalar) {
        let value = value.as_bytes();
        debug_assert!(
            value.len() == self.abi.storage_size && self.abi.array_stride >= value.len(),
            "encoded scalar width must match its ABI storage size and fit the array stride"
        );
        let start = index * self.abi.array_stride;
        self.bytes[start..start + value.len()].copy_from_slice(value);
    }
}

fn bitmap_field_key(field: &BitmapField, offset: usize) -> String {
    match &field.source {
        BitmapFieldSource::Name(name) => name.clone(),
        BitmapFieldSource::Value(_) => format!("reserved_{}_{}", offset, field.bits),
    }
}
