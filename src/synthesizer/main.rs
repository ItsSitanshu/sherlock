use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Dynamic Configuration Models ────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct TxMetrics {
    weight: f64,
    mu: f64,
    sigma: f64,
    fee_rate: f64,
}

#[derive(Deserialize, Debug)]
struct Config {
    output_dir: String,
    n_users: usize,
    n_merchants: usize,
    n_transactions: usize,
    anomaly_rate: f64,
    time_peak_hour: f64,
    time_hour_std: f64,
    anomaly_base: f64,
    anomaly_alpha: f64,
    transactions: HashMap<String, TxMetrics>,
}

// ─── Enums ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TxType {
    QrPayment,
    Utilities,
    WalletTopup,
    BankTransfer,
    Remittance,
    MerchantIn,
    MerchantOut,
}

impl TxType {
    fn as_str(self) -> &'static str {
        match self {
            TxType::QrPayment => "QR_PAYMENT",
            TxType::Utilities => "UTILITIES",
            TxType::WalletTopup => "WALLET_TOPUP",
            TxType::BankTransfer => "BANK_TRANSFER",
            TxType::Remittance => "REMITTANCE",
            TxType::MerchantIn => "MERCHANT_IN",
            TxType::MerchantOut => "MERCHANT_OUT",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "QrPayment" => Some(TxType::QrPayment),
            "Utilities" => Some(TxType::Utilities),
            "WalletTopup" => Some(TxType::WalletTopup),
            "BankTransfer" => Some(TxType::BankTransfer),
            "Remittance" => Some(TxType::Remittance),
            "MerchantIn" => Some(TxType::MerchantIn),
            "MerchantOut" => Some(TxType::MerchantOut),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EntityRole { Merchant, User }

impl EntityRole {
    fn as_str(self) -> &'static str {
        match self { EntityRole::Merchant => "MERCHANT", EntityRole::User => "USER" }
    }
}

// ─── Minimal PRNG (xorshift64) ───────────────────────────────────────────────

struct Rng { state: u64 }

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Rng { state: seed ^ 0xcafe_babe_dead_beef }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        self.state = x; x
    }
    fn f64(&mut self) -> f64 { (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 }
    fn usize_range(&mut self, lo: usize, hi: usize) -> usize { lo + (self.next_u64() as usize % (hi - lo)) }
    fn bool_prob(&mut self, p: f64) -> bool { self.f64() < p }
    fn u32_range(&mut self, lo: u32, hi: u32) -> u32 { lo + (self.next_u64() as u32 % (hi - lo)) }

    fn normal(&mut self, mean: f64, std: f64) -> f64 {
        let u1 = self.f64().max(1e-12);
        let u2 = self.f64();
        let z  = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        mean + std * z
    }

    fn pareto_amount(&mut self, x_min: f64, alpha: f64) -> f64 {
        let u = self.f64().max(1e-12);
        x_min / u.powf(1.0 / alpha)
    }

    fn lognormal(&mut self, mu: f64, sigma: f64) -> f64 {
        let n = self.normal(0.0, 1.0);
        (mu + sigma * n).exp()
    }

    fn choice<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        &slice[self.usize_range(0, slice.len())]
    }
}

// ─── Probability Distribution Helper ─────────────────────────────────────────

struct TxSampler {
    cdf: Vec<(TxType, f64)>,
}

impl TxSampler {
    fn new(config: &Config) -> Self {
        let mut cdf = Vec::new();
        let mut cumulative = 0.0;
        
        for (name, metrics) in &config.transactions {
            if let Some(tx_type) = TxType::from_str(name) {
                cumulative += metrics.weight;
                cdf.push((tx_type, cumulative));
            }
        }
        
        // Normalize in case weights don't exactly sum to 1.0
        for (_, val) in &mut cdf {
            *val /= cumulative;
        }
        
        TxSampler { cdf }
    }

    fn pick(&self, rng: &mut Rng) -> TxType {
        let r = rng.f64();
        for &(tx_type, cumulative_prob) in &self.cdf {
            if r <= cumulative_prob {
                return tx_type;
            }
        }
        self.cdf.last().unwrap().0
    }
}

// ─── Timestamp helpers ───────────────────────────────────────────────────────

const BASE_EPOCH: u64 = 1_640_995_200;
const ONE_YEAR_S: u64 = 365 * 24 * 3600;

