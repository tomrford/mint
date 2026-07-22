use crate::data::DataSource;
use crate::error::MintError;
use crate::layout;
use crate::layout::block::Config;
use crate::layout::error::LayoutError;
use crate::layout::used_values::{NoopValueSink, ValueCollector};
use crate::output;
use crate::output::error::OutputError;
use crate::output::{DataRange, OutputFile, OutputFormat};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
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
        let unit_octets = self.address_unit_bits / 8;
        debug_assert!(
            unit_octets > 0
                && self.address_unit_bits.is_multiple_of(8)
                && (self.allocated_size as usize).is_multiple_of(unit_octets),
            "build statistics must contain a whole number of target address units"
        );
        u64::from(self.allocated_size) / unit_octets as u64
    }
}

#[derive(Debug)]
pub struct BuildStats {
    pub blocks_processed: usize,
    pub total_allocated: u64,
    pub total_reserved: u64,
    pub total_duration: Duration,
    pub block_stats: Vec<BlockStat>,
}

impl Default for BuildStats {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildStats {
    pub fn new() -> Self {
        Self {
            blocks_processed: 0,
            total_allocated: 0,
            total_reserved: 0,
            total_duration: Duration::from_secs(0),
            block_stats: Vec::new(),
        }
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
    pub fn output_file(&self, format: OutputFormat, record_width: usize) -> OutputFile {
        OutputFile {
            ranges: self.ranges.clone(),
            format,
            record_width,
        }
    }

    pub fn render(&self, format: OutputFormat, record_width: usize) -> Result<String, OutputError> {
        output::emit_hex(&self.ranges, record_width, format)
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
pub(crate) struct ResolvedBlock {
    pub(crate) name: String,
    pub(crate) layout: PathBuf,
}

struct BlockBuildResult {
    selector: BlockSelector,
    data_range: DataRange,
    stat: BlockStat,
    used_values: Option<serde_json::Value>,
}

pub fn build(request: BuildRequest<'_>) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();

    if request.blocks.is_empty() {
        return Err(LayoutError::NoBlocksProvided.into());
    }

    let (resolved_blocks, layouts) = resolve_blocks(&request.blocks)?;
    let mut artifact = build_resolved(
        resolved_blocks,
        &layouts,
        request.data_source,
        request.strict,
        request.capture_values,
    )?;
    artifact.stats.total_duration = start_time.elapsed();
    Ok(artifact)
}

pub fn build_from_layouts(
    request: BuildFromLayoutsRequest<'_>,
) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();

    if request.blocks.is_empty() {
        return Err(LayoutError::NoBlocksProvided.into());
    }

    let layouts = collect_named_layouts(request.layouts)?;
    let resolved_blocks = resolve_blocks_from_layouts(&request.blocks, &layouts)?;
    let mut artifact = build_resolved(
        resolved_blocks,
        &layouts,
        request.data_source,
        request.strict,
        request.capture_values,
    )?;
    artifact.stats.total_duration = start_time.elapsed();
    Ok(artifact)
}

fn build_resolved(
    resolved_blocks: Vec<ResolvedBlock>,
    layouts: &HashMap<PathBuf, Config>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BuildArtifact, MintError> {
    let fingerprints = calculate_layout_fingerprints(layouts, &resolved_blocks)?;
    let mut results = build_bytestreams(
        &resolved_blocks,
        layouts,
        &fingerprints,
        data_source,
        strict,
        capture_values,
    )?;

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

fn calculate_layout_fingerprints(
    layouts: &HashMap<PathBuf, Config>,
    resolved_blocks: &[ResolvedBlock],
) -> Result<HashMap<PathBuf, HashMap<String, u64>>, LayoutError> {
    layouts
        .par_iter()
        .map(|(path, config)| {
            let roots = resolved_blocks
                .iter()
                .filter(|block| &block.layout == path)
                .map(|block| block.name.as_str());
            layout::fingerprint::calculate_scoped(config, roots, false)
                .map(|values| (path.clone(), values.into_iter().collect()))
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

fn resolve_blocks_from_layouts(
    block_args: &[BlockSelector],
    layouts: &HashMap<PathBuf, Config>,
) -> Result<Vec<ResolvedBlock>, LayoutError> {
    let mut resolved = Vec::new();
    for arg in block_args {
        let layout = layouts.get(&arg.layout).ok_or_else(|| {
            let available = layouts
                .keys()
                .map(|layout| layout.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            LayoutError::BlockNotFound(format!(
                "layout '{}'. Available layouts: {}",
                arg.layout.display(),
                available
            ))
        })?;

        if let Some(block_name) = &arg.block {
            if layout.blocks.contains_key(block_name) {
                resolved.push(ResolvedBlock {
                    name: block_name.clone(),
                    layout: arg.layout.clone(),
                });
            } else {
                let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(LayoutError::BlockNotFound(format!(
                    "'{}' in '{}'. Available blocks: {}",
                    block_name,
                    arg.layout.display(),
                    available_blocks
                )));
            }
        } else {
            for block_name in layout.blocks.keys() {
                resolved.push(ResolvedBlock {
                    name: block_name.clone(),
                    layout: arg.layout.clone(),
                });
            }
        }
    }

    let mut seen = HashSet::new();
    Ok(resolved
        .into_iter()
        .filter(|b| seen.insert((b.layout.clone(), b.name.clone())))
        .collect())
}

pub(crate) fn resolve_blocks(
    block_args: &[BlockSelector],
) -> Result<(Vec<ResolvedBlock>, HashMap<PathBuf, Config>), LayoutError> {
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

    let resolved = resolve_blocks_from_layouts(&normalized_blocks, &layouts)?;
    Ok((resolved, layouts))
}

fn build_bytestreams(
    blocks: &[ResolvedBlock],
    layouts: &HashMap<PathBuf, Config>,
    fingerprints: &HashMap<PathBuf, HashMap<String, u64>>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<Vec<BlockBuildResult>, MintError> {
    blocks
        .par_iter()
        .map(|resolved| {
            build_single_bytestream(
                resolved,
                layouts,
                fingerprints,
                data_source,
                strict,
                capture_values,
            )
        })
        .collect()
}

fn build_single_bytestream(
    resolved: &ResolvedBlock,
    layouts: &HashMap<PathBuf, Config>,
    fingerprints: &HashMap<PathBuf, HashMap<String, u64>>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BlockBuildResult, MintError> {
    let result = (|| {
        let layout = layouts.get(&resolved.layout).ok_or_else(|| {
            LayoutError::FileError(format!(
                "resolved layout missing from build map: {}",
                resolved.layout.display()
            ))
        })?;
        let block = layout.blocks.get(&resolved.name).ok_or_else(|| {
            let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
            LayoutError::BlockNotFound(format!(
                "'{}' in '{}'. Available blocks: {}",
                resolved.name,
                resolved.layout.display(),
                available_blocks
            ))
        })?;
        let fingerprints = fingerprints.get(&resolved.layout).ok_or_else(|| {
            LayoutError::FileError(format!(
                "resolved layout missing from fingerprint map: {}",
                resolved.layout.display()
            ))
        })?;
        let mut collector = ValueCollector::new();
        let mut noop = NoopValueSink;
        let value_sink = if capture_values {
            &mut collector as &mut dyn crate::layout::used_values::ValueSink
        } else {
            &mut noop as &mut dyn crate::layout::used_values::ValueSink
        };

        let build_output = block.emit(
            &resolved.name,
            fingerprints,
            data_source,
            &layout.mint,
            strict,
            value_sink,
        )?;

        let data_range = output::bytestream_to_datarange(
            build_output.bytestream,
            &block.header,
            layout.mint.abi,
        )?;

        let stat = BlockStat {
            layout: resolved.layout.clone(),
            block: resolved.name.clone(),
            start_address: data_range.start_address,
            address_unit_bits: data_range.address_unit_bits,
            allocated_size: data_range.allocated_size,
            reserved_size: data_range.reserved_size,
            checksum_values: build_output.checksum_values,
        };

        Ok(BlockBuildResult {
            selector: BlockSelector {
                layout: resolved.layout.clone(),
                block: Some(resolved.name.clone()),
            },
            data_range,
            stat,
            used_values: capture_values.then(|| collector.into_value()),
        })
    })();

    result.map_err(|e| MintError::InBlock {
        block_name: resolved.name.clone(),
        layout_file: resolved.layout.display().to_string(),
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
            stats.add_block(r.stat);
            (r.selector.display_name(), r.data_range)
        })
        .collect();

    check_overlaps(&named_ranges)?;
    let ranges = named_ranges.into_iter().map(|(_, r)| r).collect();
    Ok((ranges, stats))
}

const ADDRESS_SPACE_SIZE: u64 = u32::MAX as u64 + 1;

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

    let mut ranges = Vec::with_capacity(named_ranges.len());
    for (name, range) in named_ranges {
        let (start, end) = checked_range_bounds(name, range)?;
        ranges.push((name.as_str(), start, end));
    }

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

fn checked_range_bounds(name: &str, range: &DataRange) -> Result<(u64, u64), MintError> {
    let start = u64::from(range.output_start_address()?);
    let end = start + u64::from(range.allocated_size);
    if end > ADDRESS_SPACE_SIZE {
        return Err(OutputError::AddressRangeError(format!(
            "Block '{}' range 0x{:08X}-0x{:08X} exceeds the 32-bit address space",
            name,
            start,
            end.saturating_sub(1)
        ))
        .into());
    }
    Ok((start, end))
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
            .entry(result.selector.layout.display().to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let serde_json::Value::Object(blocks) = file_entry else {
            return Err(OutputError::FileError(
                "JSON export contains unexpected non-object entry.".to_owned(),
            )
            .into());
        };
        let block_name = result.selector.block.as_deref().ok_or_else(|| {
            OutputError::FileError("resolved build result is missing a block name.".to_owned())
        })?;
        if blocks.contains_key(block_name) {
            return Err(OutputError::FileError(format!(
                "Duplicate block '{}' in JSON export for file '{}'.",
                block_name,
                result.selector.layout.display()
            ))
            .into());
        }
        blocks.insert(block_name.to_owned(), value);
    }
    Ok(serde_json::Value::Object(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start_address: u32, allocated_size: u32) -> DataRange {
        range_with_unit(start_address, allocated_size, 8)
    }

    fn range_with_unit(
        start_address: u32,
        allocated_size: u32,
        address_unit_bits: usize,
    ) -> DataRange {
        let reserved_size = (address_unit_bits / 8) as u32;
        DataRange {
            start_address,
            address_unit_bits,
            bytestream: vec![0; reserved_size as usize],
            reserved_size,
            allocated_size,
        }
    }

    #[test]
    fn c28x_overlap_checks_use_scaled_octet_addresses() {
        let adjacent = vec![
            ("first".to_owned(), range_with_unit(0x1000, 4, 16)),
            ("second".to_owned(), range_with_unit(0x1002, 2, 16)),
        ];
        check_overlaps(&adjacent).expect("adjacent C28x ranges do not overlap");

        let overlapping = vec![
            ("first".to_owned(), range_with_unit(0x1000, 4, 16)),
            ("second".to_owned(), range_with_unit(0x1001, 2, 16)),
        ];
        check_overlaps(&overlapping).expect_err("overlapping C28x ranges should fail");
    }

    #[test]
    fn allocated_ranges_reject_overlap_but_allow_adjacency() {
        let adjacent = vec![
            ("first".to_owned(), range(0x1000, 0x10)),
            ("second".to_owned(), range(0x1010, 0x10)),
        ];
        check_overlaps(&adjacent).expect("adjacent ranges do not overlap");

        let overlapping = vec![
            ("first".to_owned(), range(0x1000, 0x11)),
            ("second".to_owned(), range(0x1010, 0x10)),
        ];
        let error = check_overlaps(&overlapping).expect_err("overlap should be rejected");
        assert!(error.to_string().contains("overlaps"), "{error}");
    }
}
