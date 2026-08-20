use serde::Serialize;

use crate::diagnostic::Error;
use crate::fingerprint;
use crate::schema::CompiledSchema;
use crate::types::{TypeId, TypeKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectFormat {
    Text,
    Json,
}

#[derive(Serialize)]
struct InspectReport {
    abi: String,
    start_address: u32,
    octet_start_address: u32,
    root_size_octets: usize,
    root_size_units: u64,
    alignment: usize,
    fingerprint: String,
    padding_octets: usize,
    fields: Vec<InspectField>,
    arrays: Vec<InspectArray>,
    padding_ranges: Vec<InspectPadding>,
}

#[derive(Serialize)]
struct InspectField {
    path: String,
    r#type: String,
    offset: usize,
    size: usize,
    alignment: usize,
}

#[derive(Serialize)]
struct InspectArray {
    path: String,
    dimensions: Vec<u64>,
    stride: usize,
}

#[derive(Serialize)]
struct InspectPadding {
    offset: usize,
    size: usize,
}

pub fn render(schema: &CompiledSchema, format: InspectFormat) -> Result<String, Error> {
    let report = report(schema)?;
    match format {
        InspectFormat::Json => serde_json::to_string_pretty(&report).map_err(|error| {
            crate::diagnostic::Error::one(crate::diagnostic::Diagnostic::new(
                crate::diagnostic::Category::Encoding,
                "inspect",
                format!("failed to render inspect JSON: {error}"),
            ))
        }),
        InspectFormat::Text => Ok(render_text(&report)),
    }
}

fn report(schema: &CompiledSchema) -> Result<InspectReport, Error> {
    let root = schema.layout.root_layout();
    let mut fields = Vec::new();
    let mut arrays = Vec::new();
    collect(
        &schema.layout,
        schema.layout.root,
        0,
        "",
        &mut fields,
        &mut arrays,
    );
    Ok(InspectReport {
        abi: schema.layout.abi.name().to_owned(),
        start_address: schema.layout.start_address,
        octet_start_address: schema.layout.octet_start()?,
        root_size_octets: root.size,
        root_size_units: schema
            .layout
            .abi
            .offset_to_address_units(root.size)
            .unwrap_or(0),
        alignment: root.alignment,
        fingerprint: fingerprint::hex(schema.fingerprint),
        padding_octets: schema.layout.padding_octets(),
        fields,
        arrays,
        padding_ranges: schema
            .layout
            .padding_ranges
            .iter()
            .map(|range| InspectPadding {
                offset: range.offset,
                size: range.size,
            })
            .collect(),
    })
}

fn collect(
    layout: &crate::layout::ResolvedLayout,
    type_id: TypeId,
    base: usize,
    path: &str,
    fields: &mut Vec<InspectField>,
    arrays: &mut Vec<InspectArray>,
) {
    match &layout.types[type_id.0] {
        TypeKind::Record { .. } => {
            for field in &layout.layouts[type_id.0].fields {
                let child_path = if path.is_empty() {
                    field.name.clone()
                } else {
                    format!("{path}.{}", field.name)
                };
                fields.push(InspectField {
                    path: child_path.clone(),
                    r#type: field.spelling.clone(),
                    offset: base + field.offset,
                    size: field.size,
                    alignment: field.alignment,
                });
                collect(
                    layout,
                    field.type_id,
                    base + field.offset,
                    &child_path,
                    fields,
                    arrays,
                );
            }
        }
        TypeKind::Array { .. } => {
            if let Some(array) = &layout.layouts[type_id.0].array {
                arrays.push(InspectArray {
                    path: path.to_owned(),
                    dimensions: array.dimensions.clone(),
                    stride: array.stride,
                });
                let child_path = format!("{path}[]");
                collect(layout, array.element, base, &child_path, fields, arrays);
            }
        }
        TypeKind::Scalar { .. } | TypeKind::Enum => {}
    }
}

fn render_text(report: &InspectReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("abi: {}\n", report.abi));
    out.push_str(&format!(
        "start-address: 0x{:08X} (octet 0x{:08X})\n",
        report.start_address, report.octet_start_address
    ));
    out.push_str(&format!(
        "root: {} octets / {} units, align {}\n",
        report.root_size_octets, report.root_size_units, report.alignment
    ));
    out.push_str(&format!("fingerprint: {}\n", report.fingerprint));
    out.push_str(&format!("padding octets: {}\n\n", report.padding_octets));
    out.push_str("path                       type                 offset  size  align\n");
    for field in &report.fields {
        out.push_str(&format!(
            "{:<26} {:<20} {:>6} {:>5} {:>6}\n",
            field.path, field.r#type, field.offset, field.size, field.alignment
        ));
    }
    out.push('\n');
    out.push_str("arrays:\n");
    if report.arrays.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for array in &report.arrays {
            out.push_str(&format!(
                "  {}  dims {:?}  stride {}\n",
                array.path, array.dimensions, array.stride
            ));
        }
    }
    out.push('\n');
    out.push_str("padding:\n");
    if report.padding_ranges.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for range in &report.padding_ranges {
            out.push_str(&format!(
                "  [{}, {})  {} octets\n",
                range.offset,
                range.offset + range.size,
                range.size
            ));
        }
    }
    out
}
