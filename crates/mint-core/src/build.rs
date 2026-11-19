use crate::data::DataSource;
use crate::data::error::DataError;
use crate::error::MintError;
use crate::layout;
use crate::layout::block::{Block, Config};
use crate::layout::error::LayoutError;
use crate::layout::fingerprint::PlannedLayout;
use crate::layout::resolved::PlannedBlock;
use crate::layout::used_values::ValueCollector;
use crate::output;
use crate::output::error::OutputError;
use crate::output::{DataRange, OutputFormat};
use indexmap::IndexMap;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockSelector {
    pub layout: PathBuf,
    pub block: Option<String>,
}

impl BlockSelector {
    pub fn all(layout: impl Into<PathBuf>) -> Self {
        Self {
            layout: layout.into(),
            block: None,
        }
    }

    pub fn named(layout: impl Into<PathBuf>, block: impl AsRef<str>) -> Self {
        Self {
            layout: layout.into(),
            block: Some(block.as_ref().to_owned()),
        }
    }

    pub fn display_name(&self) -> String {
        match &self.block {
            Some(block) => format!("{}#{}", self.layout.display(), block),
            None => self.layout.display().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedBlock<'a> {
    pub(crate) name: &'a str,
    pub(crate) layout: &'a PathBuf,
    pub(crate) block: &'a Block,
    config: &'a Config,
}

struct BlockJob<'a> {
    selected: ResolvedBlock<'a>,
    plan: &'a PlannedBlock<'a>,
    fingerprints: &'a IndexMap<String, u64>,
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
    let (blocks, layouts) = load_layouts(blocks)?;
    let resolved_blocks = resolve_blocks(&blocks, &layouts)?;
    let mut artifact = build_resolved(resolved_blocks, strict, capture_values, load_data_source)?;
    artifact.stats.record_duration(start_time.elapsed());
    Ok(artifact)
}

pub fn build_from_layouts(
    request: BuildFromLayoutsRequest<'_>,
) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();

    let layouts = collect_named_layouts(request.layouts)?;
    let resolved_blocks = resolve_blocks(&request.blocks, &layouts)?;
    let mut artifact = build_resolved(
        resolved_blocks,
        request.strict,
        request.capture_values,
        || Ok(request.data_source),
    )?;
    artifact.stats.record_duration(start_time.elapsed());
    Ok(artifact)
}

fn build_resolved<'a, S: Deref<Target = dyn DataSource + 'a>>(
    resolved_blocks: Vec<ResolvedBlock<'_>>,
    strict: bool,
    capture_values: bool,
    load_data_source: impl FnOnce() -> Result<Option<S>, DataError>,
) -> Result<BuildArtifact, MintError> {
    let plans = plan_layouts(&resolved_blocks, |block_name, layout_file, source| {
        MintError::InBlock {
            block_name,
            layout_file,
            source,
        }
    })?;
    let jobs = resolved_blocks
        .into_iter()
        .map(|selected| {
            let layout = &plans[selected.layout];
            BlockJob {
                plan: &layout.blocks[selected.name],
                fingerprints: &layout.fingerprints,
                selected,
            }
        })
        .collect::<Vec<_>>();
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

    Ok(BuildArtifact {
        ranges,
        stats,
        used_values,
    })
}

pub(crate) fn plan_layouts<'a>(
    resolved_blocks: &[ResolvedBlock<'a>],
    with_context: fn(String, String, Box<MintError>) -> MintError,
) -> Result<HashMap<&'a PathBuf, PlannedLayout<'a>>, MintError> {
    let mut layouts: HashMap<_, (_, Vec<_>)> = HashMap::new();
    for selected in resolved_blocks {
        let (_, blocks) = layouts
            .entry(selected.layout)
            .or_insert_with(|| (selected.config, Vec::new()));
        blocks.push((selected.name, selected.block));
    }
    layouts
        .into_par_iter()
        .map(|(path, (config, blocks))| {
            layout::fingerprint::plan_layout(config, blocks, false)
                .map(|plan| (path, plan))
                .map_err(|error| match error {
                    LayoutError::InBlock { block, source } => with_context(
                        block,
                        path.display().to_string(),
                        Box::new((*source).into()),
                    ),
                    error => error.into(),
                })
        })
        .collect()
}

