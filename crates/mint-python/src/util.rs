use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDict, PyDictMethods, PyFloat, PyInt, PyList, PyListMethods, PyModule, PyString,
    PyTuple, PyTupleMethods,
};
use serde_json::{Map, Number, Value};

use crate::MintError;

pub(crate) fn value_error(err: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(err.to_string())
}

pub(crate) fn mint_error(err: impl std::fmt::Display) -> PyErr {
    MintError::new_err(err.to_string())
}

pub(crate) fn parse_python_json(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    PyModule::import(py, "json")
}

pub(crate) fn py_to_json_value(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }

    if let Ok(value) = value.cast::<PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }

    if let Ok(value) = value.cast::<PyInt>() {
        let text: String = value.str()?.extract()?;
        if let Ok(unsigned) = text.parse::<u64>() {
            return Ok(Value::Number(Number::from(unsigned)));
        }
        if let Ok(signed) = text.parse::<i64>() {
            return Ok(Value::Number(Number::from(signed)));
        }
        return Err(value_error(
            "integer values must fit within signed or unsigned 64-bit JSON numbers",
        ));
    }

    if let Ok(value) = value.cast::<PyFloat>() {
        let raw = value.extract::<f64>()?;
        return Number::from_f64(raw)
            .map(Value::Number)
            .ok_or_else(|| value_error("floating-point values must be finite"));
    }

    if let Ok(value) = value.cast::<PyString>() {
        return Ok(Value::String(value.extract()?));
    }

    if let Ok(value) = value.cast::<PyList>() {
        return value
            .iter()
            .map(|item| py_to_json_value(&item))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }

    if let Ok(value) = value.cast::<PyTuple>() {
        return value
            .iter()
            .map(|item| py_to_json_value(&item))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }

    if let Ok(value) = value.cast::<PyDict>() {
        let mut object = Map::with_capacity(value.len());
        for (key, item) in value.iter() {
            let key = key
                .cast::<PyString>()
                .map_err(|_| value_error("data dictionaries must use string keys"))?
                .extract()?;
            object.insert(key, py_to_json_value(&item)?);
        }
        return Ok(Value::Object(object));
    }

    Err(value_error(format!(
        "unsupported data value of type '{}'",
        value.get_type().name()?
    )))
}