fn sample_timestamp(rng: &mut Rng, base: u64, peak_hour: f64, hour_std: f64) -> u64 {
    let day_offset = rng.next_u64() % (2 * ONE_YEAR_S / 86400) * 86400;
    let hour: u64 = {
        let h = (rng.normal(peak_hour, hour_std) as i64).clamp(0, 23) as u64;
        h
    };
    let minute = rng.next_u64() % 60;
    let second = rng.next_u64() % 60;
    base + day_offset + hour * 3600 + minute * 60 + second
}

fn fmt_ts(epoch_s: u64) -> String {
    let s = epoch_s % 60;
    let m = (epoch_s / 60) % 60;
    let h = (epoch_s / 3600) % 24;
    let total_days = epoch_s / 86400;
    let (year, month, day) = days_to_ymd(total_days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00", year, month, day, h, m, s)
}

fn fmt_date(epoch_s: u64) -> String {
    let total_days = epoch_s / 86400;
    let (y, mo, d) = days_to_ymd(total_days);
    format!("{:04}-{:02}-{:02}", y, mo, d)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365*yoe + yoe/4 - yoe/100);
    let mp = (5*doy + 2) / 153;
    let d = doy - (153*mp+2)/5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

fn bs_date(epoch_s: u64) -> String {
    let total_days = epoch_s / 86400 + 20678;
    let (y, mo, d) = days_to_ymd(total_days);
    format!("{:04}-{:02}-{:02}", y, mo, d)
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

// ─── Fake data generators (Static strings kept hardcoded for simplicity) ────

static FIRST_NAMES: &[&str] = &["Ram","Sita","Hari","Maya","Bikash","Sunita","Rajesh","Anita","Deepak","Priya","Suresh","Gita","Nabin","Rekha","Sandesh","Puja","Aakash","Manisha","Rohit","Sabina","Binod","Kamala","Dinesh","Laxmi","Prabin","Shruti","Santosh","Nisha","Bibek","Pooja"];
static LAST_NAMES: &[&str] = &["Sharma","Thapa","Karki","Shrestha","Adhikari","Bhattarai","Poudel","Tamang","Gurung","Rai","Limbu","Magar","Neupane","Khadka","Subedi","Pandey","Bista","Yadav"];
static CITIES: &[&str] = &["Kathmandu","Lalitpur","Bhaktapur","Pokhara","Biratnagar","Birgunj","Dharan","Butwal","Hetauda","Itahari"];
static BANKS: &[&str] = &["NIC Asia Bank","Nabil Bank","Nepal Investment Bank","Standard Chartered Bank Nepal","Himalayan Bank","Everest Bank","Kumari Bank","Laxmi Bank","Global IME Bank","Sanima Bank"];
static COUNTRIES: &[&str] = &["Qatar","UAE","Saudi Arabia","Malaysia","South Korea","Japan","Australia","USA","UK","India"];
static ID_TYPES: &[&str] = &["CITIZENSHIP","PASSPORT","DRIVING_LICENSE","VOTER_ID","NATIONAL_ID"];
static PURPOSES: &[&str] = &["FAMILY_SUPPORT","EDUCATION","BUSINESS","MEDICAL","LOAN_REPAYMENT","INVESTMENT","OTHER"];
static GENDERS: &[i32] = &[1, 2];

fn fake_name(rng: &mut Rng) -> String { format!("{} {}", rng.choice(FIRST_NAMES), rng.choice(LAST_NAMES)) }
fn fake_mobile(rng: &mut Rng) -> String { format!("98{:08}", rng.u32_range(0, 100_000_000)) }
fn fake_email(rng: &mut Rng, name: &str) -> String {
    let domain = ["gmail.com","yahoo.com","hotmail.com","outlook.com"][rng.usize_range(0,4)];
    let slug: String = name.to_lowercase().replace(' ', ".");
    format!("{}{}@{}", slug, rng.u32_range(0, 999), domain)
}
fn fake_idx(rng: &mut Rng) -> String { format!("{:016X}", rng.next_u64()) }
fn fake_serial(rng: &mut Rng, prefix: &str) -> String { format!("{}{:010}", prefix, rng.next_u64() % 10_000_000_000) }
fn fake_referral(rng: &mut Rng) -> String {
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    (0..8).map(|_| chars[rng.usize_range(0, chars.len())]).collect()
}
fn fake_ip(rng: &mut Rng) -> String { format!("{}.{}.{}.{}", rng.u32_range(1,255), rng.u32_range(0,255), rng.u32_range(0,255), rng.u32_range(1,255)) }
fn fake_device_id(rng: &mut Rng) -> String { format!("DEV{:012X}", rng.next_u64() & 0xFFFF_FFFF_FFFF) }
fn fake_pan(rng: &mut Rng) -> String { format!("4{:015}", rng.next_u64() % 1_000_000_000_000_000u64) }
fn fake_msg_id(rng: &mut Rng) -> String { format!("MSG{:014}", rng.next_u64() % 100_000_000_000_000u64) }
fn fake_trace(rng: &mut Rng) -> String { format!("{:06}", rng.u32_range(100_000, 999_999)) }
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

// ─── Graph state ─────────────────────────────────────────────────────────────

struct Graph {
    edge_counts: HashMap<(usize, usize), u32>,
    devices: HashMap<usize, Vec<String>>,
}

impl Graph {
    fn new() -> Self { Graph { edge_counts: HashMap::new(), devices: HashMap::new() } }
    fn record_tx(&mut self, uid: usize, mid: usize) { *self.edge_counts.entry((uid, mid)).or_insert(0) += 1; }
    fn register_device(&mut self, uid: usize, dev: String) { self.devices.entry(uid).or_default().push(dev); }
    fn repeat_merchant(&self, rng: &mut Rng, uid: usize, n_merchants: usize) -> usize {
        let known: Vec<usize> = self.edge_counts.keys().filter(|(u, _)| *u == uid).map(|(_, m)| *m).collect();
        if !known.is_empty() && rng.bool_prob(0.55) {
            known[rng.usize_range(0, known.len())]
        } else {
            rng.usize_range(1, n_merchants + 1)
        }
    }
    fn get_device(&mut self, rng: &mut Rng, uid: usize) -> String {
        let devs = self.devices.entry(uid).or_default();
        if devs.is_empty() || rng.bool_prob(0.05) {
            let d = fake_device_id(rng);
            devs.push(d.clone());
            d
        } else {
            let idx = rng.usize_range(0, devs.len());
            devs[idx].clone()
        }
    }
}

// ─── Writers ─────────────────────────────────────────────────────────────────

fn open_writer(dir: &str, name: &str) -> BufWriter<File> {
    let path = format!("{}/{}", dir, name);
    BufWriter::new(File::create(path).expect("cannot create file"))
}

fn writeln_csv(w: &mut BufWriter<File>, fields: &[&str]) {
    let line = fields.iter().map(|f| escape_csv(f)).collect::<Vec<_>>().join(",");
    writeln!(w, "{}", line).unwrap();
}

// ─── Main Synthesizer ────────────────────────────────────────────────────────

fn main() -> std::io::Result<()> {
    // 1. Read Dynamic Config
    let config_file = File::open("metrics_config.json").expect("Make sure metrics_config.json exists.");
    let reader = BufReader::new(config_file);
    let config: Config = serde_json::from_reader(reader).expect("JSON format must match the Config struct");

    fs::create_dir_all(&config.output_dir)?;
    
    let mut rng = Rng::new();
    let mut graph = Graph::new();
    let tx_sampler = TxSampler::new(&config);

    println!("╔════════════════════════════════════════════╗");
    println!("║   Khalti Synthetic Transaction Ledger      ║");
    println!("╠════════════════════════════════════════════╣");
    println!("║  Users       : {:>6}                      ║", config.n_users);
    println!("║  Merchants   : {:>6}                      ║", config.n_merchants);
    println!("║  Transactions: {:>6}                      ║", config.n_transactions);
    println!("║  Anomaly rate: {:>5.1}%                     ║", config.anomaly_rate * 100.0);
    println!("╚════════════════════════════════════════════╝");

    // Helper to get metrics for a TxType dynamically
    let get_metrics = |tx_type: TxType| -> &TxMetrics {
        // Fallback to defaults if a key is missing from JSON
        config.transactions.iter()
            .find(|(k, _)| TxType::from_str(k) == Some(tx_type))
            .map(|(_, m)| m)
            .expect("TxType missing from config")
    };

    // ── 1. autho_user ────────────────────────────────────────────────────────
    {
        let mut w = open_writer(&config.output_dir, "autho_user.csv");
        writeln_csv(&mut w, &[
            "id","password","last_login","is_superuser","idx","created_on","modified_on",
            "is_obsolete","mobile","name","email","is_staff","is_active","is_deleted",
            "is_verified","owner_id","has_kyc","creator_id","qrcode_id","referral_code",
            "is_email_verified","dob","gender_id","serial_number","profile_status",
            "verified_by_id","verified_on","created_on_np_date","primary_role",
            "district_id","dob_type","username","entity_role",
        ]);

        for id in 1..=(config.n_users + config.n_merchants) {
            let role = if id <= config.n_users { EntityRole::User } else { EntityRole::Merchant };
            let name = fake_name(&mut rng);
            let mobile = fake_mobile(&mut rng);
            let email = fake_email(&mut rng, &name);
            let created_ts = sample_timestamp(&mut rng, BASE_EPOCH, config.time_peak_hour, config.time_hour_std);
            let modified_ts = created_ts + rng.next_u64() % 86400;
            let last_login  = modified_ts + rng.next_u64() % (30 * 86400);
            let verified_ts = created_ts + rng.next_u64() % (7 * 86400);
            let dob_year = 1970 + rng.u32_range(0, 38);
            let dob_month = rng.u32_range(1, 13);
            let dob_day   = rng.u32_range(1, 29);
            let is_verified = rng.bool_prob(0.85);
            let has_kyc     = is_verified && rng.bool_prob(0.90);
            let district_id = rng.u32_range(1, 78);
            let gender_id   = *rng.choice(GENDERS);
            let status = if is_verified { "VERIFIED" } else if rng.bool_prob(0.5) { "PENDING" } else { "INCOMPLETE" };

            writeln_csv(&mut w, &[
                &id.to_string(), "pbkdf2_sha256$390000$...", &fmt_ts(last_login), "false",
                &fake_idx(&mut rng), &fmt_ts(created_ts), &fmt_ts(modified_ts), "false",
                &mobile, &name, &email, &(id <= 2).to_string(), "true", "false",
                &is_verified.to_string(), &rng.u32_range(1, 5).to_string(), &has_kyc.to_string(),
                &rng.u32_range(1, 5).to_string(), &id.to_string(), &fake_referral(&mut rng),
                &rng.bool_prob(0.80).to_string(), &format!("{:04}-{:02}-{:02}", dob_year, dob_month, dob_day),
                &gender_id.to_string(), &fake_serial(&mut rng, "KH"), status,
                if is_verified { &"1" } else { "" },
                if is_verified { &fmt_ts(verified_ts) } else { "" },
                &bs_date(created_ts), role.as_str(), &district_id.to_string(), "AD",
                &format!("user{}", id), role.as_str(),
            ]);
        }
    }

    // ── 2. gateway_walletbalance ─────────────────────────────────────────────
    {
        let mut w = open_writer(&config.output_dir, "gateway_walletbalance.csv");
        writeln_csv(&mut w, &["id","idx","created_on","modified_on","is_obsolete","balance","secondary_balance","hold_balance","lien_amount","user_id"]);
        let now = now_ts();
        for uid in 1..=(config.n_users + config.n_merchants) {
            let bal = (rng.lognormal(13.0, 1.5) as i64).max(0);
            let hold = (bal as f64 * rng.f64() * 0.10) as i64;
            writeln_csv(&mut w, &[
                &uid.to_string(), &fake_idx(&mut rng), &fmt_ts(BASE_EPOCH + rng.next_u64() % ONE_YEAR_S),
                &fmt_ts(now), "false", &bal.to_string(), "0", &hold.to_string(), "0", &uid.to_string(),
            ]);
        }
    }

    // ── 3. Core transactions + wallet history + QR / service / remittance ────
    let mut wh_w   = open_writer(&config.output_dir, "gateway_wallethistory.csv");
    let mut qr_w   = open_writer(&config.output_dir, "qrapp_fonepaytransaction.csv");
    let mut svc_w  = open_writer(&config.output_dir, "service_servicelog.csv");
    let mut rem_w  = open_writer(&config.output_dir, "remittance_remittance.csv");
    let mut dis_w  = open_writer(&config.output_dir, "disbursement_transaction.csv");

    writeln_csv(&mut wh_w, &["id","idx","created_on","modified_on","is_obsolete","balance","secondary_balance","hold_balance","transaction_effect","transaction_id","user_id","lien_amount","created_on_np_date","primary_transaction_effect","secondary_transaction_effect","tx_type","is_anomaly"]);
    writeln_csv(&mut qr_w, &["id","idx","created_on","modified_on","is_obsolete","mid","midx","tid","iin","status","amount","bill_amount","markup_amount","settlement_amount","fee_amount","remarks","purpose","msg_id","trace_number","device_id","pan","on_us","bill_number","discount_amount","source_fee","destination_fee","acquiree_id","issuee_id","transaction_id","reference_number","coupon_discount","currency","created_on_np_date","is_anomaly"]);
    writeln_csv(&mut svc_w, &["id","idx","created_on","modified_on","is_obsolete","status","amount","response_id","detail","service_id","user_id","service_charge","max_tries","notify_mobile","created_on_np_date","is_anomaly"]);
    writeln_csv(&mut rem_w, &["id","idx","created_on","modified_on","is_obsolete","status","beneficary_name","beneficary_country","beneficary_address","beneficary_city","beneficary_mobile","beneficary_id_type","beneficary_id_no","beneficary_account_no","beneficary_bank_name","beneficary_bank_branch","sender_name","sender_country","sender_address","sender_city","sender_mobile","sender_id_type","sender_id_no","purpose","remit_type","local_currency","paying_currency","local_amount","remit_amount","amount","service_charge","exchange_rate","relation","control_no","remittance_agent_id","sender_id","third_party_send","coupon_amount","from_kpg","created_on_np_date","is_anomaly"]);
    writeln_csv(&mut dis_w, &["id","idx","created_on","modified_on","is_obsolete","amount","status","type","remarks","batch_id","destination_account_id","source_account_id","created_on_np_date","is_anomaly"]);

    let mut qr_count = 0usize;
    let mut svc_count = 0usize;
    let mut rem_count = 0usize;
    let mut dis_count = 0usize;

    for txn_id in 1..=config.n_transactions {
        let is_anomaly = rng.bool_prob(config.anomaly_rate);
        let tx_type    = tx_sampler.pick(&mut rng);
        let metrics    = get_metrics(tx_type);
        
        let uid        = rng.usize_range(1, config.n_users + 1);
        let mid        = graph.repeat_merchant(&mut rng, uid, config.n_merchants);
        graph.record_tx(uid, mid);
        let device_id  = graph.get_device(&mut rng, uid);

        let ts         = sample_timestamp(&mut rng, BASE_EPOCH, config.time_peak_hour, config.time_hour_std);
        
        // Use JSON driven distributions
        let amount = if is_anomaly {
            (rng.pareto_amount(config.anomaly_base, config.anomaly_alpha).min(200_000_00.0)) as i64
        } else {
            ((rng.lognormal(metrics.mu, metrics.sigma) as i64).max(100) / 100) * 100
        };

        let fee        = ((amount as f64 * metrics.fee_rate) as i64).max(0);
        let effect     = if matches!(tx_type, TxType::WalletTopup | TxType::MerchantIn | TxType::Remittance) { amount } else { -(amount + fee) };

        let status = if is_anomaly && rng.bool_prob(0.3) { "FAILED" }
                     else if rng.bool_prob(0.96)         { "SUCCESS" }
                     else                                { "PENDING" };

        let bal_snapshot = (rng.lognormal(13.0, 1.5) as i64).max(amount.abs());
        writeln_csv(&mut wh_w, &[
            &txn_id.to_string(), &fake_idx(&mut rng), &fmt_ts(ts), &fmt_ts(ts + 2), "false",
            &bal_snapshot.to_string(), "0", "0", &effect.to_string(), &txn_id.to_string(),
            &uid.to_string(), "0", &fmt_date(ts), &effect.to_string(), "0", tx_type.as_str(),
            &is_anomaly.to_string(),
        ]);

        match tx_type {
            TxType::QrPayment | TxType::MerchantIn | TxType::MerchantOut => {
                qr_count += 1;
                let merchant_id = mid + config.n_users;
                let bill_amt    = amount - (amount as f64 * 0.02) as i64;
                let settle_amt  = amount - fee;
                writeln_csv(&mut qr_w, &[
                    &qr_count.to_string(), &fake_idx(&mut rng), &fmt_ts(ts), &fmt_ts(ts + 1), "false",
                    &format!("M{:06}", merchant_id), &fake_idx(&mut rng), &format!("T{:08}", txn_id),
                    "FONEPAY", status, &amount.to_string(), &bill_amt.to_string(), "0", &settle_amt.to_string(),
                    &fee.to_string(), "", *rng.choice(PURPOSES), &fake_msg_id(&mut rng), &fake_trace(&mut rng),
                    &device_id, &fake_pan(&mut rng), &rng.bool_prob(0.40).to_string(), &format!("BILL{:010}", txn_id),
                    "0", &(fee / 2).to_string(), &(fee / 2).to_string(), &merchant_id.to_string(), &uid.to_string(),
                    &txn_id.to_string(), &fake_idx(&mut rng), "0", "NPR", &fmt_date(ts), &is_anomaly.to_string(),
                ]);
            }
            TxType::Utilities => {
                svc_count += 1;
                writeln_csv(&mut svc_w, &[
                    &svc_count.to_string(), &fake_idx(&mut rng), &fmt_ts(ts), &fmt_ts(ts + 2), "false",
                    status, &amount.to_string(), &format!("RSP{:012}", rng.next_u64() % 1_000_000_000_000u64),
                    "Utility payment", &rng.u32_range(1, 30).to_string(), &uid.to_string(), &fee.to_string(),
                    "3", &fake_mobile(&mut rng), &fmt_date(ts), &is_anomaly.to_string(),
                ]);
            }
            TxType::Remittance => {
                rem_count += 1;
                let exch_rate = 130.0 + rng.normal(0.0, 2.5);
                let remit_amt = (amount as f64 / exch_rate) as i64;
                let sender_country = *rng.choice(COUNTRIES);
                let ben_city = *rng.choice(CITIES);
                writeln_csv(&mut rem_w, &[
                    &rem_count.to_string(), &fake_idx(&mut rng), &fmt_ts(ts), &fmt_ts(ts + 3), "false", status,
                    &fake_name(&mut rng), "Nepal", "Kathmandu, Nepal", ben_city, &fake_mobile(&mut rng),
                    *rng.choice(ID_TYPES), &format!("{:09}", rng.u32_range(100_000_000, 999_999_999)),
                    &format!("{:016}", rng.next_u64() % 10_000_000_000_000_000u64), *rng.choice(BANKS),
                    ben_city, &fake_name(&mut rng), sender_country, "Abroad", sender_country,
                    &fake_mobile(&mut rng), *rng.choice(ID_TYPES), &format!("{:09}", rng.u32_range(100_000_000, 999_999_999)),
                    *rng.choice(PURPOSES), "INWARD", "NPR", "USD", &amount.to_string(), &remit_amt.to_string(),
                    &amount.to_string(), &fee.to_string(), &format!("{:.4}", exch_rate), "FAMILY",
                    &format!("CN{:012}", rng.next_u64() % 1_000_000_000_000u64), &rng.u32_range(1, 8).to_string(),
                    &uid.to_string(), "false", "0", "false", &fmt_date(ts), &is_anomaly.to_string(),
                ]);
            }
            TxType::BankTransfer | TxType::WalletTopup => {
                dis_count += 1;
                let dest_acc = mid + config.n_users;
                writeln_csv(&mut dis_w, &[
                    &dis_count.to_string(), &fake_idx(&mut rng), &fmt_ts(ts), &fmt_ts(ts + 5), "false",
                    &amount.to_string(), status, tx_type.as_str(), "", &rng.u32_range(1, 500).to_string(),
                    &dest_acc.to_string(), &uid.to_string(), &fmt_date(ts), &is_anomaly.to_string(),
                ]);
            }
        }
    }

    // ── 4. autho_useractionlog ────────────────────────────────────────────────
    {
        let mut w = open_writer(&config.output_dir, "autho_useractionlog.csv");
        writeln_csv(&mut w, &["id","action","remarks","created_on","request_path","remote_addr","request_method","user_id","profile_id"]);
        let actions = ["LOGIN","LOGOUT","KYC_SUBMIT","KYC_APPROVE","TOPUP","TRANSFER","CHANGE_PIN","UPDATE_PROFILE","VERIFY_OTP"];
        let methods = ["POST","GET","PUT","PATCH"];
        let paths   = ["/api/auth/login/","/api/wallet/topup/","/api/qr/pay/","/api/kyc/submit/","/api/profile/update/","/api/remit/"];
        let n_logs  = config.n_transactions / 3;
        for i in 1..=n_logs {
            let uid = rng.usize_range(1, config.n_users + 1);
            let ts  = sample_timestamp(&mut rng, BASE_EPOCH, config.time_peak_hour, config.time_hour_std);
            writeln_csv(&mut w, &[
                &i.to_string(), rng.choice(actions), "", &fmt_ts(ts), rng.choice(paths), &fake_ip(&mut rng),
                rng.choice(methods), &uid.to_string(), &uid.to_string(),
            ]);
        }
    }

    println!("\nAll files written to ./{}/", config.output_dir);
    Ok(())
}