use crate::layout::settings::ChecksumConfig;

/// Computes CRC-32 with configurable polynomial, initial value, reflection, and XOR-out.
pub fn calculate_crc(data: &[u8], crc_settings: &ChecksumConfig) -> u32 {
    let polynomial = crc_settings.polynomial;
    let start = crc_settings.start;
    let xor_out = crc_settings.xor_out;
    let ref_in = crc_settings.ref_in;
    let ref_out = crc_settings.ref_out;

    let mut crc = if ref_in { start.reverse_bits() } else { start };

    let poly = if ref_in {
        polynomial.reverse_bits()
    } else {
        polynomial
    };

    for &byte in data {
        let idx = if ref_in {
            (crc ^ (byte as u32)) & 0xFF
        } else {
            ((crc >> 24) ^ (byte as u32)) & 0xFF
        };

        let mut step = if ref_in { idx } else { idx << 24 };
        if ref_in {
            for _ in 0..8 {
                step = (step >> 1) ^ ((step & 1) * poly);
            }
        } else {
            for _ in 0..8 {
                step = (step << 1) ^ (((step >> 31) & 1) * poly);
            }
        }

        crc = if ref_in {
            step ^ (crc >> 8)
        } else {
            step ^ (crc << 8)
        };
    }

    if ref_in ^ ref_out {
        crc = crc.reverse_bits();
    }

    crc ^ xor_out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_mpeg2_non_reflected_vector() {
        let crc_settings = ChecksumConfig {
            polynomial: 0x04C11DB7,
            start: 0xFFFF_FFFF,
            xor_out: 0x0000_0000,
            ref_in: false,
            ref_out: false,
        };

        let test_str = b"123456789";
        let result = calculate_crc(test_str, &crc_settings);
        assert_eq!(
            result, 0x0376E6E7,
            "CRC32/MPEG-2 test vector failed (expected 0x0376E6E7 for \"123456789\")"
        );
    }
}
