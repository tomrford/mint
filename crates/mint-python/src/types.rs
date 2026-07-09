use mint_core::build::{BlockStat, BuildArtifact, BuildStats};
use mint_core::output::{DataRange, OutputFormat};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyTuple};

use crate::{LayoutSource, mint_error, py_json_loads, value_error};

#[pyclass(name = "Layout", skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyLayout {
    pub(crate) name: String,
    pub(crate) source: LayoutSource,
}

#[pymethods]
impl PyLayout {
    #[staticmethod]
    fn from_file(path: String) -> Self {
        Self {
            name: path.clone(),
            source: LayoutSource::File { path },
        }
    }

    #[staticmethod]
    fn from_string(name: String, text: String) -> Self {
        Self {
            name,
            source: LayoutSource::String { text },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[pyo3(signature = (names=None, *more_names))]
    fn blocks(
        &self,
        names: Option<&Bound<'_, PyAny>>,
        more_names: &Bound<'_, PyTuple>,
    ) -> PyResult<Vec<PyBuildBlock>> {
        let Some(names) = parse_block_names(names, more_names)? else {
            return Ok(vec![PyBuildBlock {
                layout_name: self.name.clone(),
                source: self.source.clone(),
                name: None,
            }]);
        };

        names
            .into_iter()
            .map(|name| {
                Ok(PyBuildBlock {
                    layout_name: self.name.clone(),
                    source: self.source.clone(),
                    name: Some(name),
                })
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("Layout(name={:?})", self.name)
    }
}

fn parse_block_names(
    name: Option<&Bound<'_, PyAny>>,
    more_names: &Bound<'_, PyTuple>,
) -> PyResult<Option<Vec<String>>> {
    let Some(first) = name else {
        return Ok(None);
    };

    if first.is_none() {
        if more_names.is_empty() {
            return Ok(None);
        }
        return Err(value_error("Block names cannot follow None."));
    }

    let mut names = if let Ok(name) = first.extract::<String>() {
        vec![name]
    } else if more_names.is_empty() {
        return first.extract::<Vec<String>>().map(Some);
    } else {
        return Err(value_error(
            "The first block name must be a string when passing multiple names.",
        ));
    };

    for name in more_names {
        names.push(name.extract::<String>()?);
    }
    Ok(Some(names))
}

#[pyclass(name = "BuildBlock", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyBuildBlock {
    pub(crate) layout_name: String,
    pub(crate) source: LayoutSource,
    pub(crate) name: Option<String>,
}

#[pymethods]
impl PyBuildBlock {
    #[getter]
    fn layout_name(&self) -> &str {
        &self.layout_name
    }

    #[getter]
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn __repr__(&self) -> String {
        match &self.name {
            Some(name) => format!("BuildBlock(layout={:?}, name={:?})", self.layout_name, name),
            None => format!("BuildBlock(layout={:?}, name=None)", self.layout_name),
        }
    }
}

#[pyclass(name = "DataRange", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyDataRange {
    inner: DataRange,
}

#[pymethods]
impl PyDataRange {
    #[getter]
    fn start_address(&self) -> u32 {
        self.inner.start_address
    }

    #[getter]
    fn reserved_size(&self) -> u32 {
        self.inner.reserved_size
    }

    #[getter]
    fn allocated_size(&self) -> u32 {
        self.inner.allocated_size
    }

    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.bytestream).unbind()
    }

    fn __len__(&self) -> usize {
        self.inner.bytestream.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "DataRange(start_address=0x{:X}, length={}, reserved_size={}, allocated_size={})",
            self.inner.start_address,
            self.inner.bytestream.len(),
            self.inner.reserved_size,
            self.inner.allocated_size
        )
    }
}

#[pyclass(name = "BlockStat", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyBlockStat {
    inner: BlockStat,
}

#[pymethods]
impl PyBlockStat {
    #[getter]
    fn layout(&self) -> String {
        self.inner.layout.display().to_string()
    }

