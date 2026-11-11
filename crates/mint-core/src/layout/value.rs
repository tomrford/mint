use super::abi::{EncodedScalar, EndianBytes, Endianness};
use super::conversions::encode_scalar;
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
        self.encode(scalar_type, endianness, strict)
            .map(|encoded| encoded.as_bytes().to_vec())
    }

    pub(crate) fn encode(
        &self,
        scalar_type: ScalarType,
        endianness: Endianness,
        strict: bool,
    ) -> Result<EncodedScalar, LayoutError> {
        encode_scalar(self, scalar_type, endianness, strict)
    }

    pub fn string_to_bytes(
        &self,
        scalar_type: ScalarType,
        endianness: Endianness,
    ) -> Result<Vec<u8>, LayoutError> {
        let mut bytes = Vec::new();
        for element in self.string_elements(scalar_type, endianness)? {
            bytes.extend_from_slice(element.as_bytes());
        }
        Ok(bytes)
    }

    /// Encodes each string byte as one `u8` or `u16` array element.
    pub(crate) fn string_elements(
        &self,
        scalar_type: ScalarType,
        endianness: Endianness,
    ) -> Result<impl Iterator<Item = EncodedScalar> + '_, LayoutError> {
        match self {
            DataValue::Str(val) => {
                let wide = match scalar_type {
                    ScalarType::U8 => false,
                    ScalarType::U16 => true,
                    _ => {
                        return Err(LayoutError::DataValueExportFailed(
                            "Strings should have type u8 or u16.".to_owned(),
                        ));
                    }
                };
                Ok(val.bytes().map(move |byte| {
                    if wide {
                        u16::from(byte).to_endian_bytes(endianness)
                    } else {
                        byte.to_endian_bytes(endianness)
                    }
                }))
            }
            _ => Err(LayoutError::DataValueExportFailed(
                "String expected for string type.".to_owned(),
            )),
        }
    }
}
