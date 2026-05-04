use std::collections::HashMap;

// ─────────────────────────────────────────────
// 1. Entity / Node / Edge types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityType {
    Account(String),
}

impl EntityType {
    pub fn as_str(&self) -> &str {
        match self { EntityType::Account(s) => s }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub entity: EntityType,
    // --- Global Level (G) ---
    pub page_rank: f64,
    // --- Meso-Level ---
    pub scc_id: Option<usize>,
    // --- Local Level (C) ---
    pub k_core: u32,
    pub cluster_coeff: f64,
    pub entropy: f64,
}

#[derive(Debug, Clone)]
pub struct TransactionEdge {
    pub target_idx: usize,
    pub amount: f64,
    pub created_on: u64,
    pub burstiness: f64,
}

// ─────────────────────────────────────────────
// 2. CSV row representations (one per source)
// ─────────────────────────────────────────────

/// wallet_history rows
#[derive(Debug)]
pub struct WalletHistoryRow {
    pub id: String,
    pub idx: u64,
    pub created_on: u64,
    pub modified_on: u64,
    pub is_obsolete: bool,
    pub balance: f64,
    pub secondary_balance: f64,
    pub hold_balance: f64,
    pub transaction_effect: f64,
    pub transaction_id: String,
    pub user_id: String,
    pub lien_amount: f64,
    pub created_on_np_date: String,
    pub primary_transaction_effect: f64,
    pub secondary_transaction_effect: f64,
    pub tx_type: String,
    pub is_anomaly: bool,
}

/// qr_transactions rows
#[derive(Debug)]
pub struct QrTransactionRow {
    pub id: String,
    pub idx: u64,
    pub created_on: u64,
    pub modified_on: u64,
    pub is_obsolete: bool,
    pub mid: String,
    pub midx: u64,
    pub tid: String,
    pub iin: String,
    pub status: String,
    pub amount: f64,
    pub bill_amount: f64,
    pub markup_amount: f64,
    pub settlement_amount: f64,
    pub fee_amount: f64,
    pub remarks: String,
    pub purpose: String,
    pub msg_id: String,
    pub trace_number: String,
    pub device_id: String,
    pub pan: String,
    pub on_us: bool,
    pub bill_number: String,
    pub discount_amount: f64,
    pub source_fee: f64,
    pub destination_fee: f64,
    pub acquiree_id: String,
    pub issuee_id: String,
    pub transaction_id: String,
    pub reference_number: String,
    pub coupon_discount: f64,
    pub currency: String,
    pub created_on_np_date: String,
    pub is_anomaly: bool,
}

/// service_payments rows
#[derive(Debug)]
pub struct ServicePaymentRow {
    pub id: String,
    pub idx: u64,
    pub created_on: u64,
    pub modified_on: u64,
    pub is_obsolete: bool,
    pub status: String,
    pub amount: f64,
    pub response_id: String,
    pub detail: String,
    pub service_id: String,
    pub user_id: String,
    pub service_charge: f64,
    pub max_tries: u32,
    pub notify_mobile: String,
    pub created_on_np_date: String,
    pub is_anomaly: bool,
}

/// remittance rows
#[derive(Debug)]
pub struct RemittanceRow {
    pub id: String,
    pub idx: u64,
    pub created_on: u64,
    pub modified_on: u64,
    pub is_obsolete: bool,
    pub status: String,
    pub beneficary_name: String,
    pub beneficary_country: String,
    pub beneficary_address: String,
    pub beneficary_city: String,
    pub beneficary_mobile: String,
    pub beneficary_id_type: String,
    pub beneficary_id_no: String,
    pub beneficary_account_no: String,
    pub beneficary_bank_name: String,
    pub beneficary_bank_branch: String,
    pub sender_name: String,
    pub sender_country: String,
    pub sender_address: String,
    pub sender_city: String,
    pub sender_mobile: String,
    pub sender_id_type: String,
    pub sender_id_no: String,
    pub purpose: String,
    pub remit_type: String,
    pub local_currency: String,
    pub paying_currency: String,
    pub local_amount: f64,
    pub remit_amount: f64,
    pub amount: f64,
    pub service_charge: f64,
    pub exchange_rate: f64,
    pub relation: String,
    pub control_no: String,
    pub remittance_agent_id: String,
    pub sender_id: String,
    pub third_party_send: bool,
    pub coupon_amount: f64,
    pub from_kpg: bool,
    pub created_on_np_date: String,
    pub is_anomaly: bool,
}

/// disbursement rows
#[derive(Debug)]
pub struct DisbursementRow {
    pub id: String,
    pub idx: u64,
    pub created_on: u64,
    pub modified_on: u64,
    pub is_obsolete: bool,
    pub amount: f64,
    pub status: String,
    pub tx_type: String,
    pub remarks: String,
    pub batch_id: String,
    pub destination_account_id: String,
    pub source_account_id: String,
    pub created_on_np_date: String,
    pub is_anomaly: bool,
}

// ─────────────────────────────────────────────
// 3. Graph state struct
// ─────────────────────────────────────────────

pub struct SherlockGraph {
    pub nodes: Vec<Node>,
    pub adj_list: Vec<Vec<TransactionEdge>>,
    pub(crate) entity_map: HashMap<EntityType, usize>,
}

impl SherlockGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), adj_list: Vec::new(), entity_map: HashMap::new() }
    }
}
