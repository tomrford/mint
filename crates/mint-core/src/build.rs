use crate::data::DataSource;
use crate::data::error::DataError;
use crate::error::MintError;
use crate::layout::block::Config;
pub use crate::layout::selection::BlockSelector;
use crate::layout::selection::{LayoutSelection, SelectedBlock};
use crate::layout::used_values::ValueCollector;
use crate::output;
use crate::output::error::OutputError;
use crate::output::{DataRange, OutputFormat};
use rayon::prelude::*;
use std::ops::Deref;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BlockStat {
    pub layout: PathBuf,
    pub block: String,
    /// Start address in target addressable units.
    pub start_address: u32,
    /// Width of one target addressable unit.
    pub address_unit_bits: usize,
    /// Allocated block size in octets.
    pub allocated_size: u32,
    /// Emitted payload size in octets.
    pub reserved_size: u32,
    pub checksum_values: Vec<u32>,
}

impl BlockStat {
    pub fn display_name(&self) -> String {
        format!("{}#{}", self.layout.display(), self.block)
    }

    pub fn allocated_address_units(&self) -> u64 {
        debug_assert!(
            self.address_unit_bits >= 8
                && self.address_unit_bits.is_multiple_of(8)
                && (self.allocated_size as usize).is_multiple_of(self.address_unit_bits / 8),
            "build statistics must contain a whole number of target address units"
        );
        let unit_octets = (self.address_unit_bits / 8).max(1) as u64;
        u64::from(self.allocated_size) / unit_octets
    }
}

#[derive(Debug, Default)]
pub struct BuildStats {
    pub blocks_processed: usize,
    pub total_allocated: u64,
    pub total_reserved: u64,
    #[deprecated(note = "Scheduled for removal in mint-core 3.0")]
    pub total_duration: Duration,
    pub block_stats: Vec<BlockStat>,
}

impl BuildStats {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(
        deprecated,
        reason = "Keep reporting elapsed time to mint-core 2.x callers"
    )]
    fn record_duration(&mut self, duration: Duration) {
        self.total_duration = duration;
    }

    pub fn add_block(&mut self, stat: BlockStat) {
        self.blocks_processed += 1;
        self.total_allocated += u64::from(stat.allocated_size);
        self.total_reserved += u64::from(stat.reserved_size);
        self.block_stats.push(stat);
    }

    pub fn space_reserved_pct(&self) -> f64 {
        if self.total_allocated == 0 {
            0.0
        } else {
            (self.total_reserved as f64 / self.total_allocated as f64) * 100.0
        }
    }
}

#[derive(Clone)]
pub struct BuildRequest<'a> {
    pub blocks: Vec<BlockSelector>,
    pub data_source: Option<&'a dyn DataSource>,
    pub strict: bool,
    pub capture_values: bool,
}

#[derive(Debug)]
pub struct NamedLayout {
    pub name: PathBuf,
    pub config: Config,
}

pub struct BuildFromLayoutsRequest<'a> {
    pub layouts: Vec<NamedLayout>,
    pub blocks: Vec<BlockSelector>,
    pub data_source: Option<&'a dyn DataSource>,
    pub strict: bool,
    pub capture_values: bool,
}

#[derive(Debug)]
pub struct BuildArtifact {
    pub ranges: Vec<DataRange>,
    pub stats: BuildStats,
    pub used_values: Option<serde_json::Value>,
}

impl BuildArtifact {
    pub fn render(&self, format: OutputFormat, record_width: usize) -> Result<String, OutputError> {
        output::render(&self.ranges, record_width, format)
    }
}

struct BlockBuildResult {
    data_range: DataRange,
    stat: BlockStat,
    used_values: Option<serde_json::Value>,
}

pub fn build(request: BuildRequest<'_>) -> Result<BuildArtifact, MintError> {
    build_from_files(
        &request.blocks,
        request.strict,
        request.capture_values,
        || Ok(request.data_source),
    )
}

/// Build selected blocks, loading the data source after their static validation.
pub fn build_with_data_source(
    blocks: &[BlockSelector],
    strict: bool,
    capture_values: bool,
    load_data_source: impl FnOnce() -> Result<Option<Box<dyn DataSource>>, DataError>,
) -> Result<BuildArtifact, MintError> {
    build_from_files(blocks, strict, capture_values, load_data_source)
}

fn build_from_files<'a, S: Deref<Target = dyn DataSource + 'a>>(
    blocks: &[BlockSelector],
    strict: bool,
    capture_values: bool,
    load_data_source: impl FnOnce() -> Result<Option<S>, DataError>,
) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();
    let selection = LayoutSelection::files(blocks)?;
    build_resolved(
        &selection,
        strict,
        capture_values,
        load_data_source,
        start_time,
    )
}

pub fn build_from_layouts(
    request: BuildFromLayoutsRequest<'_>,
) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();

    let selection = LayoutSelection::named(
        request
            .layouts
            .into_iter()
            .map(|layout| (layout.name, layout.config)),
        &request.blocks,
    )?;
    build_resolved(
        &selection,
        request.strict,
        request.capture_values,
        || Ok(request.data_source),
        start_time,
    )
}

