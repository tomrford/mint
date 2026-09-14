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

    let mut out = String::new();
    let mut offset = 0usize;
    let mut last_ela = None;
    while offset < bytes.len() {
        let address = start + offset as u64;
        let upper = (address >> 16) as u16;
        if last_ela != Some(upper) {
            hex_record(&mut out, 0, 0x04, &upper.to_be_bytes());
            last_ela = Some(upper);
        }
        let room = (0x1_0000 - (address & 0xFFFF)) as usize;
        let remaining = bytes.len() - offset;
        let width = remaining.min(RECORD_WIDTH).min(room);
        let record_addr = (address & 0xFFFF) as u16;
        hex_record(&mut out, record_addr, 0x00, &bytes[offset..offset + width]);
        offset += width;
    }
    out.push_str(":00000001FF\n");
    Ok(out)
}

fn hex_record(out: &mut String, address: u16, record_type: u8, data: &[u8]) {
    let [high, low] = address.to_be_bytes();
    let header = [data.len() as u8, high, low, record_type];
    let mut sum = 0u8;
    out.push(':');
    for &byte in header.iter().chain(data) {
        hex_byte(out, byte);
        sum = sum.wrapping_add(byte);
    }
    hex_byte(out, sum.wrapping_neg());
    out.push('\n');
}

fn hex_byte(out: &mut String, byte: u8) {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    out.push(DIGITS[(byte >> 4) as usize] as char);
    out.push(DIGITS[(byte & 15) as usize] as char);
}

fn encode(schema: &CompiledSchema, message: impl Into<String>) -> Error {
    Error::at(
        Category::Encoding,
        &schema.source,
        schema.layout.start_address_span,
        message,
    )
}
