use crate::layout::settings::ChecksumConfig;

/// Hand-rolled CRC32 calculation matching the crc crate's NoTable implementation.
/// This removes the need for static state and allows each block to use its own CRC settings.
pub fn calculate_crc(data: &[u8], crc_settings: &ChecksumConfig) -> u32 {
    let polynomial = crc_settings.polynomial;
    let start = crc_settings.start;
    let xor_out = crc_settings.xor_out;
    let ref_in = crc_settings.ref_in;
    let ref_out = crc_settings.ref_out;

    // Initialize CRC based on ref_in
    let mut crc = if ref_in { start.reverse_bits() } else { start };

    // Prepare polynomial
    let poly = if ref_in {
        polynomial.reverse_bits()
    } else {
        polynomial
    };

    // Process each byte
    for &byte in data {
        let idx = if ref_in {
            (crc ^ (byte as u32)) & 0xFF
        } else {
            ((crc >> 24) ^ (byte as u32)) & 0xFF
        };

        // Perform 8 rounds of bitwise CRC calculation
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

    // Finalize
    if ref_in ^ ref_out {
        crc = crc.reverse_bits();
    }

    crc ^ xor_out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_crc_config() -> ChecksumConfig {
        ChecksumConfig {
            polynomial: 0x04C11DB7,
            start: 0xFFFF_FFFF,
            xor_out: 0xFFFF_FFFF,
            ref_in: true,
            ref_out: true,
        }
    }

    // Verify our CRC32 implementation against the well-known test vector
    #[test]
    fn test_crc32_standard_test_vector() {
        let crc_settings = standard_crc_config();

        // The standard CRC32 test vector - "123456789" should produce 0xCBF43926
        let test_str = b"123456789";
        let result = calculate_crc(test_str, &crc_settings);
        assert_eq!(
            result, 0xCBF43926,
            "Standard CRC32 test vector failed (expected 0xCBF43926 for \"123456789\")"
        );

        // Test with simple data to ensure the implementation is stable
        let simple_data = vec![0x01, 0x02, 0x03, 0x04];
        let simple_result = calculate_crc(&simple_data, &crc_settings);
        assert_eq!(simple_result, 0xB63CFBCD, "CRC32 for [1,2,3,4] failed");
    }

    #[test]
    fn test_crc32_mpeg2_non_reflected_vector() {
        let crc_settings = ChecksumConfig {
            polynomial: 0x04C11DB7,
            start: 0xFFFF_FFFF,
            xor_out: 0x0000_0000,
            ref_in: false,
            ref_out: false,
        };

        // CRC-32/MPEG-2 parameters (non-reflected) over "123456789" should produce 0x0376E6E7
        let test_str = b"123456789";
        let result = calculate_crc(test_str, &crc_settings);
        assert_eq!(
            result, 0x0376E6E7,
            "CRC32/MPEG-2 test vector failed (expected 0x0376E6E7 for \"123456789\")"
        );
    }
}
