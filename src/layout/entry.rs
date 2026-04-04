use super::block::BuildConfig;
use super::conversions::clamp_bitfield_value;
use super::error::LayoutError;
use super::settings::MintConfig;
use super::used_values::{
    ValueSink, array_2d_to_json, array_to_json, data_value_to_json, i128_to_json,
};
use super::value::{DataValue, ValueSource};
use crate::data::DataSource;
use serde::Deserialize;

/// Leaf entry representing an item to add to the flash block.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafEntry {
    #[serde(rename = "type")]
    pub scalar_type: ScalarType,
    #[serde(flatten, default)]
    size_keys: SizeKeys,
    #[serde(flatten)]
    pub source: EntrySource,
}

/// Scalar type enum derived from 'type' string in leaf entries.
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum ScalarType {
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "u64")]
    U64,
    #[serde(rename = "i8")]
    I8,
    #[serde(rename = "i16")]
    I16,
    #[serde(rename = "i32")]
    I32,
    #[serde(rename = "i64")]
    I64,
    #[serde(rename = "f32")]
    F32,
    #[serde(rename = "f64")]
    F64,
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
            (Some(_), Some(_)) => Err(LayoutError::DataValueExportFailed(
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
}

/// Single bitmap field within a bitmap entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitmapField {
    pub bits: usize,
    #[serde(flatten)]
    pub source: BitmapFieldSource,
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
    /// Returns the alignment of the leaf entry.
    pub fn get_alignment(&self) -> usize {
        self.scalar_type.size_bytes()
    }

    pub fn emit_bytes(
        &self,
        data_source: Option<&dyn DataSource>,
        config: &BuildConfig,
        value_sink: &mut dyn ValueSink,
        field_path: &[String],
    ) -> Result<Vec<u8>, LayoutError> {
        if config.word_addressing && matches!(self.scalar_type, ScalarType::U8 | ScalarType::I8) {
            return Err(LayoutError::DataValueExportFailed(
                "u8/i8 types are not supported with word_addressing enabled.".into(),
            ));
        }

        if let EntrySource::Ref(_) = &self.source {
            return Err(LayoutError::DataValueExportFailed(
                "Ref entries are resolved in a fixup pass, not via emit_bytes.".into(),
            ));
        }

        if let EntrySource::Checksum(_) = &self.source {
            return Err(LayoutError::DataValueExportFailed(
                "Checksum entries are resolved in a fixup pass, not via emit_bytes.".into(),
            ));
        }

        if let EntrySource::Bitmap(fields) = &self.source {
            self.validate_bitmap(fields)?;
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

    /// Validates ref entry rules.
    pub fn validate_ref(&self, target: &str) -> Result<(), LayoutError> {
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
            return Err(LayoutError::DataValueExportFailed(
                "size/SIZE keys are forbidden with ref.".into(),
            ));
        }
        if !self.scalar_type.is_integer() {
            return Err(LayoutError::DataValueExportFailed(
                "Ref requires integer storage type.".into(),
            ));
        }
        if target.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Ref target path must not be empty.".into(),
            ));
        }
        Ok(())
    }

    /// Validates checksum entry rules.
    pub fn validate_checksum(
        &self,
        config_name: &str,
        settings: &MintConfig,
    ) -> Result<(), LayoutError> {
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
            return Err(LayoutError::DataValueExportFailed(
                "size/SIZE keys are forbidden with checksum.".into(),
            ));
        }
        if config_name.is_empty() {
            return Err(LayoutError::DataValueExportFailed(
                "Checksum config name must not be empty.".into(),
            ));
        }
        if !settings.checksum.contains_key(config_name) {
            let available = settings
                .checksum
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(LayoutError::DataValueExportFailed(format!(
                "Checksum config '{}' not found in [mint.checksum]. Available: [{}]",
                config_name, available
            )));
        }
        // Validate type matches CRC output width (4 bytes for CRC-32)
        if self.scalar_type.size_bytes() != 4 {
            return Err(LayoutError::DataValueExportFailed(format!(
                "Checksum type must be u32 (4 bytes), got {} ({} bytes).",
                self.scalar_type.name(),
                self.scalar_type.size_bytes()
            )));
        }
        Ok(())
    }

    /// Validates bitmap entry rules.
    fn validate_bitmap(&self, fields: &[BitmapField]) -> Result<(), LayoutError> {
        if self.size_keys.size.is_some() || self.size_keys.strict_size.is_some() {
            return Err(LayoutError::DataValueExportFailed(
                "size/SIZE keys are forbidden with bitmap.".into(),
            ));
        }

        if !self.scalar_type.is_integer() {
            return Err(LayoutError::DataValueExportFailed(
                "Bitmap requires integer storage type.".into(),
            ));
        }

        let mut total_bits = 0usize;
        for field in fields {
            if field.bits == 0 {
                return Err(LayoutError::DataValueExportFailed(
                    "Bitmap field bits must be > 0.".into(),
                ));
            }
            total_bits += field.bits;
        }

        let expected_bits = self.scalar_type.size_bytes() * 8;
        if total_bits != expected_bits {
            return Err(LayoutError::DataValueExportFailed(format!(
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

        DataValue::U64(accumulator as u64).to_bytes(self.scalar_type, config.endianness, false)
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
                value_sink.record_value(field_path, data_value_to_json(&value)?)?;
                value.to_bytes(self.scalar_type, config.endianness, config.strict)
            }
            EntrySource::Value(ValueSource::Single(v)) => {
                value_sink.record_value(field_path, data_value_to_json(v)?)?;
                v.to_bytes(self.scalar_type, config.endianness, config.strict)
            }
            EntrySource::Value(_) => Err(LayoutError::DataValueExportFailed(
                "Single value expected for scalar type.".to_string(),
            )),
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled in build_bytestream"),
            EntrySource::Checksum(_) => unreachable!("checksum handled in build_bytestream"),
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
                                "Strings should have type u8.".to_string(),
                            ));
                        }
                        value_sink.record_value(field_path, data_value_to_json(&v)?)?;
                        out.extend(v.string_to_bytes()?);
                    }
                    ValueSource::Array(v) => {
                        value_sink.record_value(field_path, array_to_json(&v)?)?;
                        for v in v {
                            out.extend(v.to_bytes(
                                self.scalar_type,
                                config.endianness,
                                config.strict,
                            )?);
                        }
                    }
                }
            }
            EntrySource::Value(ValueSource::Array(v)) => {
                value_sink.record_value(field_path, array_to_json(v)?)?;
                for v in v {
                    out.extend(v.to_bytes(self.scalar_type, config.endianness, config.strict)?);
                }
            }
            EntrySource::Value(ValueSource::Single(v)) => {
                if !matches!(self.scalar_type, ScalarType::U8) {
                    return Err(LayoutError::DataValueExportFailed(
                        "Strings should have type u8.".to_string(),
                    ));
                }
                value_sink.record_value(field_path, data_value_to_json(v)?)?;
                out.extend(v.string_to_bytes()?);
            }
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled in build_bytestream"),
            EntrySource::Checksum(_) => unreachable!("checksum handled in build_bytestream"),
        }

        if out.len() > total_bytes {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is larger than defined size.".to_string(),
            ));
        }
        if strict_len && out.len() < total_bytes {
            return Err(LayoutError::DataValueExportFailed(
                "Array/string is smaller than defined size (strict SIZE).".to_string(),
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
                        "2D array column count mismatch.".to_string(),
                    ));
                }

                if data.len() > rows {
                    return Err(LayoutError::DataValueExportFailed(
                        "2D array row count greater than defined size.".to_string(),
                    ));
                }

                if strict_len && data.len() < rows {
                    return Err(LayoutError::DataValueExportFailed(
                        "2D array row count smaller than defined size (strict SIZE).".to_string(),
                    ));
                }

                value_sink.record_value(field_path, array_2d_to_json(&data)?)?;

                let mut out = Vec::with_capacity(total_bytes);
                for row in data {
                    for v in row {
                        out.extend(v.to_bytes(
                            self.scalar_type,
                            config.endianness,
                            config.strict,
                        )?);
                    }
                }

                while out.len() < total_bytes {
                    out.push(config.padding);
                }

                Ok(out)
            }
            EntrySource::Value(_) => Err(LayoutError::DataValueExportFailed(
                "2D arrays within the layout file are not supported.".to_string(),
            )),
            EntrySource::Bitmap(_) => unreachable!("bitmap handled in emit_bytes"),
            EntrySource::Ref(_) => unreachable!("ref handled in build_bytestream"),
            EntrySource::Checksum(_) => unreachable!("checksum handled in build_bytestream"),
        }
    }
}

