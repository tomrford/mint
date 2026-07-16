use super::block::BuildConfig;
use super::conversions::clamp_bitfield_value;
use super::error::LayoutError;
use super::scalar_type::{ScalarType, fixed_point_unsupported_error};
use super::settings::Endianness;
use super::used_values::{
    ValueSink, array_2d_to_json, array_to_json, data_value_to_json, i128_to_json,
};
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

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

/// Leaf entry representing an item to add to the flash block.
#[derive(Debug)]
pub struct LeafEntry {
    pub scalar_type: ScalarType,
    size_keys: SizeKeys,
    pub source: EntrySource,
}

#[derive(Deserialize)]
struct RawLeafEntry {
    #[serde(rename = "type")]
    scalar_type: ScalarType,
    #[serde(flatten, default)]
    size_keys: SizeKeys,
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
            size_keys: raw.size_keys,
            source: raw.source,
        })
    }
}

/// Size source enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SizeSource {
    OneD(usize),
    TwoD([usize; 2]),
}

/// Helper struct to capture both 'size' and 'SIZE' keys.
#[derive(Debug, Default, Deserialize)]
struct SizeKeys {
    #[serde(rename = "size")]
    size: Option<SizeSource>,
    #[serde(rename = "SIZE")]
    strict_size: Option<SizeSource>,
}

impl SizeKeys {
    fn resolve(&self) -> Result<(Option<SizeSource>, bool), LayoutError> {
        match (&self.size, &self.strict_size) {
            (Some(_), Some(_)) => Err(LayoutError::InvalidLayout(
                "Use either 'size' or 'SIZE', not both.".into(),
            )),
            (Some(s), None) => Ok((Some(s.clone()), false)),
            (None, Some(s)) => Ok((Some(s.clone()), true)),
            (None, None) => Ok((None, false)),
        }
    }
}

/// Mutually exclusive source enum.
#[derive(Debug, Deserialize)]
pub enum EntrySource {
    #[serde(rename = "name")]
    Name(String),
    #[serde(rename = "value")]
    Value(ValueSource),
    #[serde(rename = "bitmap")]
    Bitmap(Vec<BitmapField>),
    #[serde(rename = "ref")]
    Ref(String),
    #[serde(rename = "checksum")]
    Checksum(String),
    #[serde(rename = "const")]
    Const(String),
    #[serde(rename = "fingerprint")]
    Fingerprint(FingerprintTarget),
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

/// Single bitmap field within a bitmap entry.
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

/// Source for a bitmap field (no arrays allowed).
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
    pub(crate) fn size(&self) -> Result<Option<SizeSource>, LayoutError> {
        self.size_keys.resolve().map(|(size, _)| size)
    }

    /// Returns the alignment of the leaf entry.
    pub fn get_alignment(&self) -> usize {
        self.scalar_type.size_bytes()
    }

    pub(crate) fn emit_bytes(
        &self,
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        if let EntrySource::Bitmap(fields) = &self.source {
            return self.emit_bitmap(fields, data_source, config, value_sink, field_path);
        }

        let (size, strict_len) = self.size_keys.resolve()?;
        match size {
            None => self.emit_bytes_single(data_source, config, value_sink, field_path),
            Some(SizeSource::OneD(size)) => self.emit_bytes_1d(
                data_source,
                size,
                config,
                strict_len,
                value_sink,
                field_path,
            ),
            Some(SizeSource::TwoD(size)) => self.emit_bytes_2d(
                data_source,
                size,
                config,
                strict_len,
                value_sink,
                field_path,
            ),
        }
    }

