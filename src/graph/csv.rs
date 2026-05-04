use crate::types::*;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

// ─────────────────────────────────────────────
// Parsing helpers
// ─────────────────────────────────────────────

pub(crate) fn parse_f64(s: &str) -> f64 { s.trim().parse().unwrap_or(0.0) }
pub(crate) fn parse_u64(s: &str) -> u64 { s.trim().parse().unwrap_or(0) }
pub(crate) fn parse_u32(s: &str) -> u32 { s.trim().parse().unwrap_or(0) }
pub(crate) fn parse_bool(s: &str) -> bool { matches!(s.trim(), "1" | "true" | "True" | "TRUE") }
pub(crate) fn s(v: &str) -> String { v.trim().to_string() }

/// Split a CSV line respecting double-quoted fields.
pub fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => { fields.push(cur.clone()); cur.clear(); }
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Read all data lines from a CSV file (skips header).
pub fn read_csv_lines(path: &str) -> io::Result<Vec<Vec<String>>> {
    let f = BufReader::new(File::open(path)?);
    let mut lines = f.lines();
    lines.next(); // skip header
    let mut out = Vec::new();
    for line in lines.flatten() {
        let line = line.trim().to_string();
        if !line.is_empty() { out.push(split_csv(&line)); }
    }
    Ok(out)
}

pub fn parse_wallet_history(path: &str) -> io::Result<Vec<WalletHistoryRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 17 { return None; }
        Some(WalletHistoryRow {
            id: s(&f[0]), idx: parse_u64(&f[1]), created_on: parse_u64(&f[2]),
            modified_on: parse_u64(&f[3]), is_obsolete: parse_bool(&f[4]),
            balance: parse_f64(&f[5]), secondary_balance: parse_f64(&f[6]),
            hold_balance: parse_f64(&f[7]), transaction_effect: parse_f64(&f[8]),
            transaction_id: s(&f[9]), user_id: s(&f[10]), lien_amount: parse_f64(&f[11]),
            created_on_np_date: s(&f[12]), primary_transaction_effect: parse_f64(&f[13]),
            secondary_transaction_effect: parse_f64(&f[14]), tx_type: s(&f[15]),
            is_anomaly: parse_bool(&f[16]),
        })
    }).collect())
}

pub fn parse_qr_transactions(path: &str) -> io::Result<Vec<QrTransactionRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 34 { return None; }
        Some(QrTransactionRow {
            id: s(&f[0]), idx: parse_u64(&f[1]), created_on: parse_u64(&f[2]),
            modified_on: parse_u64(&f[3]), is_obsolete: parse_bool(&f[4]),
            mid: s(&f[5]), midx: parse_u64(&f[6]), tid: s(&f[7]), iin: s(&f[8]),
            status: s(&f[9]), amount: parse_f64(&f[10]), bill_amount: parse_f64(&f[11]),
            markup_amount: parse_f64(&f[12]), settlement_amount: parse_f64(&f[13]),
            fee_amount: parse_f64(&f[14]), remarks: s(&f[15]), purpose: s(&f[16]),
            msg_id: s(&f[17]), trace_number: s(&f[18]), device_id: s(&f[19]),
            pan: s(&f[20]), on_us: parse_bool(&f[21]), bill_number: s(&f[22]),
            discount_amount: parse_f64(&f[23]), source_fee: parse_f64(&f[24]),
            destination_fee: parse_f64(&f[25]), acquiree_id: s(&f[26]),
            issuee_id: s(&f[27]), transaction_id: s(&f[28]),
            reference_number: s(&f[29]), coupon_discount: parse_f64(&f[30]),
            currency: s(&f[31]), created_on_np_date: s(&f[32]),
            is_anomaly: parse_bool(&f[33]),
        })
    }).collect())
}

