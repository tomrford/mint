pub mod args;
pub mod checksum;
pub mod error;
pub mod report;

use crate::layout::header::Header;
use crate::layout::settings::MintConfig;
use crate::output::args::OutputFormat;
use error::OutputError;

use bin_file::{BinFile, IHexFormat};

#[derive(Debug, Clone)]
pub struct DataRange {
    pub start_address: u32,
    pub bytestream: Vec<u8>,
    pub used_size: u32,
    pub allocated_size: u32,
}

pub fn bytestream_to_datarange(
    bytestream: Vec<u8>,
    header: &Header,
    settings: &MintConfig,
    padding_bytes: u32,
) -> Result<DataRange, OutputError> {
    if bytestream.len() > header.length as usize {
        return Err(OutputError::HexOutputError(
            "Bytestream length exceeds block length.".to_owned(),
        ));
    }

    let used_size = (bytestream.len() as u32).saturating_sub(padding_bytes);
    let start_address = header
        .start_address
        .checked_add(settings.virtual_offset)
        .ok_or_else(|| {
            OutputError::HexOutputError("Start address overflows address space.".to_owned())
        })?;

    Ok(DataRange {
        start_address,
        bytestream,
        used_size,
        allocated_size: header.length,
    })
}

pub fn emit_hex(
    ranges: &[DataRange],
    record_width: usize,
    format: OutputFormat,
) -> Result<String, OutputError> {
    if !(1..=128).contains(&record_width) {
        return Err(OutputError::HexOutputError(
            "Record width must be between 1 and 128".to_owned(),
        ));
    }

    // Use bin_file to format output.
    let mut bf = BinFile::new();
    let mut max_end: usize = 0;

    for range in ranges {
        bf.add_bytes(
            range.bytestream.as_slice(),
            Some(range.start_address as usize),
            false,
        )
        .map_err(|e| OutputError::HexOutputError(format!("Failed to add bytes: {}", e)))?;

        let end = (range.start_address as usize).saturating_add(range.bytestream.len());
        if end > max_end {
            max_end = end;
        }
    }

    match format {
        OutputFormat::Hex => {
            let ihex_format = if max_end <= 0x1_0000 {
                IHexFormat::IHex16
            } else {
                IHexFormat::IHex32
            };
            let lines = bf.to_ihex(Some(record_width), ihex_format).map_err(|e| {
                OutputError::HexOutputError(format!("Failed to generate Intel HEX: {}", e))
            })?;
            Ok(lines.join("\n"))
        }
        OutputFormat::Mot => {
            use bin_file::SRecordAddressLength;
            let addr_len = if max_end <= 0x1_0000 {
                SRecordAddressLength::Length16
            } else if max_end <= 0x100_0000 {
                SRecordAddressLength::Length24
            } else {
                SRecordAddressLength::Length32
            };
            let lines = bf.to_srec(Some(record_width), addr_len).map_err(|e| {
                OutputError::HexOutputError(format!("Failed to generate S-Record: {}", e))
            })?;
            Ok(lines.join("\n"))
        }
    }
}

/// Represents an output file to be written.
#[derive(Debug, Clone)]
pub struct OutputFile {
    pub ranges: Vec<DataRange>,
    pub format: OutputFormat,
    pub record_width: usize,
}

impl OutputFile {
    /// Render this file's contents as a hex/mot string.
    pub fn render(&self) -> Result<String, OutputError> {
        emit_hex(&self.ranges, self.record_width, self.format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::header::Header;
    use crate::layout::settings::Endianness;
    use crate::layout::settings::MintConfig;
    use std::collections::HashMap;

    fn sample_settings() -> MintConfig {
        MintConfig {
            endianness: Endianness::Little,
            virtual_offset: 0,
            checksum: HashMap::new(),
        }
    }

    fn sample_header(len: u32) -> Header {
        Header {
            start_address: 0,
            length: len,
            padding: 0xFF,
        }
    }

    #[test]
    fn basic_datarange_generation() {
        let settings = sample_settings();
        let header = sample_header(16);

        let bytestream = vec![1u8, 2, 3, 4];
        let dr = bytestream_to_datarange(bytestream.clone(), &header, &settings, 0)
            .expect("data range generation failed");

        assert_eq!(dr.bytestream.len(), 4);
        assert_eq!(dr.start_address, 0);
        assert_eq!(dr.used_size, 4);
        assert_eq!(dr.allocated_size, 16);
    }

    #[test]
    fn bytestream_exceeds_block_length_errors() {
        let settings = sample_settings();
        let header = sample_header(4);

        let bytestream = vec![1u8; 8]; // 8 bytes > 4 byte block
        let result = bytestream_to_datarange(bytestream, &header, &settings, 0);
        assert!(result.is_err());
    }

    #[test]
    fn hex_output_format() {
        let settings = sample_settings();
        let header = sample_header(16);

        let bytestream = vec![1u8, 2, 3, 4];
        let dr = bytestream_to_datarange(bytestream, &header, &settings, 0)
            .expect("data range generation failed");
        let hex = emit_hex(&[dr], 16, OutputFormat::Hex).expect("hex generation failed");

        assert!(!hex.is_empty());
        // Intel HEX starts with ':'
        assert!(hex.starts_with(':'));
    }
}
