use serde::Serialize;

use crate::abi::{Abi, Scalar, ScalarAbi};
use crate::diagnostic::{Category, Error};
use crate::source::Span;
use crate::types::{MAX_RESOLVED_SIZE, SchemaTypes, TypeId, TypeKind};

#[derive(Clone, Debug)]
pub struct ResolvedLayout {
    pub abi: Abi,
    pub start_address: u32,
    pub start_address_span: Span,
    pub padding: u8,
    pub source_name: String,
    pub root: TypeId,
    pub types: Vec<TypeKind>,
    pub layouts: Vec<TypeLayout>,
    pub octet_start: u32,
    pub padding_ranges: Vec<PaddingRange>,
}

#[derive(Clone, Debug)]
pub struct TypeLayout {
    pub size: usize,
    pub alignment: usize,
    pub fields: Vec<FieldLayout>,
    pub array: Option<ArrayLayout>,
    pub scalar: Option<(Scalar, ScalarAbi)>,
}

#[derive(Clone, Debug)]
pub struct FieldLayout {
    pub name: String,
    pub type_id: TypeId,
    pub offset: usize,
    pub size: usize,
    pub alignment: usize,
    pub span: Span,
    pub fingerprint: bool,
    pub spelling: String,
}

#[derive(Clone, Debug)]
pub struct ArrayLayout {
    pub element: TypeId,
    pub dimensions: Vec<u64>,
    pub count: u64,
    pub stride: usize,
}

/// One alignment gap, optionally repeated across array elements.
///
/// Array padding is stored as the first occurrence plus a compact repeat
/// list instead of one range per element.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PaddingRange {
    pub offset: usize,
    pub size: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repeats: Vec<PaddingRepeat>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PaddingRepeat {
    pub count: u64,
    pub stride: usize,
}

impl PaddingRange {
    pub fn occurrence_count(&self) -> u64 {
        self.repeats
            .iter()
            .try_fold(1u64, |acc, repeat| acc.checked_mul(repeat.count))
            .unwrap_or(u64::MAX)
    }

    pub fn total_octets(&self) -> usize {
        usize::try_from(self.occurrence_count())
            .ok()
            .and_then(|count| count.checked_mul(self.size))
            .unwrap_or(usize::MAX)
    }
}

pub fn resolve(schema: SchemaTypes) -> Result<ResolvedLayout, Error> {
    let mut layouts = vec![
        TypeLayout {
            size: 0,
            alignment: 1,
            fields: Vec::new(),
            array: None,
            scalar: None,
        };
        schema.types.len()
    ];
    layout_type(&schema, schema.root, &mut layouts)?;
    let mut padding_ranges = Vec::new();
    collect_padding(&schema, &layouts, schema.root, 0, "", &mut padding_ranges);
    let root_layout = &layouts[schema.root.0];
    if root_layout.size > MAX_RESOLVED_SIZE {
        return Err(fail(
            &schema,
            Category::Schema,
            schema.root_span,
            format!(
                "resolved root size ({} octets) exceeds the 256 MiB limit",
                root_layout.size
            ),
        ));
    }
    let octet_start = octet_start_address(
        schema.abi,
        schema.start_address,
        &schema.source_name,
        schema.start_address_span,
    )?;
    if root_layout.alignment == 0
        || !u64::from(octet_start).is_multiple_of(root_layout.alignment as u64)
    {
        return Err(fail(
            &schema,
            Category::Schema,
            schema.start_address_span,
            format!(
                "start-address 0x{:X} is not aligned to the root record's {}-octet alignment",
                schema.start_address, root_layout.alignment
            ),
        ));
    }
    let output_end = u64::from(octet_start)
        .checked_add(root_layout.size as u64)
        .ok_or_else(|| {
            fail(
                &schema,
                Category::Encoding,
                schema.start_address_span,
                "output range overflows the 32-bit address space",
            )
        })?;
    if output_end > u64::from(u32::MAX) + 1 {
        return Err(fail(
            &schema,
            Category::Encoding,
            schema.start_address_span,
            format!(
                "octet-addressed output range 0x{octet_start:08X}-0x{:08X} exceeds the 32-bit address space",
                output_end.saturating_sub(1)
            ),
        ));
    }
    padding_ranges.sort_by_key(|range| range.offset);
    Ok(ResolvedLayout {
        abi: schema.abi,
        start_address: schema.start_address,
        start_address_span: schema.start_address_span,
        padding: schema.padding,
        source_name: schema.source_name,
        root: schema.root,
        types: schema.types,
        layouts,
        octet_start,
        padding_ranges,
    })
}

pub fn octet_start_address(
    abi: Abi,
    start_address: u32,
    source: &str,
    span: Span,
) -> Result<u32, Error> {
    u64::from(start_address)
        .checked_mul(abi.address_unit_octets() as u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            Error::located(
                Category::Encoding,
                source,
                span,
                "start-address cannot be represented as a 32-bit octet address",
            )
        })
}

