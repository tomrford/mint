use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BlockStat {
    pub name: String,
    pub start_address: u32,
    pub allocated_size: u32,
    pub used_size: u32,
}

#[derive(Debug)]
pub struct BuildStats {
    pub blocks_processed: usize,
    pub total_allocated: usize,
    pub total_used: usize,
    pub total_duration: Duration,
    pub block_stats: Vec<BlockStat>,
}

impl Default for BuildStats {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildStats {
    pub fn new() -> Self {
        Self {
            blocks_processed: 0,
            total_allocated: 0,
            total_used: 0,
            total_duration: Duration::from_secs(0),
            block_stats: Vec::new(),
        }
    }

    pub fn add_block(&mut self, stat: BlockStat) {
        self.blocks_processed += 1;
        self.total_allocated += stat.allocated_size as usize;
        self.total_used += stat.used_size as usize;
        self.block_stats.push(stat);
    }

    pub fn space_efficiency(&self) -> f64 {
        if self.total_allocated == 0 {
            0.0
        } else {
            (self.total_used as f64 / self.total_allocated as f64) * 100.0
        }
    }
}
