use crate::types::{EntityType, Node, TransactionEdge, SherlockGraph};
use crate::csv::*;
use crate::progress::ProgressBar;
use std::io;
use std::time::Instant;

impl SherlockGraph {
    // ── O(1) node insertion ──────────────────
    pub fn add_node(&mut self, entity: EntityType) -> usize {
        if let Some(&idx) = self.entity_map.get(&entity) { return idx; }
        let idx = self.nodes.len();
        self.nodes.push(Node {
            entity: entity.clone(),
            page_rank: 0.0, scc_id: None,
            k_core: 0, cluster_coeff: 0.0, entropy: 0.0,
        });
        self.adj_list.push(Vec::new());
        self.entity_map.insert(entity, idx);
        idx
    }

    // ── Multigraph edge insertion ────────────
    pub fn add_transaction(
        &mut self,
        source: EntityType,
        target: EntityType,
        amount: f64,
        timestamp: u64,
    ) {
        let src_idx = self.add_node(source);
        let tgt_idx = self.add_node(target);
        self.adj_list[src_idx].push(TransactionEdge {
            target_idx: tgt_idx,
            amount,
            created_on: timestamp,
            burstiness: 0.0,
        });
    }

    // ─────────────────────────────────────────
    // 6. CSV ingestion methods (with progress)
    // ─────────────────────────────────────────

    /// Load wallet_history CSV.
    /// Edge: user_id → transaction_id, weight = |transaction_effect|
    pub fn load_wallet_history(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_wallet_history(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("wallet_history", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.user_id.is_empty() || r.transaction_id.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("USR:{}", r.user_id)),
                EntityType::Account(format!("TX:{}", r.transaction_id)),
                r.transaction_effect.abs(),
                r.created_on,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }

    /// Load qr_transactions CSV.
    /// Edge: acquiree_id → issuee_id, weight = amount
    pub fn load_qr_transactions(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_qr_transactions(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("qr_transactions", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.acquiree_id.is_empty() || r.issuee_id.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("ACQ:{}", r.acquiree_id)),
                EntityType::Account(format!("ISS:{}", r.issuee_id)),
                r.amount,
                r.created_on,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }

    /// Load service_payments CSV.
    /// Edge: user_id → service_id, weight = amount
    pub fn load_service_payments(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_service_payments(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("service_payments", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.user_id.is_empty() || r.service_id.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("USR:{}", r.user_id)),
                EntityType::Account(format!("SVC:{}", r.service_id)),
                r.amount,
                r.created_on,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }

    /// Load remittance CSV.
    /// Edge: sender_id → beneficary_account_no, weight = amount
    pub fn load_remittance(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_remittance(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("remittance", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.sender_id.is_empty() || r.beneficary_account_no.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("SND:{}", r.sender_id)),
                EntityType::Account(format!("BNF:{}", r.beneficary_account_no)),
                r.amount,
                r.created_on,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }

    /// Load disbursements CSV.
    /// Edge: source_account_id → destination_account_id, weight = amount
    pub fn load_disbursements(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_disbursements(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("disbursements", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.source_account_id.is_empty() || r.destination_account_id.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("SRC:{}", r.source_account_id)),
                EntityType::Account(format!("DST:{}", r.destination_account_id)),
                r.amount,
                r.created_on,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }

    /// Load unified mono-CSV format.
    /// Edge: user_id → counterparty_id, weight = amount
    pub fn load_unified_transactions(&mut self, path: &str) -> io::Result<usize> {
        let rows = parse_unified_transactions(path)?;
        let n = rows.len();
        let pb = ProgressBar::new("unified_transactions", n);
        let t0 = Instant::now();
        for (i, r) in rows.iter().enumerate() {
            if r.user_id.is_empty() || r.counterparty_id.is_empty() { continue; }
            self.add_transaction(
                EntityType::Account(format!("USR:{}", r.user_id)),
                EntityType::Account(format!("CP:{}", r.counterparty_id)),
                r.amount,
                r.timestamp,
            );
            if i % 500 == 0 { pb.update(i); }
        }
        pb.finish(t0.elapsed().as_millis());
        Ok(n)
    }
}
