use serde_json::{Map, Number, Value};

use crate::layout::error::LayoutError;
use crate::layout::value::DataValue;

/// Records resolved values for export.
pub trait ValueSink {
    /// Insert a value at the given path.
    fn record_value(&mut self, path: &[String], value: Value) -> Result<(), LayoutError>;
}

/// Collects used values into a nested JSON object.
#[derive(Debug, Default)]
pub struct ValueCollector {
    root: Map<String, Value>,
}

impl ValueCollector {
    /// Create an empty collector.
    pub fn new() -> Self {
        Self { root: Map::new() }
    }

    /// Convert the collected values into a JSON object.
    pub fn into_value(self) -> Value {
        Value::Object(self.root)
    }
}

impl ValueSink for ValueCollector {
    fn record_value(&mut self, path: &[String], value: Value) -> Result<(), LayoutError> {
        insert_value(&mut self.root, path, value)
    }
}

/// No-op sink for builds that don't export JSON.
pub struct NoopValueSink;

impl ValueSink for NoopValueSink {
    fn record_value(&mut self, _path: &[String], _value: Value) -> Result<(), LayoutError> {
        Ok(())
    }
}

pub fn data_value_to_json(value: &DataValue) -> Result<Value, LayoutError> {
    match value {
        DataValue::Bool(v) => Ok(Value::Number(Number::from(if *v { 1 } else { 0 }))),
        DataValue::U64(v) => Ok(Value::Number(Number::from(*v))),
        DataValue::I64(v) => Ok(Value::Number(Number::from(*v))),
        DataValue::F64(v) => Number::from_f64(*v).map(Value::Number).ok_or_else(|| {
            LayoutError::DataValueExportFailed(
                "Non-finite float cannot be serialized to JSON.".to_owned(),
            )
        }),
        DataValue::Str(v) => Ok(Value::String(v.clone())),
    }
}

pub fn i128_to_json(value: i128) -> Result<Value, LayoutError> {
    if value >= 0 {
        let unsigned = u64::try_from(value).map_err(|_| {
            LayoutError::DataValueExportFailed("Value out of range for JSON number.".to_owned())
        })?;
        Ok(Value::Number(Number::from(unsigned)))
    } else {
        let signed = i64::try_from(value).map_err(|_| {
            LayoutError::DataValueExportFailed("Value out of range for JSON number.".to_owned())
        })?;
        Ok(Value::Number(Number::from(signed)))
    }
}

pub fn array_to_json(values: &[DataValue]) -> Result<Value, LayoutError> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(data_value_to_json(value)?);
    }
    Ok(Value::Array(out))
}

pub fn array_2d_to_json(values: &[Vec<DataValue>]) -> Result<Value, LayoutError> {
    let mut out = Vec::with_capacity(values.len());
    for row in values {
        out.push(array_to_json(row)?);
    }
    Ok(Value::Array(out))
}

fn insert_value(
    root: &mut Map<String, Value>,
    path: &[String],
    value: Value,
) -> Result<(), LayoutError> {
    if path.is_empty() {
        return Err(LayoutError::DataValueExportFailed(
            "Cannot record value with empty path.".to_owned(),
        ));
    }

    let key = &path[0];
    if path.len() == 1 {
        if root.contains_key(key) {
            return Err(LayoutError::DataValueExportFailed(format!(
                "Duplicate value path '{}'.",
                path.join(".")
            )));
        }
        root.insert(key.clone(), value);
        return Ok(());
    }

    let entry = root
        .entry(key.clone())
        .or_insert_with(|| Value::Object(Map::new()));

    match entry {
        Value::Object(child) => insert_value(child, &path[1..], value),
        _ => Err(LayoutError::DataValueExportFailed(format!(
            "Path '{}' collides with existing value.",
            path.join(".")
        ))),
    }
}
