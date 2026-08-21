use crate::abi::{Abi, Scalar, ScalarAbi};
use crate::diagnostic::{Category, Diagnostic, Error};
use crate::source::Span;
use crate::types::{MAX_RESOLVED_SIZE, SchemaTypes, TypeId, TypeKind};

#[derive(Clone, Debug)]
pub struct ResolvedLayout {
    pub abi: Abi,
    pub start_address: u32,
    pub start_address_span: Span,
    pub padding: u8,
    pub source_name: String,
    pub root_name: String,
    pub root: TypeId,
    pub types: Vec<TypeKind>,
    pub layouts: Vec<TypeLayout>,
    pub fingerprint_field: Option<String>,
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
    pub stride: usize,
}

/// One alignment gap, optionally repeated across array elements.
///
/// Array padding is stored as the first occurrence plus a compact repeat
/// list instead of one range per element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaddingRange {
    pub offset: usize,
    pub size: usize,
    pub path: String,
    pub repeats: Vec<PaddingRepeat>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        return Err(layout_error(
            schema.source_name.as_str(),
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
    if root_layout.alignment == 0 || !octet_start.is_multiple_of(root_layout.alignment as u64) {
        return Err(layout_error(
            schema.source_name.as_str(),
            Category::Schema,
            schema.start_address_span,
            format!(
                "start-address 0x{:X} is not aligned to the root record's {}-octet alignment",
                schema.start_address, root_layout.alignment
            ),
        ));
    }
    let output_end = octet_start
        .checked_add(root_layout.size as u64)
        .ok_or_else(|| {
            layout_error(
                schema.source_name.as_str(),
                Category::Encoding,
                schema.start_address_span,
                "output range overflows the 32-bit address space",
            )
        })?;
    if output_end > u64::from(u32::MAX) + 1 {
        return Err(layout_error(
            schema.source_name.as_str(),
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
        root_name: schema.root_name,
        root: schema.root,
        types: schema.types,
        layouts,
        fingerprint_field: schema.fingerprint_field,
        padding_ranges,
    })
}

pub fn octet_start_address(
    abi: Abi,
    start_address: u32,
    source: &str,
    span: Span,
) -> Result<u64, Error> {
    u64::from(start_address)
        .checked_mul(abi.address_unit_octets() as u64)
        .and_then(|value| u32::try_from(value).ok().map(u64::from))
        .ok_or_else(|| {
            layout_error(
                source,
                Category::Encoding,
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
        TypeKind::Scalar { scalar, .. } => {
            let scalar_abi = schema.abi.scalar(*scalar).map_err(|message| {
                layout_error(
                    schema.source_name.as_str(),
                    Category::Schema,
                    schema.root_span,
                    message,
                )
            })?;
            layouts[id.0] = TypeLayout {
                size: scalar_abi.storage_size,
                alignment: scalar_abi.alignment,
                fields: Vec::new(),
                array: None,
                scalar: Some((*scalar, scalar_abi)),
            };
        }
        TypeKind::Enum => {
            return Err(layout_error(
                schema.source_name.as_str(),
                Category::Schema,
                schema.root_span,
                "enum-typed members are not supported",
            ));
        }
        TypeKind::Array {
            element,
            dimensions,
        } => {
            let element = *element;
            let dimensions = dimensions.clone();
            layout_type(schema, element, layouts)?;
            let elem = layouts[element.0].clone();
            let mut count = 1usize;
            for dim in &dimensions {
                let dim = usize::try_from(*dim)
                    .map_err(|_| size_error(schema, "array extent exceeds usize"))?;
                count = count
                    .checked_mul(dim)
                    .ok_or_else(|| size_error(schema, "array element count overflow"))?;
            }
            let stride = elem.size;
            let size = count
                .checked_mul(stride)
                .ok_or_else(|| size_error(schema, "array byte count overflow"))?;
            layouts[id.0] = TypeLayout {
                size,
                alignment: elem.alignment,
                fields: Vec::new(),
                array: Some(ArrayLayout {
                    element,
                    dimensions,
                    stride,
                }),
                scalar: None,
            };
        }
        TypeKind::Record { fields, .. } => {
            let fields = fields.clone();
            let mut field_layouts = Vec::new();
            let mut cursor = 0usize;
            let mut alignment = 1usize;
            for field in &fields {
                layout_type(schema, field.type_id, layouts)?;
                let child = layouts[field.type_id.0].clone();
                let aligned = aligned_offset(cursor, child.alignment)
                    .map_err(|_| size_error_at(schema, field.span, "alignment overflow"))?;
                cursor = aligned
                    .checked_add(child.size)
                    .ok_or_else(|| size_error_at(schema, field.span, "record size overflow"))?;
                alignment = alignment.max(child.alignment);
                field_layouts.push(FieldLayout {
                    name: field.name.clone(),
                    type_id: field.type_id,
                    offset: aligned,
                    size: child.size,
                    alignment: child.alignment,
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
                    .map_err(|_| size_error_at(schema, last.span, "alignment overflow"))?,
                None => aligned_offset(cursor, alignment)
                    .map_err(|_| size_error(schema, "alignment overflow"))?,
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
            let Some(count) = array_element_count(array, layout.size) else {
                return;
            };
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
        TypeKind::Scalar { .. } | TypeKind::Enum => {}
    }
}

fn array_element_count(array: &ArrayLayout, size: usize) -> Option<u64> {
    if array.stride == 0 || size == 0 {
        return None;
    }
    if let Some(count) = array
        .dimensions
        .iter()
        .try_fold(1u64, |acc, dim| acc.checked_mul(*dim))
    {
        return (count > 0).then_some(count);
    }
    let count = u64::try_from(size / array.stride).ok()?;
    (count > 0).then_some(count)
}

fn child_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}.{name}")
    }
}

fn array_path(path: &str) -> String {
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

fn layout_error(source: &str, category: Category, span: Span, message: impl Into<String>) -> Error {
    Error::one(Diagnostic::new(category, source, message).at(span))
}

fn size_error(schema: &SchemaTypes, message: &str) -> Error {
    layout_error(
        schema.source_name.as_str(),
        Category::Schema,
        schema.root_span,
        message,
    )
}

fn size_error_at(schema: &SchemaTypes, span: Span, message: &str) -> Error {
    layout_error(schema.source_name.as_str(), Category::Schema, span, message)
}

impl ResolvedLayout {
    pub fn root_layout(&self) -> &TypeLayout {
        &self.layouts[self.root.0]
    }

    pub fn octet_start(&self) -> Result<u32, Error> {
        let start = octet_start_address(
            self.abi,
            self.start_address,
            &self.source_name,
            self.start_address_span,
        )?;
        u32::try_from(start).map_err(|_| {
            layout_error(
                self.source_name.as_str(),
                Category::Encoding,
                self.start_address_span,
                "start-address does not fit a 32-bit octet address",
            )
        })
    }

    pub fn padding_octets(&self) -> usize {
        self.padding_ranges
            .iter()
            .map(PaddingRange::total_octets)
            .sum()
    }
}
