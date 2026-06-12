use pyo3::prelude::*;
use pyo3::types::PyModule;

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

pub(crate) fn py_to_json_value(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<serde_json::Value> {
    let text = crate::py_json_dumps(py, value)?;
    serde_json::from_str(&text).map_err(value_error)
}
