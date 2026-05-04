pub mod types;
pub mod csv;
pub mod progress;
pub mod graph;
pub mod algorithms;

pub use types::*;
pub use csv::{split_csv, read_csv_lines};
pub use progress::ProgressBar;
pub use algorithms::{TarjanScc, find_cycles_within_sccs, print_summary};

use std::io;
use std::time::Instant;

// ─────────────────────────────────────────────
// 10. Top-level benchmark runner
// ─────────────────────────────────────────────

/// Paths to each CSV file. Pass `None` to skip that source.
pub struct DataPaths<'a> {
    pub wallet_history:   Option<&'a str>,
    pub qr_transactions:  Option<&'a str>,
    pub service_payments: Option<&'a str>,
    pub remittance:       Option<&'a str>,
    pub disbursements:    Option<&'a str>,
}

pub fn build_and_analyse(
    paths: DataPaths<'_>,
    cycle_depth: usize,
) -> io::Result<(SherlockGraph, usize, Vec<Vec<usize>>)> {
    let mut g = SherlockGraph::new();
    let wall = Instant::now();

    println!("\n══ SherlockGraph — CSV ingestion ══════════════════════");

    if let Some(p) = paths.wallet_history   { g.load_wallet_history(p)?; }
    if let Some(p) = paths.qr_transactions  { g.load_qr_transactions(p)?; }
    if let Some(p) = paths.service_payments { g.load_service_payments(p)?; }
    if let Some(p) = paths.remittance       { g.load_remittance(p)?; }
    if let Some(p) = paths.disbursements    { g.load_disbursements(p)?; }

    println!("\n══ Tarjan's SCC extraction ════════════════════════════");
    let pb_scc = ProgressBar::new("compute_tarjans_scc", g.nodes.len());
    pb_scc.update(0);
    let t_scc = Instant::now();
    let scc_count = TarjanScc::compute(&mut g);
    let scc_ms = t_scc.elapsed().as_millis();
    pb_scc.finish(scc_ms);

    println!("\n══ Cycle enumeration (depth ≤ {}) ════════════════════", cycle_depth);
    let t_cyc = Instant::now();
    let cycles = find_cycles_within_sccs(&g, cycle_depth);
    println!("  found {} cycles  [{:.2}ms]", cycles.len(), t_cyc.elapsed().as_millis());

    println!("\n══ Total wall time: {:.2}ms", wall.elapsed().as_millis());

    print_summary(&g, scc_count, cycles.len(), scc_ms);

    Ok((g, scc_count, cycles))
}

/// Build and analyse from unified mono-CSV format.
/// Single file containing: user_id,counterparty_id,amount,timestamp,qr_status,qr_purpose,disb_type,remit_type,auth_action,device_id,is_burst
pub fn build_and_analyse_unified(
    csv_path: &str,
    cycle_depth: usize,
) -> io::Result<(SherlockGraph, usize, Vec<Vec<usize>>)> {
    let mut g = SherlockGraph::new();
    let wall = Instant::now();

    println!("\n══ SherlockGraph — Unified CSV ingestion ═══════════════");
    g.load_unified_transactions(csv_path)?;

    println!("\n══ Tarjan's SCC extraction ════════════════════════════");
    let pb_scc = ProgressBar::new("compute_tarjans_scc", g.nodes.len());
    pb_scc.update(0);
    let t_scc = Instant::now();
    let scc_count = TarjanScc::compute(&mut g);
    let scc_ms = t_scc.elapsed().as_millis();
    pb_scc.finish(scc_ms);

    println!("\n══ Cycle enumeration (depth ≤ {}) ════════════════════", cycle_depth);
    let t_cyc = Instant::now();
    let cycles = find_cycles_within_sccs(&g, cycle_depth);
    println!("  found {} cycles  [{:.2}ms]", cycles.len(), t_cyc.elapsed().as_millis());

    println!("\n══ Total wall time: {:.2}ms", wall.elapsed().as_millis());

    print_summary(&g, scc_count, cycles.len(), scc_ms);

    Ok((g, scc_count, cycles))
}
