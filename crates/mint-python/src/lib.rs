mod types;
mod util;

use std::collections::HashMap;

use mint_core::build::{self as core_build, BlockNames, BuildFromLayoutsRequest, NamedLayout};
use mint_core::data::{DataSource, ExcelDataSource, ExcelDataSourceOptions, JsonDataSource};
use mint_core::layout;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule};

use crate::types::{PyBlockStat, PyBuildBlock, PyBuildResult, PyBuildStats, PyDataRange, PyLayout};
use crate::util::{mint_error, parse_python_json, py_to_json_value, value_error};

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum LayoutSource {
    File { path: String },
    String { text: String, format: LayoutFormat },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutFormat {
    Toml,
    Yaml,
}

impl LayoutFormat {
    fn parse(value: &str) -> PyResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "toml" => Ok(Self::Toml),
            "yaml" | "yml" => Ok(Self::Yaml),
            other => Err(value_error(format!(
                "unknown layout format '{other}', expected 'toml' or 'yaml'"
            ))),
        }
    }

    fn infer(name: &str) -> PyResult<Self> {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return Err(value_error(
                "layout format could not be inferred; pass format='toml' or format='yaml'",
            ));
        };
        Self::parse(ext)
    }
}

impl LayoutSource {
    fn parse_config(&self) -> PyResult<mint_core::layout::block::Config> {
        match self {
            Self::File { path } => layout::load_layout(path).map_err(mint_error),
            Self::String { text, format } => match format {
                LayoutFormat::Toml => layout::parse_toml_layout(text).map_err(mint_error),
                LayoutFormat::Yaml => layout::parse_yaml_layout(text).map_err(mint_error),
            },
        }
    }
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyLayout>()?;
    module.add_class::<PyBuildBlock>()?;
    module.add_class::<PyDataRange>()?;
    module.add_class::<PyBlockStat>()?;
    module.add_class::<PyBuildStats>()?;
    module.add_class::<PyBuildResult>()?;
    module.add_function(wrap_pyfunction!(build, module)?)?;
    module.add(
        "MintError",
        module.py().get_type::<pyo3::exceptions::PyValueError>(),
    )?;
    Ok(())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (blocks, *, data=None, json_path=None, xlsx_path=None, variants=None, main_sheet="Main", strict=false))]
fn build(
    py: Python<'_>,
    blocks: Vec<PyRef<'_, PyBuildBlock>>,
    data: Option<&Bound<'_, PyAny>>,
    json_path: Option<String>,
    xlsx_path: Option<String>,
    variants: Option<Vec<String>>,
    main_sheet: &str,
    strict: bool,
) -> PyResult<PyBuildResult> {
    if blocks.is_empty() {
        return Err(value_error("at least one build block is required"));
    }

    let data_source = create_data_source(py, data, json_path, xlsx_path, variants, main_sheet)?;
    let data_source_ref = data_source.as_deref();

    let mut named_layouts = Vec::new();
    let mut seen_layouts: HashMap<String, LayoutSource> = HashMap::new();
    let mut block_names = Vec::with_capacity(blocks.len());

    for block in blocks {
        match seen_layouts.get(&block.layout_name) {
            Some(source) if source != &block.source => {
                return Err(value_error(format!(
                    "layout name '{}' was provided with multiple sources; use distinct layout names",
                    block.layout_name
                )));
            }
            Some(_) => {}
            None => {
                let config = block.source.parse_config()?;
                seen_layouts.insert(block.layout_name.clone(), block.source.clone());
                named_layouts.push(NamedLayout {
                    name: block.layout_name.clone(),
                    config,
                });
            }
        }

        block_names.push(BlockNames {
            name: block.name.clone().unwrap_or_default(),
            file: block.layout_name.clone(),
        });
    }

    let artifact = core_build::build_from_layouts(BuildFromLayoutsRequest {
        layouts: named_layouts,
        blocks: block_names,
        data_source: data_source_ref,
        strict,
        capture_values: true,
    })
    .map_err(mint_error)?;

    PyBuildResult::from_artifact(py, artifact)
}

fn create_data_source(
    py: Python<'_>,
    data: Option<&Bound<'_, PyAny>>,
    json_path: Option<String>,
    xlsx_path: Option<String>,
    variants: Option<Vec<String>>,
    main_sheet: &str,
) -> PyResult<Option<Box<dyn DataSource>>> {
    let source_count = usize::from(data.is_some())
        + usize::from(json_path.is_some())
        + usize::from(xlsx_path.is_some());
    if source_count > 1 {
        return Err(value_error(
            "data, json_path, and xlsx_path are mutually exclusive",
        ));
    }

    let variants = variants.unwrap_or_default();
    if source_count > 0 && variants.is_empty() {
        return Err(value_error(
            "variants are required when data, json_path, or xlsx_path is provided",
        ));
    }

    if let Some(data) = data {
        let value = py_to_json_value(py, data)?;
        return Ok(Some(Box::new(
            JsonDataSource::from_value(value, &variants).map_err(mint_error)?,
        )));
    }

    if let Some(path) = json_path {
        return Ok(Some(Box::new(
            JsonDataSource::from_path(path, &variants).map_err(mint_error)?,
        )));
    }

    if let Some(path) = xlsx_path {
        let mut options = ExcelDataSourceOptions::new(variants);
        main_sheet.clone_into(&mut options.main_sheet);
        return Ok(Some(Box::new(
            ExcelDataSource::from_path(path, options).map_err(mint_error)?,
        )));
    }

    Ok(None)
}

pub(crate) fn py_json_loads(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    parse_python_json(py)?
        .call_method1("loads", (text,))
        .map(|v| v.unbind())
}

pub(crate) fn py_json_dumps(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("separators", PyList::new(py, [",", ":"])?)?;
    parse_python_json(py)?
        .call_method("dumps", (value,), Some(&kwargs))?
        .extract()
}
