use std::time::Duration;

pub fn format_bytes(bytes: usize) -> String {
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

pub fn format_address_range(start: u32, allocated: u32) -> String {
    let end = start + allocated - 1;
    format!("0x{:X}-0x{:X}", start, end)
}

pub fn format_efficiency(used: u32, allocated: u32) -> String {
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
