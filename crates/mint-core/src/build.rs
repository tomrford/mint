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
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BlockStat {
    pub name: String,
    pub start_address: u32,
    pub allocated_size: u32,
    pub used_size: u32,
    pub checksum_values: Vec<u32>,
}

#[derive(Debug)]
pub struct BuildStats {
    pub blocks_processed: usize,
    pub total_allocated: usize,
    pub total_used: usize,
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
            total_used: 0,
            total_duration: Duration::from_secs(0),
            block_stats: Vec::new(),
        }
    }

    pub fn add_block(&mut self, stat: BlockStat) {
        self.blocks_processed += 1;
        self.total_allocated += stat.allocated_size as usize;
        self.total_used += stat.used_size as usize;
        self.block_stats.push(stat);
    }

    pub fn space_used_pct(&self) -> f64 {
        if self.total_allocated == 0 {
            0.0
        } else {
            (self.total_used as f64 / self.total_allocated as f64) * 100.0
        }
    }
}

#[derive(Clone)]
pub struct BuildRequest<'a> {
    pub blocks: Vec<BlockNames>,
    pub data_source: Option<&'a dyn DataSource>,
    pub strict: bool,
    pub capture_values: bool,
}

#[derive(Debug)]
pub struct NamedLayout {
    pub name: String,
    pub config: Config,
}

pub struct BuildFromLayoutsRequest<'a> {
    pub layouts: Vec<NamedLayout>,
    pub blocks: Vec<BlockNames>,
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
        self.output_file(format, record_width).render()
    }
}

#[derive(Debug, Clone)]
pub struct BlockNames {
    pub name: String,
    pub file: String,
}

#[derive(Debug, Clone)]
struct ResolvedBlock {
    name: String,
    file: String,
}

struct BlockBuildResult {
    block_names: BlockNames,
    data_range: DataRange,
    stat: BlockStat,
    used_values: Option<serde_json::Value>,
}

pub fn build(request: BuildRequest<'_>) -> Result<BuildArtifact, MintError> {
    if request.blocks.is_empty() {
        return Err(LayoutError::NoBlocksProvided.into());
    }

    let (resolved_blocks, layouts) = resolve_blocks(&request.blocks)?;
    build_resolved(
        resolved_blocks,
        &layouts,
        request.data_source,
        request.strict,
        request.capture_values,
    )
}

pub fn build_from_layouts(
    request: BuildFromLayoutsRequest<'_>,
) -> Result<BuildArtifact, MintError> {
    if request.blocks.is_empty() {
        return Err(LayoutError::NoBlocksProvided.into());
    }

    let layouts = collect_named_layouts(request.layouts)?;
    let resolved_blocks = resolve_blocks_from_layouts(&request.blocks, &layouts)?;
    build_resolved(
        resolved_blocks,
        &layouts,
        request.data_source,
        request.strict,
        request.capture_values,
    )
}

