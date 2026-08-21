use crate::CompiledSchema;
use crate::diagnostic::{Category, Error};

const RECORD_WIDTH: usize = 32;

pub fn render_i32hex(schema: &CompiledSchema, bytes: &[u8]) -> Result<String, Error> {
    let expected = schema.layout.root_layout().size;
    if bytes.len() != expected {
        return Err(encode(
            schema,
            format!(
                "encoded payload is {} octets, expected {expected}",
                bytes.len()
            ),
        ));
    }
    let start = u64::from(schema.layout.octet_start);

    let mut lines = Vec::new();
    let mut offset = 0usize;
    let mut last_ela = None;
    while offset < bytes.len() {
        let address = start + offset as u64;
        let upper = (address >> 16) as u16;
        if last_ela != Some(upper) {
            lines.push(ela_record(upper));
            last_ela = Some(upper);
        }
        let room = (0x1_0000 - (address & 0xFFFF)) as usize;
        let remaining = bytes.len() - offset;
        let width = remaining.min(RECORD_WIDTH).min(room);
        let record_addr = (address & 0xFFFF) as u16;
        lines.push(data_record(record_addr, &bytes[offset..offset + width]));
        offset += width;
    }
    lines.push(":00000001FF".to_owned());
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out)
}

fn ela_record(upper: u16) -> String {
    let data = upper.to_be_bytes();
    hex_record(0, 0x04, &data)
}

fn data_record(address: u16, data: &[u8]) -> String {
    hex_record(address, 0x00, data)
}

fn hex_record(address: u16, record_type: u8, data: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(4 + data.len() + 1);
    bytes.push(data.len() as u8);
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.push(record_type);
    bytes.extend_from_slice(data);
    let sum: u32 = bytes.iter().map(|byte| u32::from(*byte)).sum();
    let checksum = (!sum + 1) as u8;
    bytes.push(checksum);
    let mut line = String::from(":");
    for byte in bytes {
        line.push_str(&format!("{byte:02X}"));
    }
    line
}

fn encode(schema: &CompiledSchema, message: impl Into<String>) -> Error {
    Error::at(
        Category::Encoding,
        &schema.source,
        schema.layout.start_address_span,
        message,
    )
}
