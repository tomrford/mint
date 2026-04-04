use serde_json::Value;
use std::collections::HashMap;

use super::DataSource;
use super::args::DataArgs;
use super::error::DataError;
use crate::layout::value::{DataValue, ValueSource};

fn load_json_string_or_file(input: &str) -> Result<String, DataError> {
    if input.ends_with(".json") {
        std::fs::read_to_string(input)
            .map_err(|_| DataError::FileError(format!("failed to open file: {}", input)))
    } else {
        Ok(input.to_string())
    }
}

/// Shared JSON-based data source that reads version data from JSON objects.
/// Result: `Vec<HashMap<String, Value>>` in version priority order.
pub struct JsonDataSource {
    version_columns: Vec<HashMap<String, Value>>,
}

impl JsonDataSource {
    fn new(version_columns: Vec<HashMap<String, Value>>) -> Self {
        JsonDataSource { version_columns }
    }

    /// Creates a JSON data source from a JSON object.
    /// Expected format: `{ "VersionName": { "key1": value1, "key2": value2, ... }, ... }`
    pub(crate) fn from_json(args: &DataArgs) -> Result<Self, DataError> {
        let json_str = args
            .json
            .as_ref()
            .ok_or_else(|| DataError::MiscError("missing json config".to_string()))?;

        let json_content = load_json_string_or_file(json_str)?;
        let data: HashMap<String, HashMap<String, Value>> = serde_json::from_str(&json_content)
            .map_err(|e| DataError::FileError(format!("failed to parse JSON: {}", e)))?;

        let versions = args.get_version_list();
        let mut version_columns = Vec::with_capacity(versions.len());

        for version in &versions {
            let map = data
                .get(version)
                .ok_or_else(|| {
                    DataError::RetrievalError(format!(
                        "version '{}' not found in JSON data",
                        version
                    ))
                })?
                .clone();
            version_columns.push(map);
        }

        Ok(Self::new(version_columns))
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.version_columns
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
                        "unsupported numeric type".to_string(),
                    ))
                }
            }
            Value::String(s) => Ok(DataValue::Str(s.clone())),
            _ => Err(DataError::RetrievalError(
                "expected scalar value".to_string(),
            )),
        }
    }
}

impl DataSource for JsonDataSource {
    fn retrieve_single_value(&self, name: &str) -> Result<DataValue, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any version".into()))?;

            let dv = Self::value_to_data_value(value)?;
            match dv {
                DataValue::Str(_) => Err(DataError::RetrievalError(
                    "Found non-numeric single value".to_string(),
                )),
                _ => Ok(dv),
            }
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_string(),
            source: Box::new(e),
        })
    }

    fn retrieve_1d_array_or_string(&self, name: &str) -> Result<ValueSource, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any version".into()))?;

            match value {
                Value::Array(arr) => {
                    let items: Result<Vec<_>, _> =
                        arr.iter().map(Self::value_to_data_value).collect();
                    Ok(ValueSource::Array(items?))
                }
                Value::String(s) => Ok(ValueSource::Single(DataValue::Str(s.clone()))),
                _ => Err(DataError::RetrievalError(
                    "expected array or string for 1D array".to_string(),
                )),
            }
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_string(),
            source: Box::new(e),
        })
    }

    fn retrieve_2d_array(&self, name: &str) -> Result<Vec<Vec<DataValue>>, DataError> {
        let result = (|| {
            let value = self
                .lookup(name)
                .ok_or_else(|| DataError::RetrievalError("key not found in any version".into()))?;

            let Value::Array(outer) = value else {
                return Err(DataError::RetrievalError(
                    "expected 2D array (array of arrays)".to_string(),
                ));
            };

            outer
                .iter()
                .map(|row_val| {
                    let Value::Array(inner) = row_val else {
                        return Err(DataError::RetrievalError(
                            "expected array for 2D array row".to_string(),
                        ));
                    };
                    inner.iter().map(Self::value_to_data_value).collect()
                })
                .collect()
        })();

        result.map_err(|e| DataError::WhileRetrieving {
            name: name.to_string(),
            source: Box::new(e),
        })
    }
}
