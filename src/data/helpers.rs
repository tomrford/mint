use std::collections::HashMap;

/// Warn about duplicate names and their 1-based row indices (including header offset of 1).
///
/// - `names` should be the list of names as read from the main sheet (excluding the header row).
pub fn warn_duplicate_names(names: &[String]) {
    let mut index_map: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, name) in names.iter().enumerate() {
        let key = name.trim();
        if key.is_empty() {
            continue;
        }
        // +2 to convert 0-based data row index to 1-based Excel row index with header offset
        index_map.entry(key.to_owned()).or_default().push(idx + 2);
    }

    let mut duplicates: Vec<(String, Vec<usize>)> = index_map
        .into_iter()
        .filter_map(|(k, v)| if v.len() > 1 { Some((k, v)) } else { None })
        .collect();

    duplicates.sort_by(|a, b| a.0.cmp(&b.0));

    if !duplicates.is_empty() {
        eprintln!("[WARN] Duplicate names detected (column 'Name'):");
        for (dup_name, rows) in duplicates {
            let rows_str = rows
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("  - '{}' at rows {}", dup_name, rows_str);
        }
    }
}
