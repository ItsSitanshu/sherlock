use std::io::{self, Write};

pub struct ProgressBar {
    label: String,
    total: usize,
    width: usize,
}

impl ProgressBar {
    pub fn new(label: &str, total: usize) -> Self {
        Self { label: label.to_string(), total, width: 40 }
    }

    pub fn update(&self, done: usize) {
        let pct = if self.total == 0 { 1.0 } else { done as f64 / self.total as f64 };
        let filled = (pct * self.width as f64).round() as usize;
        let bar: String = format!(
            "[{}{}] {:5.1}%  {}/{}",
            "█".repeat(filled),
            "░".repeat(self.width - filled),
            pct * 100.0,
            done,
            self.total,
        );
        print!("\r  {:<30} {}", self.label, bar);
        let _ = io::stdout().flush();
    }

    pub fn finish(&self, elapsed_ms: u128) {
        self.update(self.total);
        println!("  [{:.2}ms]", elapsed_ms);
    }
}
