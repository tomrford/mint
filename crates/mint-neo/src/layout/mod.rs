use crate::abi::{Abi, Scalar, ScalarAbi};
use crate::diagnostic::{Category, Diagnostic, Error};
use crate::source::Span;
use crate::types::{MAX_RESOLVED_SIZE, SchemaTypes, TypeId, TypeKind};

#[derive(Clone, Debug)]
pub struct ResolvedLayout {
    pub abi: Abi,
    pub start_address: u32,
    pub padding: u8,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaddingRange {
    pub offset: usize,
    pub size: usize,
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
    collect_padding(&schema, &layouts, schema.root, 0, &mut padding_ranges);
    let root_layout = &layouts[schema.root.0];
    if root_layout.size > MAX_RESOLVED_SIZE {
        return Err(Error::one(Diagnostic::new(
            Category::Schema,
            &schema.root_name,
            format!(
                "resolved root size ({} octets) exceeds the 256 MiB limit",
                root_layout.size
            ),
        )));
    }
    let octet_start = octet_start_address(schema.abi, schema.start_address)?;
    if root_layout.alignment == 0 || !octet_start.is_multiple_of(root_layout.alignment as u64) {
        return Err(Error::one(Diagnostic::new(
            Category::Schema,
            &schema.root_name,
            format!(
                "start-address 0x{octet_start:X} is not aligned to the root record's {}-octet alignment",
                root_layout.alignment
            ),
        )));
    }
    let output_end = octet_start
        .checked_add(root_layout.size as u64)
        .ok_or_else(|| {
            Error::one(Diagnostic::new(
                Category::Encoding,
                &schema.root_name,
                "output range overflows the 32-bit address space",
            ))
        })?;
    if output_end > u64::from(u32::MAX) + 1 {
        return Err(Error::one(Diagnostic::new(
            Category::Encoding,
            &schema.root_name,
            format!(
                "octet-addressed output range 0x{octet_start:08X}-0x{:08X} exceeds the 32-bit address space",
                output_end.saturating_sub(1)
            ),
        )));
    }
    padding_ranges.sort_by_key(|range| range.offset);
    Ok(ResolvedLayout {
        abi: schema.abi,
        start_address: schema.start_address,
        padding: schema.padding,
        root_name: schema.root_name,
        root: schema.root,
        types: schema.types,
        layouts,
        fingerprint_field: schema.fingerprint_field,
        padding_ranges,
    })
}

pub fn octet_start_address(abi: Abi, start_address: u32) -> Result<u64, Error> {
    u64::from(start_address)
        .checked_mul(abi.address_unit_octets() as u64)
        .and_then(|value| u32::try_from(value).ok().map(u64::from))
        .ok_or_else(|| {
            Error::one(Diagnostic::new(
                Category::Encoding,
                "header",
                "start-address cannot be represented as a 32-bit octet address",
            ))
        })
}

fn layout_type(schema: &SchemaTypes, id: TypeId, layouts: &mut [TypeLayout]) -> Result<(), Error> {
    if layouts[id.0].size != 0 || layouts[id.0].scalar.is_some() {
        return Ok(());
    }
    match &schema.types[id.0] {
        TypeKind::Scalar { scalar, .. } => {
            let scalar_abi = schema.abi.scalar(*scalar).map_err(|message| {
                Error::one(Diagnostic::new(
                    Category::Schema,
                    &schema.root_name,
                    message,
                ))
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
            return Err(Error::one(Diagnostic::new(
                Category::Schema,
                &schema.root_name,
                "enum-typed members are not supported",
            )));
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
                let dim =
                    usize::try_from(*dim).map_err(|_| size_error("array extent exceeds usize"))?;
                count = count
                    .checked_mul(dim)
                    .ok_or_else(|| size_error("array element count overflow"))?;
            }
            let stride = elem.size;
            let size = count
                .checked_mul(stride)
                .ok_or_else(|| size_error("array byte count overflow"))?;
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
                let aligned = aligned_offset(cursor, child.alignment)?;
                cursor = aligned
                    .checked_add(child.size)
                    .ok_or_else(|| size_error("record size overflow"))?;
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
            let size = aligned_offset(cursor, alignment)?;
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
                    });
                }
                collect_padding(schema, layouts, field.type_id, base + field.offset, padding);
                cursor = field.offset + field.size;
            }
            if layout.size > cursor {
                padding.push(PaddingRange {
                    offset: base + cursor,
                    size: layout.size - cursor,
                });
            }
        }
        TypeKind::Array { .. } => {
            let layout = &layouts[id.0];
            let Some(array) = &layout.array else {
                return;
            };
            if array.stride == 0 {
                return;
            }
            let count = layout.size / array.stride;
            for index in 0..count {
                collect_padding(
                    schema,
                    layouts,
                    array.element,
                    base + index * array.stride,
                    padding,
                );
            }
        }
        TypeKind::Scalar { .. } | TypeKind::Enum => {}
    }
}

fn aligned_offset(offset: usize, alignment: usize) -> Result<usize, Error> {
    if alignment == 0 {
        return Err(size_error("alignment is zero"));
    }
    let remainder = offset % alignment;
    if remainder == 0 {
        return Ok(offset);
    }
    offset
        .checked_add(alignment - remainder)
        .ok_or_else(|| size_error("alignment overflow"))
}

fn size_error(message: &str) -> Error {
    Error::one(Diagnostic::new(Category::Schema, "header", message))
}

impl ResolvedLayout {
    pub fn root_layout(&self) -> &TypeLayout {
        &self.layouts[self.root.0]
    }

    pub fn octet_start(&self) -> Result<u32, Error> {
        let start = octet_start_address(self.abi, self.start_address)?;
        u32::try_from(start).map_err(|_| {
            Error::one(Diagnostic::new(
                Category::Encoding,
                &self.root_name,
                "start-address does not fit a 32-bit octet address",
            ))
        })
    }

    pub fn padding_octets(&self) -> usize {
        self.padding_ranges.iter().map(|range| range.size).sum()
    }
}
