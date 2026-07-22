use super::abi::{EndianBytes, Endianness};
use super::conversions::convert_value_to_bytes;
use super::error::LayoutError;
use super::scalar_type::ScalarType;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ValueSource {
    Single(DataValue),
    Array(Vec<DataValue>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DataValue {
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    Str(String),
}

impl DataValue {
    pub fn to_bytes(
        &self,
        scalar_type: ScalarType,
        endianness: Endianness,
        strict: bool,
    ) -> Result<Vec<u8>, LayoutError> {
        convert_value_to_bytes(self, scalar_type, endianness, strict)
    }

    pub fn string_to_bytes(
        &self,
        scalar_type: ScalarType,
        endianness: Endianness,
    ) -> Result<Vec<u8>, LayoutError> {
        match self {
            DataValue::Str(val) => match scalar_type {
                ScalarType::U8 => Ok(val.as_bytes().to_vec()),
                ScalarType::U16 => Ok(val
                    .as_bytes()
                    .iter()
                    .flat_map(|byte| u16::from(*byte).to_endian_bytes(endianness))
                    .collect()),
                _ => Err(LayoutError::DataValueExportFailed(
                    "Strings should have type u8 or u16.".to_owned(),
                )),
            },
            _ => Err(LayoutError::DataValueExportFailed(
                "String expected for string type.".to_owned(),
            )),
        }
    }
}