fn build_resolved<'a, S: Deref<Target = dyn DataSource + 'a>>(
    selection: &LayoutSelection<'_>,
    strict: bool,
    capture_values: bool,
    load_data_source: impl FnOnce() -> Result<Option<S>, DataError>,
    start_time: Instant,
) -> Result<BuildArtifact, MintError> {
    let prepared = selection.prepare(|block_name, layout_file, source| MintError::InBlock {
        block_name,
        layout_file,
        source,
    })?;
    let jobs = prepared.blocks().collect::<Vec<_>>();
    check_allocations(&jobs)?;
    let source = load_data_source()?;
    let data_source = source.as_deref();
    let mut results = jobs
        .par_iter()
        .map(|job| build_single_bytestream(job, data_source, strict, capture_values))
        .collect::<Result<Vec<_>, _>>()?;

    let used_values = if capture_values {
        Some(take_used_values_report(&mut results)?)
    } else {
        None
    };

    let mut stats = BuildStats::new();
    let ranges = results
        .into_iter()
        .map(|result| {
            stats.add_block(result.stat);
            result.data_range
        })
        .collect();

    stats.record_duration(start_time.elapsed());
    Ok(BuildArtifact {
        ranges,
        stats,
        used_values,
    })
}

fn build_single_bytestream(
    job: &SelectedBlock<'_>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BlockBuildResult, MintError> {
    let result = (|| {
        let block = job.plan.block;
        let mut collector = ValueCollector::new(capture_values);

        let build_output = block.emit(
            job.plan,
            &job.layout.fingerprints,
            data_source,
            strict,
            &mut collector,
        )?;

        let data_range = output::bytestream_to_datarange(
            build_output.bytestream,
            &block.header,
            job.plan.resolved.abi(),
        )?;

        let stat = BlockStat {
            layout: job.path.to_path_buf(),
            block: job.name.to_owned(),
            start_address: data_range.start_address,
            address_unit_bits: data_range.address_unit_bits,
            allocated_size: data_range.allocated_size,
            reserved_size: data_range.reserved_size,
            checksum_values: build_output.checksum_values,
        };

        Ok(BlockBuildResult {
            data_range,
            stat,
            used_values: collector.into_value(),
        })
    })();

    result.map_err(|e| MintError::InBlock {
        block_name: job.name.to_owned(),
        layout_file: job.path.display().to_string(),
        source: Box::new(e),
    })
}

fn check_allocations(jobs: &[SelectedBlock<'_>]) -> Result<(), MintError> {
    let Some(first) = jobs.first() else {
        return Ok(());
    };
    let address_unit_bits = first.plan.resolved.abi().address_unit_bits();
    if jobs
        .iter()
        .any(|job| job.plan.resolved.abi().address_unit_bits() != address_unit_bits)
    {
        return Err(OutputError::AddressRangeError(
            "one output file cannot mix target addressable-unit widths".to_owned(),
        )
        .into());
    }

    let mut sorted = jobs.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|job| job.plan.allocation.start);
    for pair in sorted.windows(2) {
        let first = pair[0];
        let second = pair[1];
        let a_start = first.plan.allocation.start;
        let b_start = second.plan.allocation.start;
        let a_end = first.plan.allocation.end;
        if b_start < a_end {
            let b_end = second.plan.allocation.end;
            let overlap_end = a_end.min(b_end);
            return Err(OutputError::BlockOverlapError(format!(
                "Block '{}#{}' (0x{a_start:08X}-0x{:08X}) overlaps with block '{}#{}' (0x{b_start:08X}-0x{:08X}). Overlap: 0x{b_start:08X}-0x{:08X} ({} bytes)",
                first.path.display(), first.name, a_end - 1,
                second.path.display(), second.name, b_end - 1,
                overlap_end - 1, overlap_end - b_start,
            )).into());
        }
    }
    Ok(())
}

fn take_used_values_report(
    results: &mut [BlockBuildResult],
) -> Result<serde_json::Value, MintError> {
    let mut report = serde_json::Map::new();
    for result in results {
        let value = result.used_values.take().ok_or_else(|| {
            OutputError::FileError("JSON export requested but values were not captured.".to_owned())
        })?;
        let file_entry = report
            .entry(result.stat.layout.display().to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let serde_json::Value::Object(blocks) = file_entry else {
            return Err(OutputError::FileError(
                "JSON export contains unexpected non-object entry.".to_owned(),
            )
            .into());
        };
        let block_name = result.stat.block.as_str();
        if blocks.contains_key(block_name) {
            return Err(OutputError::FileError(format!(
                "Duplicate block '{}' in JSON export for file '{}'.",
                block_name,
                result.stat.layout.display()
            ))
            .into());
        }
        blocks.insert(block_name.to_owned(), value);
    }
    Ok(serde_json::Value::Object(report))
}