fn collect_named_layouts(
    layouts: Vec<NamedLayout>,
) -> Result<HashMap<PathBuf, Config>, LayoutError> {
    let mut out = HashMap::with_capacity(layouts.len());
    for layout in layouts {
        if out.insert(layout.name.clone(), layout.config).is_some() {
            return Err(LayoutError::FileError(format!(
                "duplicate layout name '{}'",
                layout.name.display()
            )));
        }
    }
    Ok(out)
}

pub(crate) fn resolve_blocks<'a>(
    block_args: &[BlockSelector],
    layouts: &'a HashMap<PathBuf, Config>,
) -> Result<Vec<ResolvedBlock<'a>>, LayoutError> {
    let mut resolved = Vec::new();
    for arg in block_args {
        let (path, layout) = layouts.get_key_value(&arg.layout).ok_or_else(|| {
            let mut available = layouts
                .keys()
                .map(|layout| layout.display().to_string())
                .collect::<Vec<_>>();
            available.sort_unstable();
            LayoutError::BlockNotFound(format!(
                "layout '{}'. Available layouts: {}",
                arg.layout.display(),
                available.join(", ")
            ))
        })?;

        let select = |(name, block): (&'a String, &'a Block)| ResolvedBlock {
            name,
            layout: path,
            block,
            config: layout,
        };
        if let Some(block_name) = &arg.block {
            let block = layout.blocks.get_key_value(block_name).ok_or_else(|| {
                let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
                LayoutError::BlockNotFound(format!(
                    "'{}' in '{}'. Available blocks: {}",
                    block_name,
                    path.display(),
                    available_blocks
                ))
            })?;
            resolved.push(select(block));
        } else {
            resolved.extend(layout.blocks.iter().map(select));
        }
    }

    let mut seen = HashSet::new();
    resolved.retain(|b| seen.insert((b.layout, b.name)));
    (!resolved.is_empty())
        .then_some(resolved)
        .ok_or(LayoutError::NoBlocksProvided)
}

pub(crate) fn load_layouts(
    block_args: &[BlockSelector],
) -> Result<(Vec<BlockSelector>, HashMap<PathBuf, Config>), LayoutError> {
    let mut first_paths = HashMap::new();
    let normalized_blocks = block_args
        .iter()
        .map(|block| {
            let canonical = std::fs::canonicalize(&block.layout).map_err(|error| {
                LayoutError::FileError(format!(
                    "failed to resolve layout file '{}': {error}",
                    block.layout.display()
                ))
            })?;
            let layout = first_paths
                .entry(canonical)
                .or_insert_with(|| block.layout.clone())
                .clone();
            Ok(BlockSelector {
                layout,
                block: block.block.clone(),
            })
        })
        .collect::<Result<Vec<_>, LayoutError>>()?;

    let unique_files: HashSet<PathBuf> = normalized_blocks
        .iter()
        .map(|block| block.layout.clone())
        .collect();

    let layouts: HashMap<PathBuf, Config> = unique_files
        .par_iter()
        .map(|file| layout::load_layout(file).map(|cfg| (file.clone(), cfg)))
        .collect::<Result<_, LayoutError>>()?;

    Ok((normalized_blocks, layouts))
}

fn build_single_bytestream(
    job: &BlockJob<'_>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BlockBuildResult, MintError> {
    let selected = &job.selected;
    let result = (|| {
        let block = selected.block;
        let mut collector = ValueCollector::new(capture_values);

        let build_output = block.emit(
            job.plan,
            job.fingerprints,
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
            layout: selected.layout.clone(),
            block: selected.name.to_owned(),
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
        block_name: selected.name.to_owned(),
        layout_file: selected.layout.display().to_string(),
        source: Box::new(e),
    })
}

fn check_allocations(jobs: &[BlockJob<'_>]) -> Result<(), MintError> {
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
                first.selected.layout.display(), first.selected.name, a_end - 1,
                second.selected.layout.display(), second.selected.name, b_end - 1,
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
