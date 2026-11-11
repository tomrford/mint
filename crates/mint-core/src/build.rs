use crate::data::DataSource;
use crate::error::MintError;
use crate::layout;
use crate::layout::block::{Block, Config};
use crate::layout::error::LayoutError;
use crate::layout::plan::{PlannedBlock, plan_block};
use crate::layout::settings::MintConfig;
use crate::layout::used_values::ValueCollector;
use crate::output;
use crate::output::error::OutputError;
use crate::output::{DataRange, OutputFormat};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    pub block_stats: Vec<BlockStat>,
}

impl BuildStats {
    pub fn new() -> Self {
        Self::default()
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

/// Layouts loaded for a command and the blocks selected from them.
#[derive(Debug)]
pub struct Selection {
    layouts: Vec<(PathBuf, Config)>,
    blocks: Vec<SelectedBlock>,
}

#[derive(Debug)]
struct SelectedBlock {
    layout: usize,
    name: String,
}

/// Selected blocks that passed the preflight, ready to build.
pub struct BuildPlan<'a> {
    pub(crate) blocks: Vec<PlannedSelection<'a>>,
}

pub(crate) struct PlannedSelection<'a> {
    pub(crate) layout: &'a Path,
    pub(crate) settings: &'a MintConfig,
    pub(crate) block: PlannedBlock<'a>,
}

impl Selection {
    /// Loads each named layout file once and resolves the selectors against
    /// them. Equivalent paths to one file share the first spelling.
    pub fn load(blocks: &[BlockSelector]) -> Result<Self, LayoutError> {
        let mut first_paths = HashMap::new();
        let normalized = blocks
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

        let mut files = Vec::new();
        for block in &normalized {
            if !files.contains(&block.layout) {
                files.push(block.layout.clone());
            }
        }
        let layouts = files
            .into_par_iter()
            .map(|file| layout::load_layout(&file).map(|config| (file, config)))
            .collect::<Result<Vec<_>, LayoutError>>()?;
        Self::select(layouts, &normalized)
    }

    /// Resolves the selectors against already parsed, named layouts.
    pub fn from_layouts(
        layouts: Vec<NamedLayout>,
        blocks: &[BlockSelector],
    ) -> Result<Self, LayoutError> {
        let mut named = Vec::with_capacity(layouts.len());
        for layout in layouts {
            if named.iter().any(|(name, _)| name == &layout.name) {
                return Err(LayoutError::FileError(format!(
                    "duplicate layout name '{}'",
                    layout.name.display()
                )));
            }
            named.push((layout.name, layout.config));
        }
        Self::select(named, blocks)
    }

    fn select(
        layouts: Vec<(PathBuf, Config)>,
        selectors: &[BlockSelector],
    ) -> Result<Self, LayoutError> {
        let mut blocks = Vec::new();
        for selector in selectors {
            let index = layouts
                .iter()
                .position(|(name, _)| name == &selector.layout)
                .ok_or_else(|| {
                    let mut available = layouts
                        .iter()
                        .map(|(name, _)| name.display().to_string())
                        .collect::<Vec<_>>();
                    available.sort_unstable();
                    LayoutError::BlockNotFound(format!(
                        "layout '{}'. Available layouts: {}",
                        selector.layout.display(),
                        available.join(", ")
                    ))
                })?;
            let config = &layouts[index].1;
            match &selector.block {
                Some(name) if !config.blocks.contains_key(name) => {
                    return Err(LayoutError::BlockNotFound(format!(
                        "'{}' in '{}'. Available blocks: {}",
                        name,
                        selector.layout.display(),
                        config.blocks.keys().cloned().collect::<Vec<_>>().join(", ")
                    )));
                }
                Some(name) => blocks.push((index, name.clone())),
                None => blocks.extend(config.blocks.keys().map(|name| (index, name.clone()))),
            }
        }

        let mut seen = HashSet::new();
        blocks.retain(|block| seen.insert(block.clone()));
        if blocks.is_empty() {
            return Err(LayoutError::NoBlocksProvided);
        }
        Ok(Self {
            layouts,
            blocks: blocks
                .into_iter()
                .map(|(layout, name)| SelectedBlock { layout, name })
                .collect(),
        })
    }

    /// Checks that every selected block can be produced: resolves and
    /// statically validates each one and calculates the fingerprints it
    /// emits. Building the plan needs only data values.
    pub fn preflight(&self) -> Result<BuildPlan<'_>, MintError> {
        self.plan(|layout, block, error| MintError::InBlock {
            block_name: block.to_owned(),
            layout_file: layout.display().to_string(),
            source: Box::new(error.into()),
        })
    }

    /// The preflight shared by build and header generation; `context` names
    /// the failing block in the command's own words.
    pub(crate) fn plan(
        &self,
        context: fn(&Path, &str, LayoutError) -> MintError,
    ) -> Result<BuildPlan<'_>, MintError> {
        let blocks = self
            .blocks
            .par_iter()
            .map(|selected| {
                let (path, config) = &self.layouts[selected.layout];
                let name = selected.name.as_str();
                plan_block(config, name, &config.blocks[name])
                    .map(|block| PlannedSelection {
                        layout: path,
                        settings: &config.mint,
                        block,
                    })
                    .map_err(|error| context(path, name, error))
            })
            .collect::<Result<_, _>>()?;
        Ok(BuildPlan { blocks })
    }
}

