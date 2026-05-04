#!/usr/bin/env python3
"""
Khalti Statistical DNA Extractor
"""

import os
import json
import math
from datetime import datetime, timedelta

import psycopg2
from psycopg2 import errors as pg_errors
from dotenv import load_dotenv

# Third-party progress bar
try:
    from tqdm import tqdm
except ImportError:
    import sys
    print("tqdm is required. Install with: pip install tqdm")
    sys.exit(1)


load_dotenv()

# ---------------------------------------------------------------------------
# Configuration (can be overridden via environment; defaults match original)
# ---------------------------------------------------------------------------
LOOKBACK_DAYS = int(os.getenv("KHALTI_LOOKBACK_DAYS", 14))
TIMEZONE = "Asia/Kathmandu"          # local cultural time
OUTPUT_FILE = "khalti_dna.json"

# ---------------------------------------------------------------------------
# Database connection
# ---------------------------------------------------------------------------
def get_conn():
    """Create a PostgreSQL connection from environment variables."""
    try:
        return psycopg2.connect(
            dbname=os.getenv("DB_NAME"),
            user=os.getenv("DB_USER"),
            password=os.getenv("DB_PASSWORD"),
            host=os.getenv("DB_HOST"),
            port=os.getenv("DB_PORT"),
        )
    except Exception as e:
        print(f"\n[!] Database connection error: {e}")
        raise SystemExit(1)


# ---------------------------------------------------------------------------
# SQL‑only extraction helpers
# ---------------------------------------------------------------------------
def compute_total_transactions(conn, window_start: str) -> int:
    """Count transactions in the window (merchant‑facing only)."""
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT COUNT(*) FROM qrapp_fonepaytransaction
            WHERE created_on >= %s AND amount > 0
            """,
            (window_start,),
        )
        return cur.fetchone()[0]


def compute_temporal_heatmap(conn, window_start: str) -> list:
    """
    Return a 24 (hours) × 7 (days, 0=Sunday) matrix of transaction volumes.
    """
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT
                EXTRACT(HOUR FROM created_on AT TIME ZONE %s)::int AS hour,
                EXTRACT(DOW FROM created_on AT TIME ZONE %s)::int AS dow,
                COUNT(*) AS volume
            FROM qrapp_fonepaytransaction
            WHERE created_on >= %s AND amount > 0
            GROUP BY 1, 2
            ORDER BY 1, 2
            """,
            (TIMEZONE, TIMEZONE, window_start),
        )
        rows = cur.fetchall()

    # Build 24×7 matrix (list of lists)
    heatmap = [[0] * 7 for _ in range(24)]  # hour row, day column
    for hour, dow, vol in rows:
        heatmap[hour][dow] = vol
    return heatmap


def compute_categorical_weights(conn, window_start: str) -> dict:
    """
    Extract probability distributions for critical categorical columns.
    Tables/columns targeted:
      - qrapp_fonepaytransaction.status
      - qrapp_fonepaytransaction.purpose
      - disbursement_transaction.type
      - remittance_remittance.remit_type
      - auth_useractionlog.action
    Only the top‑10 values (by volume) are kept per column.
    """
    targets = [
        ("qrapp_fonepaytransaction", "status"),
        ("qrapp_fonepaytransaction", "purpose"),
        ("disbursement_transaction", "type"),
        ("remittance_remittance", "remit_type"),
        ("auth_useractionlog", "action"),
    ]

    profiles = {}
    with conn.cursor() as cur:
        for table, column in targets:
            key = f"{table}.{column}"
            try:
                # If a table is missing, we simply record an error and continue
                cur.execute(
                    f"""
                    SELECT {column}, COUNT(*) AS volume
                    FROM {table}
                    WHERE created_on >= %s
                    GROUP BY {column}
                    ORDER BY volume DESC
                    LIMIT 10
                    """,
                    (window_start,),
                )
                results = cur.fetchall()
                total = sum(row[1] for row in results) if results else 1
                profiles[key] = {
                    str(row[0]): round(row[1] / total, 6)
                    for row in results if row[0] is not None
                }
            except (pg_errors.UndefinedTable, pg_errors.UndefinedColumn, Exception):
                # Rollback to clear any error state for this cursor
                conn.rollback()
                profiles[key] = {"error": "extraction_failed"}
    return profiles


