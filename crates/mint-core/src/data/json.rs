use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use super::DataSource;
use super::error::DataError;
use crate::layout::value::{DataValue, ValueSource};

/// Shared JSON-based data source that reads variant data from JSON objects.
/// Result: `Vec<HashMap<String, Value>>` in variant priority order.
pub struct JsonDataSource {
    variant_columns: Vec<HashMap<String, Value>>,
}

impl JsonDataSource {
    fn new(variant_columns: Vec<HashMap<String, Value>>) -> Self {
        JsonDataSource { variant_columns }
    }

    /// Creates a JSON data source from a JSON object.
    /// Expected format: `{ "VariantName": { "key1": value1, "key2": value2, ... }, ... }`
    pub fn from_value(data: Value, variants: &[String]) -> Result<Self, DataError> {
        let data: HashMap<String, HashMap<String, Value>> = serde_json::from_value(data)
            .map_err(|e| DataError::FileError(format!("failed to parse JSON: {}", e)))?;

        Self::from_variant_map(data, variants)
    }

    /// Creates a JSON data source from JSON text.
    pub fn from_str(json_content: &str, variants: &[String]) -> Result<Self, DataError> {
        let data: HashMap<String, HashMap<String, Value>> = serde_json::from_str(json_content)
            .map_err(|e| DataError::FileError(format!("failed to parse JSON: {}", e)))?;

        Self::from_variant_map(data, variants)
    }

    /// Creates a JSON data source from a JSON file path.
    pub fn from_path(path: impl AsRef<Path>, variants: &[String]) -> Result<Self, DataError> {
        let path = path.as_ref();
        let json_content = std::fs::read_to_string(path).map_err(|_| {
            DataError::FileError(format!("failed to open file: {}", path.display()))
        })?;

        Self::from_str(&json_content, variants)
    }

    fn from_variant_map(
        data: HashMap<String, HashMap<String, Value>>,
        variants: &[String],
    ) -> Result<Self, DataError> {
        let mut variant_columns = Vec::with_capacity(variants.len());

        for variant in variants {
            let map = data
                .get(variant)
                .ok_or_else(|| {
                    DataError::RetrievalError(format!(
                        "variant '{}' not found in JSON data",
                        variant
                    ))
                })?
                .clone();
            variant_columns.push(map);
        }

        Ok(Self::new(variant_columns))
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.variant_columns
            .iter()
            .find_map(|map| map.get(name).filter(|v| !v.is_null()))
    }

    fn value_to_data_value(value: &Value) -> Result<DataValue, DataError> {
        match value {
            Value::Bool(b) => Ok(DataValue::Bool(*b)),
            Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(DataValue::U64(u))
                } else if let Some(i) = n.as_i64() {
                    Ok(DataValue::I64(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(DataValue::F64(f))
                } else {
                    Err(DataError::RetrievalError(
                        "unsupported numeric type".to_owned(),
                    ))
                }
            }
            Value::String(s) => Ok(DataValue::Str(s.clone())),
            _ => Err(DataError::RetrievalError(
                "expected scalar value".to_owned(),
            )),
        }
    }
}

impl DataSource for JsonDataSource {
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any variant".into()))?;

            let dv = Self::value_to_data_value(value)?;
            match dv {
                DataValue::Str(_) => Err(DataError::RetrievalError(
                    "Found non-numeric single value".to_owned(),
                )),
                _ => Ok(dv),
            }
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_owned(),
            source: Box::new(e),
        })
    }

    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any variant".into()))?;

            match value {
                Value::Array(arr) => {
                    let items: Result<Vec<_>, _> =
                        arr.iter().map(Self::value_to_data_value).collect();
                    Ok(ValueSource::Array(items?))
                }
                Value::String(s) => Ok(ValueSource::Single(DataValue::Str(s.clone()))),
                _ => Err(DataError::RetrievalError(
                    "expected array or string for 1D array".to_owned(),
                )),
            }
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_owned(),
            source: Box::new(e),
        })
    }

    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any variant".into()))?;

            let Value::Array(outer) = value else {
                return Err(DataError::RetrievalError(
                    "expected 2D array (array of arrays)".to_owned(),
                ));
            };

            outer
                .iter()
                .map(|row_val| {
                    let Value::Array(inner) = row_val else {
                        return Err(DataError::RetrievalError(
                            "expected array for 2D array row".to_owned(),
                        ));
                    };
                    inner.iter().map(Self::value_to_data_value).collect()
                })
                .collect()
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_owned(),
            source: Box::new(e),
        })
    }
}