fn build_resolved(
    resolved_blocks: Vec<ResolvedBlock>,
    layouts: &HashMap<String, Config>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BuildArtifact, MintError> {
    let start_time = Instant::now();
    let mut results = build_bytestreams(
        &resolved_blocks,
        layouts,
        data_source,
        strict,
        capture_values,
    )?;

    let used_values = if capture_values {
        Some(take_used_values_report(&mut results)?)
    } else {
        None
    };

    let (ranges, mut stats) = collect_results(results)?;
    stats.total_duration = start_time.elapsed();

    Ok(BuildArtifact {
        ranges,
        stats,
        used_values,
    })
}

fn collect_named_layouts(
    layouts: Vec<NamedLayout>,
) -> Result<HashMap<String, Config>, LayoutError> {
    let mut out = HashMap::with_capacity(layouts.len());
    for layout in layouts {
        if out.insert(layout.name.clone(), layout.config).is_some() {
            return Err(LayoutError::FileError(format!(
                "duplicate layout name '{}'",
                layout.name
            )));
        }
    }
    Ok(out)
}

fn resolve_blocks_from_layouts(
    block_args: &[BlockNames],
    layouts: &HashMap<String, Config>,
) -> Result<Vec<ResolvedBlock>, LayoutError> {
    let mut resolved = Vec::new();
    for arg in block_args {
        let layout = layouts.get(&arg.file).ok_or_else(|| {
            let available = layouts.keys().cloned().collect::<Vec<_>>().join(", ");
            LayoutError::BlockNotFound(format!(
                "layout '{}'. Available layouts: {}",
                arg.file, available
            ))
        })?;

        if arg.name.is_empty() {
            for block_name in layout.blocks.keys() {
                resolved.push(ResolvedBlock {
                    name: block_name.clone(),
                    file: arg.file.clone(),
                });
            }
        } else if layout.blocks.contains_key(&arg.name) {
            resolved.push(ResolvedBlock {
                name: arg.name.clone(),
                file: arg.file.clone(),
            });
        } else {
            let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(LayoutError::BlockNotFound(format!(
                "'{}' in '{}'. Available blocks: {}",
                arg.name, arg.file, available_blocks
            )));
        }
    }

    let mut seen = HashSet::new();
    Ok(resolved
        .into_iter()
        .filter(|b| seen.insert((b.file.clone(), b.name.clone())))
        .collect())
}

fn resolve_blocks(
    block_args: &[BlockNames],
) -> Result<(Vec<ResolvedBlock>, HashMap<String, Config>), LayoutError> {
    let unique_files: HashSet<String> = block_args.iter().map(|b| b.file.clone()).collect();

    let layouts: Result<HashMap<String, Config>, LayoutError> = unique_files
        .par_iter()
        .map(|file| layout::load_layout(file).map(|cfg| (file.clone(), cfg)))
        .collect();

    let layouts = layouts?;

    let mut resolved = Vec::new();
    for arg in block_args {
        if arg.name.is_empty() {
            let layout = &layouts[&arg.file];
            for block_name in layout.blocks.keys() {
                resolved.push(ResolvedBlock {
                    name: block_name.clone(),
                    file: arg.file.clone(),
                });
            }
        } else {
            let layout = &layouts[&arg.file];
            if !layout.blocks.contains_key(&arg.name) {
                let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(LayoutError::BlockNotFound(format!(
                    "'{}' in '{}'. Available blocks: {}",
                    arg.name, arg.file, available_blocks
                )));
            }
            resolved.push(ResolvedBlock {
                name: arg.name.clone(),
                file: arg.file.clone(),
            });
        }
    }

    let mut seen = HashSet::new();
    let deduplicated: Vec<ResolvedBlock> = resolved
        .into_iter()
        .filter(|b| seen.insert((b.file.clone(), b.name.clone())))
        .collect();

    Ok((deduplicated, layouts))
}

fn build_bytestreams(
    blocks: &[ResolvedBlock],
    layouts: &HashMap<String, Config>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<Vec<BlockBuildResult>, MintError> {
    blocks
        .par_iter()
        .map(|resolved| {
            build_single_bytestream(resolved, layouts, data_source, strict, capture_values)
        })
        .collect()
}

fn build_single_bytestream(
    resolved: &ResolvedBlock,
    layouts: &HashMap<String, Config>,
    data_source: Option<&dyn DataSource>,
    strict: bool,
    capture_values: bool,
) -> Result<BlockBuildResult, MintError> {
    let result = (|| {
        let layout = layouts.get(&resolved.file).ok_or_else(|| {
            LayoutError::FileError(format!(
                "resolved layout missing from build map: {}",
                resolved.file
            ))
        })?;
        let block = layout.blocks.get(&resolved.name).ok_or_else(|| {
            let available_blocks = layout.blocks.keys().cloned().collect::<Vec<_>>().join(", ");
            LayoutError::BlockNotFound(format!(
                "'{}' in '{}'. Available blocks: {}",
                resolved.name, resolved.file, available_blocks
            ))
        })?;
        let mut collector = ValueCollector::new();
        let mut noop = NoopValueSink;
        let value_sink = if capture_values {
            &mut collector as &mut dyn crate::layout::used_values::ValueSink
        } else {
            &mut noop as &mut dyn crate::layout::used_values::ValueSink
        };

        let build_output = block.build_bytestream(data_source, &layout.mint, strict, value_sink)?;

        let data_range = output::bytestream_to_datarange(
            build_output.bytestream,
            &block.header,
            build_output.padding_count,
        )?;

        let stat = BlockStat {
            name: resolved.name.clone(),
            start_address: data_range.start_address,
            allocated_size: data_range.allocated_size,
            used_size: data_range.used_size,
            checksum_values: build_output.checksum_values,
        };

        Ok(BlockBuildResult {
            block_names: BlockNames {
                name: resolved.name.clone(),
                file: resolved.file.clone(),
            },
            data_range,
            stat,
            used_values: capture_values.then(|| collector.into_value()),
        })
    })();

    result.map_err(|e| MintError::InBlock {
        block_name: resolved.name.clone(),
        layout_file: resolved.file.clone(),
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
            (r.block_names.name, r.data_range)
        })
        .collect();

    check_overlaps(&named_ranges)?;
    let ranges = named_ranges.into_iter().map(|(_, r)| r).collect();
    Ok((ranges, stats))
}

fn check_overlaps(named_ranges: &[(String, DataRange)]) -> Result<(), MintError> {
    for i in 0..named_ranges.len() {
        for j in (i + 1)..named_ranges.len() {
            let (ref name_a, ref range_a) = named_ranges[i];
            let (ref name_b, ref range_b) = named_ranges[j];
            let a_start = range_a.start_address;
            let a_end = a_start + range_a.allocated_size;
            let b_start = range_b.start_address;
            let b_end = b_start + range_b.allocated_size;

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
            .entry(result.block_names.file.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let serde_json::Value::Object(blocks) = file_entry else {
            return Err(OutputError::FileError(
                "JSON export contains unexpected non-object entry.".to_owned(),
            )
            .into());
        };
        if blocks.contains_key(&result.block_names.name) {
            return Err(OutputError::FileError(format!(
                "Duplicate block '{}' in JSON export for file '{}'.",
                result.block_names.name, result.block_names.file
            ))
            .into());
        }
        blocks.insert(result.block_names.name.clone(), value);
    }
    Ok(serde_json::Value::Object(report))
}