    /// Validates const entry rules and returns the resolved const value.
    pub(crate) fn validate_const<'a>(
        &self,
        name: &str,
        config: &'a BuildConfig<'a>,
        size: Option<&SizeSource>,
    ) -> Result<&'a ValueSource, LayoutError> {
        if name.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Const name must not be empty.".into(),
            ));
        }
        let value = config.consts.get(name).ok_or_else(|| {
            let available = config.consts.keys().cloned().collect::<Vec<_>>().join(", ");
            LayoutError::DataValueExportFailed(format!(
                "Const '{}' not found in [mint.const]. Available: [{}]",
                name, available
            ))
        })?;
        match (size, value) {
            (Some(SizeSource::TwoD(_)), _) => {
                return Err(LayoutError::DataValueExportFailed(
                    "2D arrays within the layout file are not supported.".to_owned(),
                ));
            }
            (Some(SizeSource::OneD(_)), ValueSource::Single(DataValue::Str(_))) => {}
            (Some(SizeSource::OneD(_)), ValueSource::Array(_)) => {}
            (Some(SizeSource::OneD(_)), ValueSource::Single(_)) => {
                return Err(LayoutError::DataValueExportFailed(
                    "size/SIZE keys are forbidden with scalar const.".into(),
                ));
            }
            (None, _) => {}
        }
        Ok(value)
    }

    /// Validates ref entry rules.
    pub fn validate_ref(&self, target: &str) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Ref", self.scalar_type));
        }
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
            return Err(LayoutError::InvalidLayout(
                "size/SIZE keys are forbidden with ref.".into(),
            ));
        }
        if !matches!(
            self.scalar_type,
            ScalarType::U16 | ScalarType::U32 | ScalarType::U64
        ) {
            return Err(LayoutError::InvalidLayout(
                "Ref requires unsigned integer storage type (u16, u32, u64).".into(),
            ));
        }
        if target.is_empty() {
            return Err(LayoutError::InvalidLayout(
                "Ref target path must not be empty.".into(),
            ));
        }
        Ok(())
    }

    /// Validates checksum entry rules.
    pub(crate) fn validate_checksum_storage(&self) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Checksum", self.scalar_type));
        }
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
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
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
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

    /// Validates bitmap entry rules.
    pub(crate) fn validate_bitmap(&self, fields: &[BitmapField]) -> Result<(), LayoutError> {
        if self.scalar_type.fixed_point().is_some() {
            return Err(fixed_point_unsupported_error("Bitmap", self.scalar_type));
        }
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
            return Err(LayoutError::InvalidLayout(
                "size/SIZE keys are forbidden with bitmap.".into(),
            ));
        }

        if !self.scalar_type.is_integer() {
            return Err(LayoutError::InvalidLayout(
                "Bitmap requires integer storage type.".into(),
            ));
        }

        let expected_bits = self.scalar_type.size_bytes() * 8;
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

    /// Emits bytes for a bitmap entry. Validation must be called first.
    fn emit_bitmap(
        &self,
        fields: &[BitmapField],
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        let signed = self.scalar_type.is_signed();
        let mut accumulator: u128 = 0;
        let mut offset: usize = 0;
        for field in fields {
            let value = field.resolve_value(data_source)?;
            let clamped = clamp_bitfield_value(&value, field.bits, signed, config.strict)?;

            let mask = (1u128 << field.bits) - 1;
            let pattern = (clamped as u128) & mask;
            accumulator |= pattern << offset;

            let mut bitmap_path = field_path.to_vec();
            bitmap_path.push(bitmap_field_key(field, offset));
            value_sink.record_value(&bitmap_path, i128_to_json(clamped)?)?;

            offset += field.bits;
        }

        encode_bitmap_storage(accumulator, self.scalar_type, config.endianness)
    }

    fn emit_bytes_single(
        &self,
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        match &self.source {
            EntrySource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                let value = ds.retrieve_single_value(name)?;
                let bytes = value.to_bytes(self.scalar_type, config.endianness, config.strict)?;
                value_sink.record_value(field_path, data_value_to_json(&value)?)?;
                Ok(bytes)
            }
            EntrySource::Value(ValueSource::Single(v)) => {
                let bytes = v.to_bytes(self.scalar_type, config.endianness, config.strict)?;
                value_sink.record_value(field_path, data_value_to_json(v)?)?;
                Ok(bytes)
            }
            EntrySource::Value(_) => Err(LayoutError::DataValueExportFailed(
                "Single value expected for scalar type.".to_owned(),
            )),
            EntrySource::Const(name) => match self.validate_const(name, config, None)? {
                ValueSource::Single(v) => {
                    let bytes = v.to_bytes(self.scalar_type, config.endianness, config.strict)?;
                    value_sink.record_value(field_path, data_value_to_json(v)?)?;
                    Ok(bytes)
                }
                ValueSource::Array(_) => Err(LayoutError::DataValueExportFailed(
                    "Single value expected for scalar type.".to_owned(),
                )),
            },
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled by block emitter"),
            EntrySource::Checksum(_) => unreachable!("checksum handled by block emitter"),
            EntrySource::Fingerprint(_) => {
                unreachable!("fingerprint handled by block emitter")
            }
        }
    }

    fn emit_bytes_1d(
        &self,
        data_source: Option<&dyn DataSource>,
        size: usize,
        config: &BuildConfig,
        strict_len: bool,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        let elem = self.scalar_type.size_bytes();
        let total_bytes = size
            .checked_mul(elem)
            .ok_or(LayoutError::DataValueExportFailed(
                "Array size overflow".into(),
            ))?;
        let mut out = Vec::with_capacity(total_bytes);

        match &self.source {
            EntrySource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                match ds.retrieve_1d_array_or_string(name)? {
                    ValueSource::Single(v) => {
                        if !matches!(self.scalar_type, ScalarType::U8) {
                            return Err(LayoutError::DataValueExportFailed(
                                "Strings should have type u8.".to_owned(),
                            ));
                        }
                        out.extend(v.string_to_bytes()?);
                        value_sink.record_value(field_path, data_value_to_json(&v)?)?;
                    }
                    ValueSource::Array(v) => {
                        for value in &v {
                            out.extend(value.to_bytes(
                                self.scalar_type,
                                config.endianness,
                                config.strict,
                            )?);
                        }
                        value_sink.record_value(field_path, array_to_json(&v)?)?;
                    }
                }
            }
            EntrySource::Value(ValueSource::Array(v)) => {
                for value in v {
                    out.extend(value.to_bytes(
                        self.scalar_type,
                        config.endianness,
                        config.strict,
                    )?);
                }
                value_sink.record_value(field_path, array_to_json(v)?)?;
            }
            EntrySource::Value(ValueSource::Single(v)) => {
                if !matches!(self.scalar_type, ScalarType::U8) {
                    return Err(LayoutError::DataValueExportFailed(
                        "Strings should have type u8.".to_owned(),
                    ));
                }
                out.extend(v.string_to_bytes()?);
                value_sink.record_value(field_path, data_value_to_json(v)?)?;
            }
            EntrySource::Const(name) => {
                match self.validate_const(name, config, Some(&SizeSource::OneD(size)))? {
                    ValueSource::Array(v) => {
                        for value in v {
                            out.extend(value.to_bytes(
                                self.scalar_type,
                                config.endianness,
                                config.strict,
                            )?);
                        }
                        value_sink.record_value(field_path, array_to_json(v)?)?;
                    }
                    ValueSource::Single(v) => {
                        if !matches!(self.scalar_type, ScalarType::U8) {
                            return Err(LayoutError::DataValueExportFailed(
                                "Strings should have type u8.".to_owned(),
                            ));
                        }
                        out.extend(v.string_to_bytes()?);
                        value_sink.record_value(field_path, data_value_to_json(v)?)?;
                    }
                }
            }
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled by block emitter"),
            EntrySource::Checksum(_) => unreachable!("checksum handled by block emitter"),
            EntrySource::Fingerprint(_) => {
                unreachable!("fingerprint handled by block emitter")
            }
        }

        if out.len() > total_bytes {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is larger than defined size.".to_owned(),
            ));
        }
        if strict_len && out.len() < total_bytes {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is smaller than defined size (strict SIZE).".to_owned(),
            ));
        }
        while out.len() < total_bytes {
            out.push(config.padding);
        }
        Ok(out)
    }

    fn emit_bytes_2d(
        &self,
        data_source: Option<&dyn DataSource>,
        size: [usize; 2],
        config: &BuildConfig,
        strict_len: bool,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        match &self.source {
            EntrySource::Name(name) => {
                let Some(ds) = data_source else {
                    return Err(LayoutError::MissingDataSheet(format!(
                        "Field '{}' requires a value from a data source, but none was provided.",
                        name
                    )));
                };
                let data = ds.retrieve_2d_array(name)?;

                let rows = size[0];
                let cols = size[1];

                let elem = self.scalar_type.size_bytes();
                let total_elems =
                    rows.checked_mul(cols)
                        .ok_or(LayoutError::DataValueExportFailed(
                            "2D size overflow".into(),
                        ))?;
                let total_bytes =
                    total_elems
                        .checked_mul(elem)
                        .ok_or(LayoutError::DataValueExportFailed(
                            "2D byte count overflow".into(),
                        ))?;

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

                if strict_len && data.len() < rows {
                    return Err(LayoutError::DataValueExportFailed(
                        "2D array row count smaller than defined size (strict SIZE).".to_owned(),
                    ));
                }

                let mut out = Vec::with_capacity(total_bytes);
                for row in &data {
                    for v in row {
                        out.extend(v.to_bytes(
                            self.scalar_type,
                            config.endianness,
                            config.strict,
                        )?);
                    }
                }
                value_sink.record_value(field_path, array_2d_to_json(&data)?)?;

                while out.len() < total_bytes {
                    out.push(config.padding);
                }

                Ok(out)
            }
            EntrySource::Value(_) => Err(LayoutError::DataValueExportFailed(
                "2D arrays within the layout file are not supported.".to_owned(),
            )),
            EntrySource::Const(name) => {
                self.validate_const(name, config, Some(&SizeSource::TwoD(size)))?;
                Err(LayoutError::DataValueExportFailed(
                    "2D arrays within the layout file are not supported.".to_owned(),
                ))
            }
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled by block emitter"),
            EntrySource::Checksum(_) => unreachable!("checksum handled by block emitter"),
            EntrySource::Fingerprint(_) => {
                unreachable!("fingerprint handled by block emitter")
            }
        }
    }
}

fn encode_bitmap_storage(
    accumulator: u128,
    scalar_type: ScalarType,
    endianness: &Endianness,
) -> Result<Vec<u8>, LayoutError> {
    if !scalar_type.is_integer() {
        return Err(LayoutError::DataValueExportFailed(
            "Bitmap requires integer storage type.".into(),
        ));
    }

    let width = scalar_type.size_bytes();
    let bytes = match endianness {
        Endianness::Little => (accumulator as u64).to_le_bytes()[..width].to_vec(),
        Endianness::Big => (accumulator as u64).to_be_bytes()[8 - width..].to_vec(),
    };
    Ok(bytes)
}

fn bitmap_field_key(field: &BitmapField, offset: usize) -> String {
    match &field.source {
        BitmapFieldSource::Name(name) => name.clone(),
        BitmapFieldSource::Value(_) => format!("reserved_{}_{}", offset, field.bits),
    }
}