    #[getter]
    fn block(&self) -> &str {
        &self.inner.block
    }

    #[getter]
    fn start_address(&self) -> u32 {
        self.inner.start_address
    }

    #[getter]
    fn allocated_size(&self) -> u32 {
        self.inner.allocated_size
    }

    #[getter]
    fn reserved_size(&self) -> u32 {
        self.inner.reserved_size
    }

    #[getter]
    fn checksum_values(&self) -> Vec<u32> {
        self.inner.checksum_values.clone()
    }
}

#[pyclass(name = "BuildStats", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyBuildStats {
    blocks_processed: usize,
    total_allocated: usize,
    total_reserved: usize,
    total_duration_ms: f64,
    block_stats: Vec<PyBlockStat>,
    space_reserved_pct: f64,
}

impl From<BuildStats> for PyBuildStats {
    fn from(stats: BuildStats) -> Self {
        let space_reserved_pct = stats.space_reserved_pct();
        Self {
            blocks_processed: stats.blocks_processed,
            total_allocated: stats.total_allocated,
            total_reserved: stats.total_reserved,
            total_duration_ms: stats.total_duration.as_secs_f64() * 1000.0,
            block_stats: stats
                .block_stats
                .into_iter()
                .map(|inner| PyBlockStat { inner })
                .collect(),
            space_reserved_pct,
        }
    }
}

#[pymethods]
impl PyBuildStats {
    #[getter]
    fn blocks_processed(&self) -> usize {
        self.blocks_processed
    }

    #[getter]
    fn total_allocated(&self) -> usize {
        self.total_allocated
    }

    #[getter]
    fn total_reserved(&self) -> usize {
        self.total_reserved
    }

    #[getter]
    fn total_duration_ms(&self) -> f64 {
        self.total_duration_ms
    }

    #[getter]
    fn block_stats(&self) -> Vec<PyBlockStat> {
        self.block_stats.clone()
    }

    #[getter]
    fn space_reserved_pct(&self) -> f64 {
        self.space_reserved_pct
    }
}

#[pyclass(name = "BuildResult", frozen, skip_from_py_object)]
pub(crate) struct PyBuildResult {
    ranges: Vec<PyDataRange>,
    stats: PyBuildStats,
    used_values: Py<PyAny>,
}

impl PyBuildResult {
    pub(crate) fn from_artifact(py: Python<'_>, artifact: BuildArtifact) -> PyResult<Self> {
        let used_values = artifact
            .used_values
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        let used_values_text = serde_json::to_string(&used_values).map_err(value_error)?;
        Ok(Self {
            ranges: artifact
                .ranges
                .into_iter()
                .map(|inner| PyDataRange { inner })
                .collect(),
            stats: artifact.stats.into(),
            used_values: py_json_loads(py, &used_values_text)?,
        })
    }
}

#[pymethods]
impl PyBuildResult {
    #[getter]
    fn ranges(&self) -> Vec<PyDataRange> {
        self.ranges.clone()
    }

    #[getter]
    fn stats(&self) -> PyBuildStats {
        self.stats.clone()
    }

    #[getter]
    fn used_values(&self, py: Python<'_>) -> Py<PyAny> {
        self.used_values.clone_ref(py)
    }

    #[pyo3(signature = (*, record_width=32))]
    fn to_intel_hex(&self, record_width: usize) -> PyResult<String> {
        self.render(OutputFormat::Hex, record_width)
    }

    #[pyo3(signature = (*, record_width=32))]
    fn to_srec(&self, record_width: usize) -> PyResult<String> {
        self.render(OutputFormat::Mot, record_width)
    }
}

impl PyBuildResult {
    fn render(&self, format: OutputFormat, record_width: usize) -> PyResult<String> {
        let artifact = mint_core::build::BuildArtifact {
            ranges: self
                .ranges
                .iter()
                .map(|range| range.inner.clone())
                .collect(),
            stats: BuildStats::new(),
            used_values: None,
        };
        artifact.render(format, record_width).map_err(mint_error)
    }
}