fn layout_type(schema: &SchemaTypes, id: TypeId, layouts: &mut [TypeLayout]) -> Result<(), Error> {
    if layouts[id.0].size != 0 || layouts[id.0].scalar.is_some() {
        return Ok(());
    }
    match &schema.types[id.0] {
        TypeKind::Scalar { scalar } => {
            let scalar = *scalar;
            let scalar_abi = schema
                .abi
                .scalar(scalar)
                .map_err(|message| fail(schema, Category::Schema, schema.root_span, message))?;
            layouts[id.0] = TypeLayout {
                size: scalar_abi.storage_size,
                alignment: scalar_abi.alignment,
                fields: Vec::new(),
                array: None,
                scalar: Some((scalar, scalar_abi)),
            };
        }
        TypeKind::Array {
            element,
            dimensions,
        } => {
            let element = *element;
            let dimensions = dimensions.clone();
            layout_type(schema, element, layouts)?;
            let stride = layouts[element.0].size;
            let alignment = layouts[element.0].alignment;
            let mut count = 1u64;
            for dim in &dimensions {
                count = count.checked_mul(*dim).ok_or_else(|| {
                    fail(
                        schema,
                        Category::Schema,
                        schema.root_span,
                        "array element count overflow",
                    )
                })?;
            }
            let size = usize::try_from(count)
                .ok()
                .and_then(|count| count.checked_mul(stride))
                .ok_or_else(|| {
                    fail(
                        schema,
                        Category::Schema,
                        schema.root_span,
                        "array byte count overflow",
                    )
                })?;
            layouts[id.0] = TypeLayout {
                size,
                alignment,
                fields: Vec::new(),
                array: Some(ArrayLayout {
                    element,
                    dimensions,
                    count,
                    stride,
                }),
                scalar: None,
            };
        }
        TypeKind::Record { fields } => {
            let child_ids: Vec<TypeId> = fields.iter().map(|field| field.type_id).collect();
            for child_id in child_ids {
                layout_type(schema, child_id, layouts)?;
            }
            let mut field_layouts = Vec::new();
            let mut cursor = 0usize;
            let mut alignment = 1usize;
            for field in fields {
                let child_size = layouts[field.type_id.0].size;
                let child_align = layouts[field.type_id.0].alignment;
                let aligned = aligned_offset(cursor, child_align).map_err(|_| {
                    fail(schema, Category::Schema, field.span, "alignment overflow")
                })?;
                cursor = aligned.checked_add(child_size).ok_or_else(|| {
                    fail(schema, Category::Schema, field.span, "record size overflow")
                })?;
                alignment = alignment.max(child_align);
                field_layouts.push(FieldLayout {
                    name: field.name.clone(),
                    type_id: field.type_id,
                    offset: aligned,
                    size: child_size,
                    alignment: child_align,
                    span: field.span,
                    fingerprint: field.fingerprint,
                    spelling: field.spelling.clone(),
                });
            }
            if cursor > 1 {
                alignment = alignment.max(schema.abi.family().min_aggregate_alignment());
            }
            let size = match field_layouts.last() {
                Some(last) => aligned_offset(cursor, alignment)
                    .map_err(|_| fail(schema, Category::Schema, last.span, "alignment overflow"))?,
                None => aligned_offset(cursor, alignment).map_err(|_| {
                    fail(
                        schema,
                        Category::Schema,
                        schema.root_span,
                        "alignment overflow",
                    )
                })?,
            };
            layouts[id.0] = TypeLayout {
                size,
                alignment,
                fields: field_layouts,
                array: None,
                scalar: None,
            };
        }
    }
    Ok(())
}

fn collect_padding(
    schema: &SchemaTypes,
    layouts: &[TypeLayout],
    id: TypeId,
    base: usize,
    path: &str,
    padding: &mut Vec<PaddingRange>,
) {
    match &schema.types[id.0] {
        TypeKind::Record { .. } => {
            let layout = &layouts[id.0];
            let mut cursor = 0usize;
            for field in &layout.fields {
                if field.offset > cursor {
                    padding.push(PaddingRange {
                        offset: base + cursor,
                        size: field.offset - cursor,
                        path: path.to_owned(),
                        repeats: Vec::new(),
                    });
                }
                collect_padding(
                    schema,
                    layouts,
                    field.type_id,
                    base + field.offset,
                    &child_path(path, &field.name),
                    padding,
                );
                cursor = field.offset + field.size;
            }
            if layout.size > cursor {
                padding.push(PaddingRange {
                    offset: base + cursor,
                    size: layout.size - cursor,
                    path: path.to_owned(),
                    repeats: Vec::new(),
                });
            }
        }
        TypeKind::Array { .. } => {
            let layout = &layouts[id.0];
            let Some(array) = &layout.array else {
                return;
            };
            let count = array.count;
            // One element prototype plus a compact repeat; never walk each index.
            let mut proto = Vec::new();
            collect_padding(
                schema,
                layouts,
                array.element,
                0,
                &array_path(path),
                &mut proto,
            );
            for mut range in proto {
                range.offset = base.saturating_add(range.offset);
                if count > 1 {
                    range.repeats.push(PaddingRepeat {
                        count,
                        stride: array.stride,
                    });
                }
                padding.push(range);
            }
        }
        TypeKind::Scalar { .. } => {}
    }
}

pub(crate) fn child_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}.{name}")
    }
}

pub(crate) fn array_path(path: &str) -> String {
    if path.is_empty() {
        "[]".to_owned()
    } else {
        format!("{path}[]")
    }
}

fn aligned_offset(offset: usize, alignment: usize) -> Result<usize, ()> {
    if alignment == 0 {
        return Err(());
    }
    let remainder = offset % alignment;
    if remainder == 0 {
        return Ok(offset);
    }
    offset.checked_add(alignment - remainder).ok_or(())
}

fn fail(schema: &SchemaTypes, category: Category, span: Span, message: impl Into<String>) -> Error {
    Error::located(category, &schema.source_name, span, message)
}

impl ResolvedLayout {
    pub fn root_layout(&self) -> &TypeLayout {
        &self.layouts[self.root.0]
    }

    pub fn octet_start(&self) -> Result<u32, Error> {
        Ok(self.octet_start)
    }

    pub fn padding_octets(&self) -> usize {
        self.padding_ranges
            .iter()
            .map(PaddingRange::total_octets)
            .sum()
    }
}
