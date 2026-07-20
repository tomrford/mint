use std::collections::HashSet;

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

pub(crate) fn mint_error(err: impl std::error::Error) -> PyErr {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    MintError::new_err(message)
}

pub(crate) fn parse_python_json(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    PyModule::import(py, "json")
}

const MAX_DATA_DEPTH: usize = 128;

pub(crate) fn py_to_json_value(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    py_to_json_value_inner(value, &mut HashSet::new(), 0)
}

fn py_to_json_value_inner(
    value: &Bound<'_, PyAny>,
    ancestors: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<serde_json::Value> {
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
        let identity = enter_container(value.as_any(), ancestors, depth)?;
        let result = value
            .iter()
            .map(|item| py_to_json_value_inner(&item, ancestors, depth + 1))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
        ancestors.remove(&identity);
        return result;
    }

    if let Ok(value) = value.cast::<PyTuple>() {
        let identity = enter_container(value.as_any(), ancestors, depth)?;
        let result = value
            .iter()
            .map(|item| py_to_json_value_inner(&item, ancestors, depth + 1))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
        ancestors.remove(&identity);
        return result;
    }

    if let Ok(value) = value.cast::<PyDict>() {
        let identity = enter_container(value.as_any(), ancestors, depth)?;
        let mut object = Map::with_capacity(value.len());
        let result = (|| {
            for (key, item) in value.iter() {
                let key = key
                    .cast::<PyString>()
                    .map_err(|_| value_error("data dictionaries must use string keys"))?
                    .extract()?;
                object.insert(key, py_to_json_value_inner(&item, ancestors, depth + 1)?);
            }
            Ok(Value::Object(object))
        })();
        ancestors.remove(&identity);
        return result;
    }

    Err(value_error(format!(
        "unsupported data value of type '{}'",
        value.get_type().name()?
    )))
}

fn enter_container(
    value: &Bound<'_, PyAny>,
    ancestors: &mut HashSet<usize>,
    depth: usize,
) -> PyResult<usize> {
    if depth >= MAX_DATA_DEPTH {
        return Err(value_error(format!(
            "data nesting exceeds the maximum depth of {MAX_DATA_DEPTH}"
        )));
    }

    let identity = value.as_ptr() as usize;
    if !ancestors.insert(identity) {
        return Err(value_error("data contains a reference cycle"));
    }
    Ok(identity)
}
