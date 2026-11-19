pub mod checksum;
pub mod error;
pub mod report;

use crate::layout::abi::Abi;
use crate::layout::header::Header;
use error::OutputError;

use bin_file::{BinFile, IHexFormat};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OutputFormat {
    Hex,
    Mot,
}

#[derive(Debug, Clone)]
pub struct DataRange {
    /// Start address in target addressable units.
    pub start_address: u32,
    /// Width of one target addressable unit.
    pub address_unit_bits: usize,
    pub bytestream: Vec<u8>,
    /// Emitted payload size in octets.
    pub reserved_size: u32,
    /// Allocated block size in octets.
    pub allocated_size: u32,
}

impl DataRange {
    pub(crate) fn from_block(bytestream: Vec<u8>, header: &Header, abi: Abi) -> Self {
        Self {
            start_address: header.start_address,
            address_unit_bits: abi.address_unit_bits(),
            reserved_size: bytestream.len() as u32,
            bytestream,
            allocated_size: header.length,
        }
    }

    fn address_unit_octets(&self) -> Result<usize, OutputError> {
        if self.address_unit_bits == 0 || !self.address_unit_bits.is_multiple_of(8) {
            return Err(OutputError::AddressRangeError(format!(
                "addressable unit width must be a positive multiple of 8 bits, got {}",
                self.address_unit_bits
            )));
        }
        Ok(self.address_unit_bits / 8)
    }

    /// Start address used in standard octet-addressed output formats.
    pub fn output_start_address(&self) -> Result<u32, OutputError> {
        self.octet_start(self.address_unit_octets()?)
    }

    fn octet_start(&self, unit_octets: usize) -> Result<u32, OutputError> {
        let start = u64::from(self.start_address)
            .checked_mul(unit_octets as u64)
            .ok_or_else(|| {
                OutputError::AddressRangeError(
                    "target start address cannot be represented as an octet address".to_owned(),
                )
            })?;
        u32::try_from(start).map_err(|_| {
            OutputError::AddressRangeError(format!(
                "target start address 0x{:08X} with {}-bit addressable units exceeds the 32-bit output address space",
                self.start_address, self.address_unit_bits
            ))
        })
    }
}

pub fn bytestream_to_datarange(
    bytestream: Vec<u8>,
    header: &Header,
    abi: Abi,
) -> Result<DataRange, OutputError> {
    if bytestream.len() > header.length as usize {
        return Err(OutputError::HexOutputError(
            "Bytestream length exceeds block length.".to_owned(),
        ));
    }

    let unit_octets = abi.address_unit_octets();
    check_unit_length("block length", header.length as usize, unit_octets)?;
    check_unit_length("bytestream length", bytestream.len(), unit_octets)?;

    let range = DataRange::from_block(bytestream, header, abi);
    range.octet_start(unit_octets)?;
    Ok(range)
}

fn check_unit_length(what: &str, len: usize, unit_octets: usize) -> Result<(), OutputError> {
    if len.is_multiple_of(unit_octets) {
        Ok(())
    } else {
        Err(OutputError::AddressRangeError(format!(
            "{what} {len} octets is not divisible by the {unit_octets}-octet addressable unit"
        )))
    }
}

pub(crate) fn render(
    ranges: &[DataRange],
    record_width: usize,
    format: OutputFormat,
) -> Result<String, OutputError> {
    if !(1..=128).contains(&record_width) {
        return Err(OutputError::HexOutputError(
            "Record width must be between 1 and 128".to_owned(),
        ));
    }

    let unit_octets = if let Some(first) = ranges.first() {
        if ranges
            .iter()
            .any(|range| range.address_unit_bits != first.address_unit_bits)
        {
            return Err(OutputError::HexOutputError(
                "one output file cannot mix target addressable-unit widths".to_owned(),
            ));
        }
        first.address_unit_octets()?
    } else {
        1
    };
    if !record_width.is_multiple_of(unit_octets) {
        return Err(OutputError::HexOutputError(format!(
            "record width {record_width} octets is not divisible by the target's {unit_octets}-octet addressable unit"
        )));
    }

    let mut bf = BinFile::new();
    let mut max_end = 0u64;

    for range in ranges {
        let len = range.bytestream.len();
        check_unit_length("bytestream length", len, unit_octets)?;
        let output_start = range.octet_start(unit_octets)?;
        let end = u64::from(output_start) + len as u64;
        if end > u64::from(u32::MAX) + 1 {
            return Err(OutputError::AddressRangeError(format!(
                "octet-addressed output range 0x{output_start:08X}-0x{:08X} exceeds the 32-bit address space",
                end.saturating_sub(1)
            )));
        }
        bf.add_bytes(
            range.bytestream.as_slice(),
            Some(output_start as usize),
            false,
        )
        .map_err(|e| OutputError::HexOutputError(format!("Failed to add bytes: {}", e)))?;

        max_end = max_end.max(end);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::header::Header;

    fn sample_header(len: u32) -> Header {
        Header {
            start_address: 0,
            length: len,
            padding: 0xFF,
        }
    }

    #[test]
    fn bytestream_exceeds_block_length_errors() {
        let header = sample_header(4);

        let bytestream = vec![1u8; 8];
        let result = bytestream_to_datarange(bytestream, &header, Abi::GenericLe);
        assert!(result.is_err());
    }

    #[test]
    fn c28x_rejects_odd_octet_lengths() {
        let header = Header {
            start_address: 0,
            length: 3,
            padding: 0xFF,
        };
        let error = bytestream_to_datarange(vec![0, 0], &header, Abi::TiC28xEabi)
            .expect_err("odd C28x block length should fail");
        assert!(error.to_string().contains("not divisible"));
    }
}
