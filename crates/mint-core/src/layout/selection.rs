use super::block::{Block, Config};
use super::error::LayoutError;
use super::fingerprint::fingerprint;
use super::resolved::{LeafBinding, PlannedBlock, ResolvedLayout};
use crate::error::MintError;
use indexmap::IndexMap;
use indexmap::map::Slice;
use rayon::prelude::*;
use std::collections::{HashMap, hash_map::Entry};
use std::path::{Path, PathBuf};

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

pub(crate) struct LayoutSelection<'a> {
    layouts: Vec<(PathBuf, Config)>,
    selectors: Vec<(usize, Option<&'a str>)>,
}

impl<'a> LayoutSelection<'a> {
    pub(crate) fn files(selectors: &'a [BlockSelector]) -> Result<Self, LayoutError> {
        let mut paths = HashMap::new();
        let mut files = IndexMap::new();
        let selectors = selectors
            .iter()
            .map(|selector| {
                let index = match paths.entry(&selector.layout) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        let canonical = selector.layout.canonicalize().map_err(|error| {
                            LayoutError::FileError(format!(
                                "failed to resolve layout file '{}': {error}",
                                selector.layout.display()
                            ))
                        })?;
                        let file = files.entry(canonical);
                        let index = file.index();
                        file.or_insert(&selector.layout);
                        *entry.insert(index)
                    }
                };
                Ok((index, selector.block.as_deref()))
            })
            .collect::<Result<_, LayoutError>>()?;
        let paths = files.into_values().collect::<Vec<_>>();
        let layouts = paths
            .into_par_iter()
            .map(|path| super::load_layout(path).map(|config| (path.clone(), config)))
            .collect::<Result<_, _>>()?;
        Ok(Self { layouts, selectors })
    }

    pub(crate) fn named(
        layouts: impl IntoIterator<Item = (PathBuf, Config)>,
        selectors: &'a [BlockSelector],
    ) -> Result<Self, LayoutError> {
        let mut indexed = IndexMap::new();
        for (name, config) in layouts {
            match indexed.entry(name) {
                indexmap::map::Entry::Occupied(entry) => {
                    return Err(LayoutError::FileError(format!(
                        "duplicate layout name '{}'",
                        entry.key().display()
                    )));
                }
                indexmap::map::Entry::Vacant(entry) => {
                    entry.insert(config);
                }
            }
        }
        let selectors = selectors
            .iter()
            .map(|selector| {
                let index = indexed.get_index_of(&selector.layout).ok_or_else(|| {
                    let mut available = indexed
                        .keys()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>();
                    available.sort_unstable();
                    LayoutError::BlockNotFound(format!(
                        "layout '{}'. Available layouts: {}",
                        selector.layout.display(),
                        available.join(", ")
                    ))
                })?;
                Ok((index, selector.block.as_deref()))
            })
            .collect::<Result<_, LayoutError>>()?;
        Ok(Self {
            layouts: indexed.into_iter().collect(),
            selectors,
        })
    }

    pub(crate) fn prepare(
        &self,
        with_context: fn(String, String, Box<MintError>) -> MintError,
    ) -> Result<PreparedSelection<'_>, MintError> {
        let mut roots = vec![IndexMap::new(); self.layouts.len()];
        let mut order = Vec::new();
        for &(index, name) in &self.selectors {
            let (path, config) = &self.layouts[index];
            for (name, block) in select_blocks(config, name, Some(path))? {
                let (block_index, previous) = roots[index].insert_full(name.as_str(), block);
                if previous.is_none() {
                    order.push((index, block_index));
                }
            }
        }
        if order.is_empty() {
            return Err(LayoutError::NoBlocksProvided.into());
        }
        let groups = self
            .layouts
            .par_iter()
            .zip(roots)
            .map(|((path, config), roots)| {
                plan_layout(config, roots, false)
                    .map(|plan| (path.as_path(), plan))
                    .map_err(|error| match error {
                        LayoutError::InBlock { block, source } => with_context(
                            block,
                            path.display().to_string(),
                            Box::new((*source).into()),
                        ),
                        error => error.into(),
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(PreparedSelection { groups, order })
    }
}

pub(crate) struct PreparedSelection<'a> {
    groups: Vec<(&'a Path, PlannedLayout<'a>)>,
    order: Vec<(usize, usize)>,
}

pub(crate) struct SelectedBlock<'a> {
    pub(crate) name: &'a str,
    pub(crate) path: &'a Path,
    pub(crate) plan: &'a PlannedBlock<'a>,
    pub(crate) layout: &'a PlannedLayout<'a>,
}

