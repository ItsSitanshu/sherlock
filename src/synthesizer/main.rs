// -----------------------------------------------------------
// khalti_grinder.rs – Production-grade Khalti DNA Synthesizer
// -----------------------------------------------------------
// Usage: cargo run --release -- khalti_dna.json [scale_factor]

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Duration, FixedOffset, TimeZone, Utc};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Fast xorshift64* RNG – zero dependency, inlined for performance
// ---------------------------------------------------------------------------
struct Rng {
    state: u64,
}

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Rng {
            state: seed.wrapping_mul(0xDEAD_BEEF_CAFE_BABE),
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D) // multiplier for xorshift64*
    }

    #[inline]
    fn f64(&mut self) -> f64 {
        // top 53 bits
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    #[inline]
    fn bool_prob(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Continuous Pareto sample: x = x_min / u^(1/alpha), u ∈ (0,1]
    #[inline]
    fn pareto(&mut self, x_min: f64, alpha: f64) -> f64 {
        let u = self.f64().max(1e-12);
        x_min / u.powf(1.0 / alpha)
    }

    /// Binary search in a cumulative probability array (static helper)
    #[inline]
    fn weighted_index(cum: &[f64], total: f64, u: f64) -> usize {
        let needle = u * total;
        let mut lo = 0;
        let mut hi = cum.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if cum[mid] <= needle {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.saturating_sub(1)
    }
}

// ---------------------------------------------------------------------------
// DNA JSON structures – all fields have reasonable defaults for resilience
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct Dna {
    metadata: Metadata,
    #[serde(default)]
    temporal_heatmap: Vec<Vec<u64>>,
    #[serde(default)]
    categorical_weights: HashMap<String, HashMap<String, f64>>,
    monetary_distribution: MonetaryDist,
    graph_topology: GraphTopology,
    relational_structure: RelationalStructure,
}

#[derive(Deserialize)]
struct Metadata {
    window_days: i64,
    total_transactions: u64,
    extraction_timestamp: Option<String>,
}

#[derive(Deserialize)]
struct MonetaryDist {
    #[serde(default = "default_pareto_alpha")]
    pareto_alpha: f64,
    tail_threshold: Option<f64>,
}

#[derive(Deserialize)]
struct GraphTopology {
    #[serde(default = "default_pareto_alpha")]
    user_degree_pareto_alpha: f64,
    #[serde(default = "default_num_users")]
    num_users: u64,
}

#[derive(Deserialize)]
struct RelationalStructure {
    edge_weight: EdgeWeight,
    unique_devices_per_user: UniqueDevices,
}

#[derive(Deserialize)]
struct EdgeWeight {
    #[serde(default = "default_repeat_freq")]
    repeat_tx_frequency: f64,
}

#[derive(Deserialize)]
struct UniqueDevices {
    #[serde(default = "default_mean_devices")]
    mean: f64,
}

fn default_pareto_alpha() -> f64 { 1.16 }
fn default_num_users() -> u64 { 100_000 }
fn default_repeat_freq() -> f64 { 0.3 }
fn default_mean_devices() -> f64 { 1.2 }

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <khalti_dna.json> [scale_factor]", args[0]);
        std::process::exit(1);
    }
    let json_path = &args[1];
    let scale: f64 = if args.len() > 2 {
        args[2].parse().unwrap_or(1.0)
    } else {
        1.0
    };

    // ── Parse DNA ──
    let file = File::open(json_path).expect("Cannot open JSON");
    let dna: Dna = serde_json::from_reader(BufReader::new(file))?;

    let total_tx = (dna.metadata.total_transactions as f64 * scale) as u64;
    let window_days = dna.metadata.window_days;
    let num_users = dna.graph_topology.num_users.max(1);
    let alpha_degree = dna.graph_topology.user_degree_pareto_alpha.max(1.01);
    let alpha_amount = dna.monetary_distribution.pareto_alpha.max(1.01);
    let repeat_prob = dna.relational_structure.edge_weight.repeat_tx_frequency.clamp(0.0, 1.0);
    let mean_devices = dna.relational_structure.unique_devices_per_user.mean.max(1.0);

    // ── Window start ──
    let window_start = if let Some(ts) = &dna.metadata.extraction_timestamp {
        DateTime::<FixedOffset>::parse_from_rfc3339(ts)
            .map(|d: DateTime<FixedOffset>| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
            - Duration::days(window_days)
    } else {
        Utc::now() - Duration::days(window_days)
    };
    let window_start_secs = window_start.timestamp() as f64;

    // ── Temporal heatmap → probability table ──
    let heatmap = &dna.temporal_heatmap;
    let total_vol: u64 = heatmap.iter().flatten().sum();
    let mut cell_probs: Vec<(usize, usize, f64)> = Vec::with_capacity(168);
    let mut cum = 0.0;
    if total_vol > 0 {
        for (h, row) in heatmap.iter().enumerate() {
            for (d, &v) in row.iter().enumerate() {
                let p = v as f64 / total_vol as f64;
                cum += p;
                cell_probs.push((h, d, cum));
            }
        }
    } else {
        // uniform fallback
        let p = 1.0 / 168.0;
        for h in 0..24 {
            for d in 0..7 {
                cum += p;
                cell_probs.push((h, d, cum));
            }
        }
    }

    // ── Pre‑generate per‑user transaction counts (power‑law degree) ──
    let mut rng = Rng::new();
    let mut user_tx_counts: Vec<u64> = Vec::with_capacity(num_users as usize);
    let mut sum_counts = 0u64;
    for _ in 0..num_users {
        let k = (rng.pareto(1.0, alpha_degree).ceil() as u64).max(1);
        user_tx_counts.push(k);
        sum_counts += k;
    }
    // Scale to match total_tx exactly
    let scale_factor = total_tx as f64 / sum_counts as f64;
    let mut scaled_counts: Vec<u64> = Vec::with_capacity(num_users as usize);
    let mut remaining_sum = total_tx;
    for (i, &cnt) in user_tx_counts.iter().enumerate() {
        let raw = cnt as f64 * scale_factor;
        let floor = raw as u64;
        let frac = raw - floor as f64;
        let extra = if rng.f64() < frac { 1 } else { 0 };
        let val = (floor + extra).min(remaining_sum);
        scaled_counts.push(val);
        if i < (num_users as usize - 1) {
            remaining_sum -= val;
        } else {
            // last user takes the remainder
            scaled_counts[i] = remaining_sum;
            break;
        }
    }

    // Flatten to owner pool (user_id index for each transaction)
    let mut owner_pool: Vec<u64> = Vec::with_capacity(total_tx as usize);
    for (uid, &cnt) in scaled_counts.iter().enumerate() {
        for _ in 0..cnt {
            owner_pool.push(uid as u64);
        }
    }
    // Fisher–Yates shuffle to interleave users randomly
    for i in (1..owner_pool.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        owner_pool.swap(i, j);
    }

    // ── Pre‑generate device IDs per user ──
    let mut user_devices: Vec<Vec<u64>> = Vec::with_capacity(num_users as usize);
    let prob_second_device = (mean_devices - 1.0).clamp(0.0, 1.0);
    for _ in 0..num_users {
        let mut devs = Vec::with_capacity(2);
        devs.push(rng.next_u64() & 0xFFFF_FFFF_FFFF);
        if rng.f64() < prob_second_device {
            devs.push(rng.next_u64() & 0xFFFF_FFFF_FFFF);
        }
        user_devices.push(devs);
    }

    // ── Counterparty selection weights (power‑law degree again) ──
    let cp_cum_weights: Vec<f64> = scaled_counts
        .iter()
        .scan(0.0, |acc, &c| {
            *acc += c as f64;
            Some(*acc)
        })
        .collect();
    let total_cp_weight = *cp_cum_weights.last().unwrap_or(&0.0);

    // Edge memory for “stickiness”
    let mut user_partners: HashMap<u64, Vec<u64>> = HashMap::new();

    // ── Categorical pre‑computation ──
    let status_dist = build_cat_dist(&dna.categorical_weights, "qrapp_fonepaytransaction.status");
    let purpose_dist = build_cat_dist(&dna.categorical_weights, "qrapp_fonepaytransaction.purpose");
    let channel = b"QR"; // constant for now

    // ── Output file & progress ──
    let out_file = File::create("synthetic_transactions.csv")?;
    let mut writer = BufWriter::with_capacity(64 * 1024 * 1024, out_file);
    writeln!(
        writer,
        "user_id,counterparty_id,amount,timestamp,status,purpose,channel,device_id"
    )?;

    let bar = ProgressBar::new(total_tx);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} rows ({per_sec}) ETA: {eta}")
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut row_buf: Vec<u8> = Vec::with_capacity(256); // reuse buffer

    // ── Main generation loop ──
    for (tx_idx, &user_idx) in owner_pool.iter().enumerate() {
        let uid = user_idx;
        let uid_str = format!("U{:010}", uid);

        // ── Counterparty (stickiness power‑law) ──
        let cp_idx: u64 = if repeat_prob > 0.0 && rng.bool_prob(repeat_prob) {
            if let Some(hist) = user_partners.get(&uid) {
                if !hist.is_empty() {
                    hist[(rng.next_u64() as usize) % hist.len()]
                } else {
                    sample_new_counterparty(
                        &mut rng,
                        &cp_cum_weights,
                        total_cp_weight,
                        uid,
                        num_users,
                    )
                }
            } else {
                sample_new_counterparty(
                    &mut rng,
                    &cp_cum_weights,
                    total_cp_weight,
                    uid,
                    num_users,
                )
            }
        } else {
            sample_new_counterparty(
                &mut rng,
                &cp_cum_weights,
                total_cp_weight,
                uid,
                num_users,
            )
        };
        let cp_str = format!("U{:010}", cp_idx);
        // update adjacency
        if repeat_prob > 0.0 {
            user_partners.entry(uid).or_insert_with(Vec::new).push(cp_idx);
        }

        // ── Amount (Pareto with x_min = 100 NPR) ──
        let amount = rng.pareto(100.0, alpha_amount).max(10.0);

        // ── Timestamp from heatmap ──
        let u_time = rng.f64();
        let slot = cell_probs
            .iter()
            .find(|&&(_, _, cum_p)| u_time <= cum_p)
            .unwrap_or(&cell_probs[0]);
        let (hour, dow, _) = slot;
        let days_to_first = ((dow + 7) as i64 - window_start.weekday().num_days_from_sunday() as i64) % 7;
        let base_day = window_start + Duration::days(days_to_first);
        let random_day_offset = if window_days > 1 {
            (rng.next_u64() as i64 % window_days) * 86_400
        } else {
            0
        };
        let ts = base_day.timestamp() as f64
            + random_day_offset as f64
            + (*hour as f64) * 3600.0
            + rng.f64() * 3600.0;

        // ── Categoricals ──
        let status = sample_cat(&mut rng, &status_dist);
        let purpose = sample_cat(&mut rng, &purpose_dist);

        // ── Device ──
        let devs = &user_devices[uid as usize];
        let dev_id = devs[(rng.next_u64() as usize) % devs.len()];
        let dev_str = format!("DEV{:012X}", dev_id);

        // ── Fast write ──
        row_buf.clear();
        row_buf.extend_from_slice(uid_str.as_bytes());
        row_buf.push(b',');

        row_buf.extend_from_slice(cp_str.as_bytes());
        row_buf.push(b',');

        // Use ryu for fast float formatting
        let mut buf = ryu::Buffer::new();
        row_buf.extend_from_slice(buf.format(amount).as_bytes());
        row_buf.push(b',');

        let mut buf = ryu::Buffer::new();
        row_buf.extend_from_slice(buf.format(ts).as_bytes());
        row_buf.push(b',');

        row_buf.extend_from_slice(status.as_bytes());
        row_buf.push(b',');
        row_buf.extend_from_slice(purpose.as_bytes());
        row_buf.push(b',');

        row_buf.extend_from_slice(channel);
        row_buf.push(b',');

        row_buf.extend_from_slice(dev_str.as_bytes());
        row_buf.push(b'\n');

        writer.write_all(&row_buf)?;

        if tx_idx % 10_000 == 0 {
            bar.inc(10_000);
            bar.set_message(format!("Row {}", tx_idx));
        }
    }

    writer.flush()?;
    bar.finish_with_message("Synthesis complete.");
    println!("✔ Generated {} transactions into synthetic_transactions.csv", total_tx);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a cumulative categorical distribution from the JSON map.
fn build_cat_dist(
    all_weights: &HashMap<String, HashMap<String, f64>>,
    key: &str,
) -> Vec<(String, f64)> {
    if let Some(map) = all_weights.get(key) {
        let mut cum = 0.0;
        let mut dist = Vec::new();
        for (k, &v) in map {
            cum += v;
            dist.push((k.clone(), cum));
        }
        // Normalise in case the total is not exactly 1.0
        let last = dist.last().map(|x| x.1).unwrap_or(1.0);
        if (last - 1.0).abs() > 1e-6 {
            for (_, p) in &mut dist {
                *p /= last;
            }
        }
        dist
    } else {
        vec![("SUCCESS".to_string(), 1.0)]
    }
}

/// Sample from a cumulative categorical distribution.
fn sample_cat<'a>(rng: &mut Rng, dist: &'a [(String, f64)]) -> &'a str {
    let u = rng.f64();
    for (cat, cum) in dist {
        if u <= *cum {
            return cat.as_str();
        }
    }
    dist.last().map(|x| x.0.as_str()).unwrap_or("UNKNOWN")
}

/// Choose a new counterparty using power‑law biased weights (excludes self).
fn sample_new_counterparty(
    rng: &mut Rng,
    cum_weights: &[f64],
    total: f64,
    exclude: u64,
    max_id: u64,
) -> u64 {
    loop {
        let u = rng.f64();
        let idx = Rng::weighted_index(cum_weights, total, u);
        if idx as u64 != exclude && (idx as u64) < max_id {
            return idx as u64;
        }
        // extremely rare collision – just resample
    }
}