def compute_monetary_pareto(conn, window_start: str) -> dict:
    """
    Calculate Pareto α for transaction amounts above the 95th percentile.
    MLE α = 1 + n / Σ ln(x_i / x_min)
    """
    with conn.cursor() as cur:
        cur.execute(
            """
            WITH threshold AS (
                SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY amount) AS p95
                FROM qrapp_fonepaytransaction
                WHERE created_on >= %s AND amount > 0
            ),
            tail AS (
                SELECT amount
                FROM qrapp_fonepaytransaction, threshold
                WHERE created_on >= %s AND amount >= threshold.p95 AND amount > 0
            )
            SELECT
                MAX(threshold.p95) AS p95,
                COUNT(*) AS tail_count,
                SUM(LN(amount / threshold.p95)) AS sum_ln_ratio
            FROM tail, threshold
            """,
            (window_start, window_start),
        )
        p95, tail_count, sum_ln_ratio = cur.fetchone()

    # Guard against degenerate cases
    if tail_count and sum_ln_ratio and sum_ln_ratio > 0:
        pareto_alpha = 1.0 + tail_count / sum_ln_ratio
    else:
        pareto_alpha = 0.0
        sum_ln_ratio = 0.0

    return {
        "pareto_alpha": round(float(pareto_alpha), 6),
        "tail_threshold": float(p95) if p95 is not None else 0.0,
        "tail_count": int(tail_count),
        "sum_log_ratio": float(sum_ln_ratio) if sum_ln_ratio else 0.0,
    }


def compute_user_degree_pareto(conn, window_start: str) -> dict:
    """
    Pareto α for user transaction frequency (degree distribution).
    Uses the MLE for a discrete power law with x_min = 1:
        α = 1 + n / Σ ln(k)
    where k = number of transactions per user.
    """
    with conn.cursor() as cur:
        cur.execute(
            """
            WITH user_counts AS (
                SELECT COUNT(*) AS tx_count
                FROM qrapp_fonepaytransaction
                WHERE created_on >= %s AND amount > 0
                GROUP BY user_id
            )
            SELECT
                COUNT(*) AS n,
                SUM(LN(tx_count)) AS sum_log
            FROM user_counts
            WHERE tx_count >= 1
            """,
            (window_start,),
        )
        n, sum_log = cur.fetchone()

    if n and sum_log and sum_log > 0:
        pareto_alpha = 1.0 + n / sum_log
    else:
        pareto_alpha = 0.0
        sum_log = 0.0

    return {
        "user_degree_pareto_alpha": round(float(pareto_alpha), 6),
        "num_users": int(n),
        "sum_log_degree": float(sum_log) if sum_log else 0.0,
    }


# ---------------------------------------------------------------------------
# Main orchestrator
# ---------------------------------------------------------------------------
def main():
    print("\n=== Khalti DNA Extractor ===")

    # ── Progress bar setup ──
    total_steps = 6
    with tqdm(total=total_steps, desc="Extracting DNA", unit="step",
              bar_format="{l_bar}{bar:30}{r_bar}") as pbar:

        # 1. Connect & prepare window
        pbar.set_postfix_str("Connecting to DB...")
        conn = get_conn()
        window_start = (datetime.now() - timedelta(days=LOOKBACK_DAYS)).strftime(
            "%Y-%m-%d %H:%M:%S"
        )
        pbar.update(1)

        # 2. Temporal heatmap
        pbar.set_postfix_str("Building temporal heatmap...")
        heatmap = compute_temporal_heatmap(conn, window_start)
        pbar.update(1)

        # 3. Categorical weights
        pbar.set_postfix_str("Weighting categoricals...")
        categorical_weights = compute_categorical_weights(conn, window_start)
        pbar.update(1)

        # 4. Monetary fat‑tail
        pbar.set_postfix_str("Calculating monetary Pareto α...")
        monetary = compute_monetary_pareto(conn, window_start)
        pbar.update(1)

        # 5. Graph topology (user degree)
        pbar.set_postfix_str("Capturing graph topology...")
        topology = compute_user_degree_pareto(conn, window_start)
        pbar.update(1)

        # 6. Compile & write final JSON
        pbar.set_postfix_str("Writing khalti_dna.json...")
        total_tx = compute_total_transactions(conn, window_start)
        report = {
            "metadata": {
                "extraction_timestamp": datetime.utcnow().isoformat() + "Z",
                "database": "khalti_fintech",
                "window_days": LOOKBACK_DAYS,
                "total_transactions": total_tx,
            },
            "temporal_heatmap": heatmap,
            "categorical_weights": categorical_weights,
            "monetary_distribution": monetary,
            "graph_topology": topology,
        }

        with open(OUTPUT_FILE, "w") as f:
            json.dump(report, f, indent=2, default=str)  # default=str catches any non‑serializable

        conn.close()
        pbar.update(1)
        pbar.set_postfix_str("Complete!")

    print(f"\n✔ DNA extraction complete. Report saved to '{OUTPUT_FILE}'\n")


if __name__ == "__main__":
    main()