pub fn parse_service_payments(path: &str) -> io::Result<Vec<ServicePaymentRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 16 { return None; }
        Some(ServicePaymentRow {
            id: s(&f[0]), idx: parse_u64(&f[1]), created_on: parse_u64(&f[2]),
            modified_on: parse_u64(&f[3]), is_obsolete: parse_bool(&f[4]),
            status: s(&f[5]), amount: parse_f64(&f[6]), response_id: s(&f[7]),
            detail: s(&f[8]), service_id: s(&f[9]), user_id: s(&f[10]),
            service_charge: parse_f64(&f[11]), max_tries: parse_u32(&f[12]),
            notify_mobile: s(&f[13]), created_on_np_date: s(&f[14]),
            is_anomaly: parse_bool(&f[15]),
        })
    }).collect())
}

pub fn parse_remittance(path: &str) -> io::Result<Vec<RemittanceRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 41 { return None; }
        Some(RemittanceRow {
            id: s(&f[0]), idx: parse_u64(&f[1]), created_on: parse_u64(&f[2]),
            modified_on: parse_u64(&f[3]), is_obsolete: parse_bool(&f[4]),
            status: s(&f[5]), beneficary_name: s(&f[6]), beneficary_country: s(&f[7]),
            beneficary_address: s(&f[8]), beneficary_city: s(&f[9]),
            beneficary_mobile: s(&f[10]), beneficary_id_type: s(&f[11]),
            beneficary_id_no: s(&f[12]), beneficary_account_no: s(&f[13]),
            beneficary_bank_name: s(&f[14]), beneficary_bank_branch: s(&f[15]),
            sender_name: s(&f[16]), sender_country: s(&f[17]), sender_address: s(&f[18]),
            sender_city: s(&f[19]), sender_mobile: s(&f[20]), sender_id_type: s(&f[21]),
            sender_id_no: s(&f[22]), purpose: s(&f[23]), remit_type: s(&f[24]),
            local_currency: s(&f[25]), paying_currency: s(&f[26]),
            local_amount: parse_f64(&f[27]), remit_amount: parse_f64(&f[28]),
            amount: parse_f64(&f[29]), service_charge: parse_f64(&f[30]),
            exchange_rate: parse_f64(&f[31]), relation: s(&f[32]),
            control_no: s(&f[33]), remittance_agent_id: s(&f[34]),
            sender_id: s(&f[35]), third_party_send: parse_bool(&f[36]),
            coupon_amount: parse_f64(&f[37]), from_kpg: parse_bool(&f[38]),
            created_on_np_date: s(&f[39]), is_anomaly: parse_bool(&f[40]),
        })
    }).collect())
}

pub fn parse_disbursements(path: &str) -> io::Result<Vec<DisbursementRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 14 { return None; }
        Some(DisbursementRow {
            id: s(&f[0]), idx: parse_u64(&f[1]), created_on: parse_u64(&f[2]),
            modified_on: parse_u64(&f[3]), is_obsolete: parse_bool(&f[4]),
            amount: parse_f64(&f[5]), status: s(&f[6]), tx_type: s(&f[7]),
            remarks: s(&f[8]), batch_id: s(&f[9]),
            destination_account_id: s(&f[10]), source_account_id: s(&f[11]),
            created_on_np_date: s(&f[12]), is_anomaly: parse_bool(&f[13]),
        })
    }).collect())
}

/// Parse unified mono-CSV format.
/// Columns: user_id,counterparty_id,amount,timestamp,qr_status,qr_purpose,disb_type,remit_type,auth_action,device_id,is_burst
pub fn parse_unified_transactions(path: &str) -> io::Result<Vec<TransactionRow>> {
    Ok(read_csv_lines(path)?.into_iter().filter_map(|f| {
        if f.len() < 11 { return None; }
        Some(TransactionRow {
            user_id: s(&f[0]),
            counterparty_id: s(&f[1]),
            amount: parse_f64(&f[2]),
            timestamp: parse_u64(&f[3]),
            qr_status: s(&f[4]),
            qr_purpose: s(&f[5]),
            disb_type: s(&f[6]),
            remit_type: s(&f[7]),
            auth_action: s(&f[8]),
            device_id: s(&f[9]),
            is_burst: parse_bool(&f[10]),
        })
    }).collect())
}
