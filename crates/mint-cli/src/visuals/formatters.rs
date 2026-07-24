use std::time::Duration;

pub fn format_bytes(bytes: u64) -> String {
    let s = bytes.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect::<String>() + " bytes"
}

pub fn format_address_range(start: u32, allocated_address_units: u64) -> String {
    if allocated_address_units == 0 {
        return format!("0x{start:X}");
    }

    let end = u64::from(start) + allocated_address_units - 1;
    format!("0x{start:X}-0x{end:X}")
}

pub fn format_space_reserved(used: u32, allocated: u32) -> String {
    if allocated == 0 {
        "0.0%".to_owned()
    } else {
        format!("{:.1}%", (used as f64 / allocated as f64) * 100.0)
    }
}

pub fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        return format!("{:.3}s", duration.as_secs_f64());
    }

    let millis = duration.as_secs_f64() * 1_000.0;
    if millis >= 1.0 {
        format!("{:.3}ms", millis)
    } else {
        format!("{}us", duration.as_micros())
    }
}

#[cfg(test)]
mod tests {
    use super::format_address_range;

    #[test]
    fn address_ranges_use_target_address_units() {
        assert_eq!(format_address_range(0x1000, 0x100), "0x1000-0x10FF");
        assert_eq!(format_address_range(0x1000, 0x80), "0x1000-0x107F");
    }
}
