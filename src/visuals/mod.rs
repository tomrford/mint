mod formatters;

use crate::commands::stats::BuildStats;
use comfy_table::{Attribute, Cell, ContentArrangement, Table};
use formatters::{format_address_range, format_bytes, format_duration, format_efficiency};

pub fn print_summary(stats: &BuildStats) {
    println!(
        "✓ Built {} blocks in {} ({:.1}% efficiency)",
        stats.blocks_processed,
        format_duration(stats.total_duration),
        stats.space_efficiency()
    );
}

pub fn print_detailed(stats: &BuildStats) {
    let mut summary_table = Table::new();
    summary_table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Build Summary")
                .add_attribute(Attribute::Bold)
                .set_alignment(comfy_table::CellAlignment::Left),
            Cell::new(""),
        ]);

    summary_table.add_row(vec!["Build Time", &format_duration(stats.total_duration)]);
    summary_table.add_row(vec![
        "Blocks Processed",
        &format!("{}", stats.blocks_processed),
    ]);
    summary_table.add_row(vec![
        "Total Allocated",
        &format_bytes(stats.total_allocated),
    ]);
    summary_table.add_row(vec!["Total Used", &format_bytes(stats.total_used)]);
    summary_table.add_row(vec![
        "Space Efficiency",
        &format!("{:.1}%", stats.space_efficiency()),
    ]);

    println!("{summary_table}\n");

    let mut detail_table = Table::new();
    detail_table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Block").add_attribute(Attribute::Bold),
            Cell::new("Address Range").add_attribute(Attribute::Bold),
            Cell::new("Used/Alloc").add_attribute(Attribute::Bold),
            Cell::new("Efficiency").add_attribute(Attribute::Bold),
            Cell::new("Checksum Value").add_attribute(Attribute::Bold),
        ]);

    for block in &stats.block_stats {
        detail_table.add_row(vec![
            Cell::new(&block.name),
            Cell::new(format_address_range(
                block.start_address,
                block.allocated_size,
            )),
            Cell::new(format!(
                "{}/{}",
                format_bytes(block.used_size as usize),
                format_bytes(block.allocated_size as usize)
            )),
            Cell::new(format_efficiency(block.used_size, block.allocated_size)),
            Cell::new(format_checksum_values(&block.checksum_values)),
        ]);
    }

    println!("{detail_table}");
}

fn format_checksum_values(values: &[u32]) -> String {
    if values.is_empty() {
        return "N/A".to_string();
    }

    values
        .iter()
        .map(|value| format!("0x{value:08X}"))
        .collect::<Vec<_>>()
        .join(", ")
}