fn bitmap_field_key(field: &BitmapField, offset: usize) -> String {
    match &field.source {
        BitmapFieldSource::Name(name) => name.clone(),
        BitmapFieldSource::Value(_) => format!("reserved_{}_{}", offset, field.bits),
    }
}

impl ScalarType {
    /// Returns the size of the scalar type in bytes.
    pub fn size_bytes(&self) -> usize {
        match self {
            ScalarType::U8 | ScalarType::I8 => 1,
            ScalarType::U16 | ScalarType::I16 => 2,
            ScalarType::U32 | ScalarType::I32 | ScalarType::F32 => 4,
            ScalarType::U64 | ScalarType::I64 | ScalarType::F64 => 8,
        }
    }

    /// Returns true if this is an integer type (not floating-point).
    pub fn is_integer(&self) -> bool {
        !matches!(self, ScalarType::F32 | ScalarType::F64)
    }

    /// Returns the type name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            ScalarType::U8 => "u8",
            ScalarType::U16 => "u16",
            ScalarType::U32 => "u32",
            ScalarType::U64 => "u64",
            ScalarType::I8 => "i8",
            ScalarType::I16 => "i16",
            ScalarType::I32 => "i32",
            ScalarType::I64 => "i64",
            ScalarType::F32 => "f32",
            ScalarType::F64 => "f64",
        }
    }

    /// Returns true if this is a signed type.
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            ScalarType::I8 | ScalarType::I16 | ScalarType::I32 | ScalarType::I64
        )
    }
}
