use anyhow::{Context, Result};
use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use clap::Parser;
use csv::Writer;
use indicatif::{ProgressBar, ProgressStyle};
use rand::prelude::*;
use rand_distr::{Distribution, WeightedIndex};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

const DNA_FILE: &str = "../data/2e97b68.json";

/// ============================================================================
/// 1. OVERVIEW (ODD Protocol + DNA stats)
/// ============================================================================

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Config {
    #[arg(short, long, default_value_t = 1000)]
    num_users: usize,

    #[arg(short, long, default_value_t = 30)]
    num_days: i64,

    #[arg(short, long, default_value_t = 0.001)]
    fraud_rate: f64,

    #[arg(short, long, default_value = "npr_synthetic_data")]
    filename: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DNA {
    temporal_heatmap: HeatmapData,
    categorical_weights: HashMap<String, HashMap<String, Value>>,
    monetary_distribution: MonetaryDistribution,
    graph_topology: GraphTopology,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeatmapData {
    heatmap: Vec<Vec<f64>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MonetaryDistribution {
    pareto_alpha: f64,
    tail_threshold: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphTopology {
    user_degree_pareto_alpha: f64,
}

#[derive(Debug, Serialize)]
struct Transaction {
    txn_id: String,
    txn_time: String,
    amount: f64,
    source_id: String,
    destination_id: String,
    status: String,
    purpose: String,
}

struct Samplers {
    status_values: Vec<String>,
    status_dist: WeightedIndex<f64>,
    purpose_values: Vec<String>,
    purpose_dist: WeightedIndex<f64>,
}

impl Samplers {
    fn new(dna: &DNA) -> Result<Self> {
        // Status Sampler
        let (status_values, status_probs) = Self::parse_categorical(&dna.categorical_weights, "gateway_transaction.is_obsolete")?;
        let status_dist = WeightedIndex::new(&status_probs).context("Status probabilities invalid")?;

        // Purpose Sampler
        let (purpose_values, purpose_probs) = Self::parse_categorical(&dna.categorical_weights, "gateway_transaction.purpose")?;
        let purpose_dist = WeightedIndex::new(&purpose_probs).context("Purpose probabilities invalid")?;

        Ok(Self {
            status_values,
            status_dist,
            purpose_values,
            purpose_dist,
        })
    }

    fn parse_categorical(weights: &HashMap<String, HashMap<String, Value>>, key: &str) -> Result<(Vec<String>, Vec<f64>)> {
        let mut values = Vec::new();
        let mut probs = Vec::new();
        if let Some(map) = weights.get(key) {
            for (val, info) in map {
                if val == "_metadata" { continue; }
                let prob = info.get("prob")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                values.push(val.clone());
                probs.push(prob.max(0.0001));
            }
        }
        if values.is_empty() {
            values.push("unknown".to_string());
            probs.push(1.0);
        }
        Ok((values, probs))
    }
}

/// ============================================================================
/// 2. DESIGN CONCEPTS (ODD)
/// ============================================================================

struct User {
    id: String,
    cluster_id: usize,
    num_txns_total: usize,
}

impl User {
    fn new(id: String, cluster_id: usize, alpha: f64, total_txns: usize, rng: &mut impl Rng) -> Self {
        // Sample user degree from Pareto distribution (how many txns they initiate)
        let u: f64 = rng.gen();
        let degree = (1.0 / (1.0 - u).powf(1.0 / alpha)).round() as usize;
        
        Self {
            id,
            cluster_id,
            num_txns_total: degree.min(total_txns / 10), // Cap at 10% of total volume
        }
    }
}

fn sample_amount(dna: &DNA, rng: &mut impl Rng) -> f64 {
    let u: f64 = rng.gen();
    let amount = dna.monetary_distribution.tail_threshold * (1.0 - u).powf(-1.0 / dna.monetary_distribution.pareto_alpha);
    (amount * 100.0).round() / 100.0
}

fn main() -> Result<()> {
    let config = Config::parse();
    println!("Loading DNA from {}...", DNA_FILE);
    let dna_str = std::fs::read_to_string(DNA_FILE).context("Failed to read DNA file")?;
    let dna: DNA = serde_json::from_str(&dna_str).context("Failed to parse DNA JSON")?;
    
    let samplers = Samplers::new(&dna)?;
    let mut rng = thread_rng();

    // Total transaction volume estimation (scaled by days)
    let total_expected_txns = (config.num_users as f64 * 3.0 * config.num_days as f64) as usize;

    // Setup Users with Pareto Degree Distribution
    let users_per_cluster = 100;
    let num_clusters = (config.num_users as f64 / users_per_cluster as f64).ceil() as usize;
    let users: Vec<User> = (0..config.num_users).map(|i| {
        User::new(
            format!("u_{:06}", i), 
            i / users_per_cluster, 
            dna.graph_topology.user_degree_pareto_alpha,
            total_expected_txns,
            &mut thread_rng()
        )
    }).collect();

    let start_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let total_minutes = config.num_days * 1440;
    
    println!("Simulating {} days in NPR using ODD details...", config.num_days);
    
    let all_txns = Arc::new(Mutex::new(Vec::new()));
    let pb = ProgressBar::new(total_minutes as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
        .progress_chars("#>-"));

    let user_ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
    let heatmap_sum: f64 = dna.temporal_heatmap.heatmap.iter().flatten().sum();

    for tick in 0..total_minutes {
        let current_time = start_time + Duration::minutes(tick);
        let hour = current_time.hour() as usize;
        let dow = (current_time.weekday().number_from_sunday() - 1) as usize; // 0=Sun
        
        let tick_prob = dna.temporal_heatmap.heatmap[hour][dow];
        // Calculate how many transactions should happen across the whole population in this minute
        let txns_this_minute = (tick_prob / heatmap_sum) * total_expected_txns as f64 / (config.num_days as f64 * 1440.0);
        let num_to_gen = txns_this_minute.floor() as usize + (if rng.gen_bool(txns_this_minute.fract()) { 1 } else { 0 });

        if num_to_gen > 0 {
            let tick_results: Vec<Transaction> = (0..num_to_gen).into_par_iter().map(|_| {
                let mut local_rng = thread_rng();
                // Pick a user weighted by their assigned degree (simplified random choice for speed)
                let user_idx = local_rng.gen_range(0..config.num_users);
                let user = &users[user_idx];
                
                let amount = sample_amount(&dna, &mut local_rng);
                
                // Destination: 99.9% in cluster (Strict Community per user request)
                let dest_idx = if num_clusters <= 1 || local_rng.gen_bool(0.999) {
                    let offset = local_rng.gen_range(0..users_per_cluster);
                    (user.cluster_id * users_per_cluster + offset) % config.num_users
                } else {
                    local_rng.gen_range(0..config.num_users)
                };
                
                let dest_id = if user_ids[dest_idx] == user.id {
                    user_ids[(dest_idx + 1) % config.num_users].clone()
                } else {
                    user_ids[dest_idx].clone()
                };

                Transaction {
                    txn_id: String::new(), // Set later
                    txn_time: current_time.to_rfc3339(),
                    amount,
                    source_id: user.id.clone(),
                    destination_id: dest_id,
                    status: samplers.status_values[samplers.status_dist.sample(&mut local_rng)].clone(),
                    purpose: samplers.purpose_values[samplers.purpose_dist.sample(&mut local_rng)].clone(),
                }
            }).collect();

            let mut list = all_txns.lock().unwrap();
            list.extend(tick_results);
        }

        if tick % 60 == 0 { pb.inc(60); }
    }
    pb.finish_with_message("Simulation complete");

    let mut final_list = all_txns.lock().unwrap();
    final_list.sort_by(|a, b| a.txn_time.cmp(&b.txn_time));

    for (i, txn) in final_list.iter_mut().enumerate() {
        txn.txn_id = format!("TXN_{:010}", i);
    }

    let output_dir = "../data";
    std::fs::create_dir_all(output_dir).context("Failed to create data directory")?;
    let output_path = format!("{}/{}.csv", output_dir, config.filename);

    println!("Writing {} transactions to {}...", final_list.len(), output_path);
    let file = File::create(&output_path).context("Failed to create output file")?;
    let mut writer = Writer::from_writer(BufWriter::new(file));
    for txn in final_list.iter() {
        writer.serialize(txn)?;
    }
    writer.flush()?;

    println!("\nTop 10 DNA-Driven NPR Transactions:");
    for txn in final_list.iter().take(10) {
        println!("{} | {} | Rs. {:>10.2} | {} -> {} | {}", 
            txn.txn_id, txn.txn_time, txn.amount, txn.source_id, txn.destination_id, txn.purpose);
    }

    Ok(())
}