impl BuildPlan<'_> {
    /// Emits every planned block and combines them into one output.
    pub fn build(
        &self,
        data_source: Option<&dyn DataSource>,
        strict: bool,
        capture_values: bool,
    ) -> Result<BuildArtifact, MintError> {
        let mut results = self
            .blocks
            .par_iter()
            .map(|planned| build_single_bytestream(planned, data_source, strict, capture_values))
            .collect::<Result<Vec<_>, _>>()?;

        let used_values = if capture_values {
            Some(take_used_values_report(&mut results)?)
        } else {
            None
        };

        let (ranges, stats) = collect_results(results)?;

        Ok(BuildArtifact {
            ranges,
            stats,
            used_values,
        })
    }
}

struct BlockBuildResult {
    data_range: DataRange,
    stat: BlockStat,
    used_values: Option<serde_json::Value>,
}

pub fn build(request: BuildRequest<'_>) -> Result<BuildArtifact, MintError> {
    Selection::load(&request.blocks)?.preflight()?.build(
        request.data_source,
        request.strict,
        request.capture_values,
    )
}

pub fn build_from_layouts(
    request: BuildFromLayoutsRequest<'_>,
) -> Result<BuildArtifact, MintError> {
    Selection::from_layouts(request.layouts, &request.blocks)?
        .preflight()?
        .build(request.data_source, request.strict, request.capture_values)
}

fn build_single_bytestream(
    planned: &PlannedSelection<'_>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BlockBuildResult, MintError> {
    let block = planned.block.block;
    let result = (|| {
        let mut collector = ValueCollector::new(capture_values);
        let build_output = Block::emit(
            &planned.block,
            data_source,
            planned.settings,
            strict,
            &mut collector,
        )?;

        // The preflight validated the block length, address-unit divisibility
        // and 32-bit output range, and the bytestream is the resolved layout size.
        let data_range = DataRange {
            start_address: block.header.start_address,
            address_unit_bits: planned.settings.abi.address_unit_bits(),
            reserved_size: build_output.bytestream.len() as u32,
            bytestream: build_output.bytestream,
            allocated_size: block.header.length,
        };

        let stat = BlockStat {
            layout: planned.layout.to_path_buf(),
            block: planned.block.name.to_owned(),
            start_address: data_range.start_address,
            address_unit_bits: data_range.address_unit_bits,
            allocated_size: data_range.allocated_size,
            reserved_size: data_range.reserved_size,
            checksum_values: build_output.checksum_values,
        };

        Ok::<_, MintError>(BlockBuildResult {
            data_range,
            stat,
            used_values: collector.into_value(),
        })
    })();

    result.map_err(|e| MintError::InBlock {
        block_name: planned.block.name.to_owned(),
        layout_file: planned.layout.display().to_string(),
        source: Box::new(e),
    })
}

fn collect_results(
    results: Vec<BlockBuildResult>,
) -> Result<(Vec<DataRange>, BuildStats), MintError> {
    let mut stats = BuildStats::new();
    let named_ranges: Vec<(String, DataRange)> = results
        .into_iter()
        .map(|r| {
            let name = r.stat.display_name();
            stats.add_block(r.stat);
            (name, r.data_range)
        })
        .collect();

    check_overlaps(&named_ranges)?;
    let ranges = named_ranges.into_iter().map(|(_, r)| r).collect();
    Ok((ranges, stats))
}

fn check_overlaps(named_ranges: &[(String, DataRange)]) -> Result<(), MintError> {
    if let Some((_, first)) = named_ranges.first()
        && named_ranges
            .iter()
            .any(|(_, range)| range.address_unit_bits != first.address_unit_bits)
    {
        return Err(OutputError::AddressRangeError(
            "one output file cannot mix target addressable-unit widths".to_owned(),
        )
        .into());
    }

    // Block validation already bounded every built range to the 32-bit octet
    // address space.
    let ranges = named_ranges
        .iter()
        .map(|(name, range)| {
            let start = u64::from(range.start_address) * (range.address_unit_bits / 8) as u64;
            (
                name.as_str(),
                start,
                start + u64::from(range.allocated_size),
            )
        })
        .collect::<Vec<_>>();

    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            let (name_a, a_start, a_end) = ranges[i];
            let (name_b, b_start, b_end) = ranges[j];

            let overlap_start = a_start.max(b_start);
            let overlap_end = a_end.min(b_end);

            if overlap_start < overlap_end {
                let overlap_size = overlap_end - overlap_start;
                let msg = format!(
                    "Block '{}' (0x{:08X}-0x{:08X}) overlaps with block '{}' (0x{:08X}-0x{:08X}). Overlap: 0x{:08X}-0x{:08X} ({} bytes)",
                    name_a,
                    a_start,
                    a_end - 1,
                    name_b,
                    b_start,
                    b_end - 1,
                    overlap_start,
                    overlap_end - 1,
                    overlap_size
                );
                return Err(OutputError::BlockOverlapError(msg).into());
            }
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