impl PreparedSelection<'_> {
    pub(crate) fn blocks(&self) -> impl ExactSizeIterator<Item = SelectedBlock<'_>> {
        self.order.iter().map(|&(index, block_index)| {
            let (path, layout) = &self.groups[index];
            let Some((&name, plan)) = layout.blocks.get_index(block_index) else {
                unreachable!("preparation preserves the selected indices");
            };
            SelectedBlock {
                name,
                path,
                plan,
                layout,
            }
        })
    }
}

pub(crate) fn select_blocks<'a>(
    config: &'a Config,
    name: Option<&str>,
    path: Option<&Path>,
) -> Result<&'a Slice<String, Block>, LayoutError> {
    let Some(name) = name else {
        return Ok(config.blocks.as_slice());
    };
    let Some(index) = config.blocks.get_index_of(name) else {
        let origin = path
            .map(|path| format!(" in '{}'", path.display()))
            .unwrap_or_default();
        return Err(LayoutError::BlockNotFound(format!(
            "'{name}'{origin}. Available blocks: {}",
            block_names(config)
        )));
    };
    Ok(&config.blocks[index..=index])
}

pub(crate) struct PlannedLayout<'a> {
    pub(crate) config: &'a Config,
    blocks: IndexMap<&'a str, PlannedBlock<'a>>,
    pub(crate) fingerprints: IndexMap<&'a str, u64>,
}

/// Validate selected blocks and resolve the shapes needed for their fingerprints.
pub(crate) fn plan_layout<'a>(
    config: &'a Config,
    roots: impl IntoIterator<Item = (&'a str, &'a Block)>,
    hash_roots: bool,
) -> Result<PlannedLayout<'a>, LayoutError> {
    let mut blocks = IndexMap::new();
    let mut required = IndexMap::new();
    for (name, block) in roots {
        let planned = ResolvedLayout::new(&block.data, config.mint.abi)
            .and_then(|resolved| resolved.prepare(name, block, config))
            .map_err(|source| in_block(name, source))?;
        if hash_roots {
            required.insert(name, block);
        }
        for field in &planned.fields {
            if let LeafBinding::Fingerprint(target) = field.binding {
                let entry = required.entry(target);
                if let indexmap::map::Entry::Vacant(entry) = entry {
                    entry.insert(config.blocks.get(target).ok_or_else(|| {
                        in_block(name, LayoutError::BlockNotFound(format!(
                            "fingerprint target '{target}' from block '{name}'. Available blocks: {}",
                            block_names(config)
                        )))
                    })?);
                }
            }
        }
        blocks.insert(name, planned);
    }
    let fingerprints = required
        .into_iter()
        .map(|(name, block)| {
            let digest = match blocks.get(name) {
                Some(planned) => fingerprint(&planned.resolved),
                None => ResolvedLayout::new(&block.data, config.mint.abi)
                    .and_then(|resolved| fingerprint(&resolved)),
            };
            digest
                .map(|value| (name, value))
                .map_err(|source| in_block(name, source))
        })
        .collect::<Result<_, _>>()?;
    Ok(PlannedLayout {
        config,
        blocks,
        fingerprints,
    })
}

fn block_names(config: &Config) -> String {
    config
        .blocks
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn in_block(block: &str, source: LayoutError) -> LayoutError {
    LayoutError::InBlock {
        block: block.to_owned(),
        source: Box::new(source),
    }
}
