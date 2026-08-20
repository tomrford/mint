use crate::diagnostic::{Category, Diagnostic, Error};
use crate::schema::CompiledSchema;

const RECORD_WIDTH: usize = 32;

pub fn render_i32hex(schema: &CompiledSchema, bytes: &[u8]) -> Result<String, Error> {
    let start = u64::from(schema.layout.octet_start()?);
    let end = start
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| encode("output range overflow"))?;
    if end > u64::from(u32::MAX) + 1 {
        return Err(encode(format!(
            "octet-addressed output range 0x{start:08X}-0x{:08X} exceeds the 32-bit address space",
            end.saturating_sub(1)
        )));
    }

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

fn encode(message: impl Into<String>) -> Error {
    Error::one(Diagnostic::new(Category::Encoding, "hex", message))
}
