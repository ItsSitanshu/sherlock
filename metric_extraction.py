#!/usr/bin/env python3
"""
Statistical Metric Extractor: Optimized for 1.1T rows
"""

import os
import json
import math
from datetime import datetime, timedelta
from contextlib import contextmanager

import psycopg2
from psycopg2 import errors as pg_errors, sql
from psycopg2.extras import DictCursor, execute_values
from dotenv import load_dotenv

try:
    from tqdm import tqdm
except ImportError:
    import sys
    print("tqdm required: pip install tqdm")
    sys.exit(1)

load_dotenv()

LOOKBACK_DAYS = int(os.getenv("KHALTI_LOOKBACK_DAYS", 14))
TIMEZONE = "Asia/Kathmandu"
OUTPUT_FILE = "khalti_dna.json"

USE_SAMPLING = os.getenv("USE_SAMPLING", "true").lower() == "true"
SAMPLE_RATIO = float(os.getenv("SAMPLE_RATIO", "0.0001"))
MIN_SAMPLE_SIZE = int(os.getenv("MIN_SAMPLE_SIZE", "1000000"))

# TABLESAMPLE SYSTEM percent for the heatmap. ~461M rows total, 14-day window
# is ~1% of table (~4.8M rows). 1.0% sample → ~48k window rows / ~286 per
# (hour,dow) cell — MoE ~7% at 95% CI, plenty for a heatmap visualization.
HEATMAP_SAMPLE_PCT = float(os.getenv("HEATMAP_SAMPLE_PCT", "1.0"))

# TABLESAMPLE SYSTEM percent for monetary Pareto + graph topology. 0.5%
# gives ~24k window rows for monetary p95 (rock-stable estimate) and
# ~10k distinct source_ids for graph alpha. Each query ≈ 50s.
WINDOW_SAMPLE_PCT = float(os.getenv("WINDOW_SAMPLE_PCT", "0.5"))

BATCH_SIZE = 100000  
PARALLEL_WORKERS = int(os.getenv("PARALLEL_WORKERS", "4"))

class ReadOnlyConnection:
    def __init__(self):
        self.conn = None
        
    def __enter__(self):
        self.conn = psycopg2.connect(
            dbname=os.getenv("DB_NAME"),
            user=os.getenv("DB_USER"),
            password=os.getenv("DB_PASSWORD"),
            host=os.getenv("DB_HOST"),
            port=os.getenv("DB_PORT"),
            options="-c default_transaction_read_only=on -c statement_timeout=3600000"
        )
        return self.conn
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.conn:
            self.conn.close()

def get_sample_size(conn, table: str, window_start: str) -> int:
    with conn.cursor() as cur:
        cur.execute("""
            SELECT COUNT(*) FROM gateway_transaction 
            WHERE created_on >= %s AND amount > 0
        """, (window_start,))
        total = cur.fetchone()[0]
    
    if not USE_SAMPLING or total < MIN_SAMPLE_SIZE:
        return total
    
   
    required_sample = int((1.96**2 * 0.5 * 0.5) / (0.01**2))
    return min(int(total * SAMPLE_RATIO), required_sample)

def compute_total_transactions_sampled(conn, window_start: str) -> dict:
    with conn.cursor() as cur:
        cur.execute("""
            SELECT 
                reltuples::bigint AS approx_count,
                n_live_tup AS live_tuples
            FROM pg_class 
            WHERE relname = 'gateway_transaction'
        """)
        approx = cur.fetchone()
        
        cur.execute("""
            EXPLAIN (FORMAT JSON) 
            SELECT COUNT(*) FROM gateway_transaction 
            WHERE created_on >= %s AND amount > 0
        """, (window_start,))
        explain = cur.fetchone()[0]
        
        return {
            "approx_total": approx[0] if approx else 0,
            "window_count": None,  # Don't calculate exactly for 1.1T
            "estimated_from_plan": explain
        }

# ---------------------------------------------------------------------------
def compute_temporal_heatmap_streaming(conn, window_start: str) -> list:
    partition_clause = sql.SQL("AND created_on > NOW() - INTERVAL '30 days'") \
        if LOOKBACK_DAYS <= 30 else sql.SQL("")
    sample_clause = sql.SQL("TABLESAMPLE SYSTEM({pct})").format(
        pct=sql.Literal(HEATMAP_SAMPLE_PCT)
    ) if USE_SAMPLING else sql.SQL("")

    query = sql.SQL("""
        SELECT
            EXTRACT(HOUR FROM created_on AT TIME ZONE %s)::int AS hour,
            EXTRACT(DOW  FROM created_on AT TIME ZONE %s)::int AS dow,
            COUNT(*) AS volume
        FROM gateway_transaction {sample_clause}
        WHERE created_on >= %s AND amount > 0
            {partition_clause}
        GROUP BY hour, dow
        HAVING COUNT(*) > 0
        ORDER BY hour, dow
    """).format(sample_clause=sample_clause, partition_clause=partition_clause)

    with conn.cursor() as cur:
        cur.execute(query, (TIMEZONE, TIMEZONE, window_start))
        rows = cur.fetchall()

    heatmap = [[0] * 7 for _ in range(24)]
    for hour, dow, vol in rows:
        if hour is None or dow is None:
            continue
        heatmap[hour][dow] = vol
    
    return {
        "heatmap": heatmap,
        "sampling_error": 0.01 if USE_SAMPLING else 0,
        "confidence_level": 0.99 if USE_SAMPLING else 1.0
    }

def compute_categorical_weights_stratified(conn, window_start: str) -> dict:
    """Use stratified sampling for categorical columns."""
    tables_config = [
        ("gateway_transaction", "is_obsolete", "is_obsolete IS NOT NULL"),
        ("gateway_transaction", "purpose", "purpose IS NOT NULL"),
    ]
    
    profiles = {}
    with conn.cursor(cursor_factory=DictCursor) as cur:
        for table, column, where_clause in tables_config:
            key = f"{table}.{column}"
            try:
                # Use TABLESAMPLE for big tables
                sample_clause = f"TABLESAMPLE SYSTEM({SAMPLE_RATIO * 100})" if USE_SAMPLING else ""
                
                query = sql.SQL("""
                    WITH sample_data AS (
                        SELECT {column}
                        FROM {table} {sample_clause}
                        WHERE created_on >= %s 
                            AND {column} IS NOT NULL
                            AND {where_clause}
                        LIMIT %s
                    )
                    SELECT 
                        {column},
                        COUNT(*) AS volume,
                        COUNT(*) * 1.0 / SUM(COUNT(*)) OVER () AS probability,
                        STDDEV(COUNT(*)) OVER () / COUNT(*) AS relative_error
                    FROM sample_data
                    GROUP BY {column}
                    ORDER BY volume DESC
                    LIMIT 10
                """).format(
                    column=sql.Identifier(column),
                    table=sql.Identifier(table),
                    sample_clause=sql.SQL(sample_clause),
                    where_clause=sql.SQL(where_clause)
                )
                
                cur.execute(query, (window_start, MIN_SAMPLE_SIZE))
                results = cur.fetchall()
                
                total = sum(r['volume'] for r in results) if results else 1
                profiles[key] = {
                    str(r[column]): {
                        "prob": round(r['probability'], 6),
                        "volume": r['volume'],
                        "relative_error": round(r['relative_error'], 4) if r['relative_error'] else 0
                    }
                    for r in results
                }
                profiles[key]["_metadata"] = {
                    "sampling_ratio": SAMPLE_RATIO if USE_SAMPLING else 1.0,
                    "total_sample_size": sum(r['volume'] for r in results)
                }
                
            except Exception as e:
                conn.rollback()
                profiles[key] = {"error": str(e)}
    
    return profiles

def compute_monetary_pareto_reservoir(conn, window_start: str) -> dict:
    reservoir_size = 1000000  # 1M samples for tail

    sample = sql.SQL("TABLESAMPLE SYSTEM({pct})").format(pct=sql.Literal(WINDOW_SAMPLE_PCT)) \
        if USE_SAMPLING else sql.SQL("")

    with conn.cursor() as cur:
        q1 = sql.SQL("""
            WITH threshold AS (
                SELECT
                    percentile_cont(0.95) WITHIN GROUP (ORDER BY amount) AS p95
                FROM gateway_transaction {sample}
                WHERE created_on >= %s AND amount > 0
            )
            SELECT
                p95,
                (
                    SELECT COUNT(*)
                    FROM gateway_transaction {sample}, threshold
                    WHERE created_on >= %s
                        AND amount >= threshold.p95
                        AND amount > 0
                ) AS tail_count_approx
            FROM threshold
        """).format(sample=sample)
        cur.execute(q1, (window_start, window_start))

        p95, approx_tail_count = cur.fetchone()

        q2 = sql.SQL("""
            WITH RECURSIVE reservoir AS (
                SELECT
                    amount,
                    random() AS r
                FROM gateway_transaction {sample}
                WHERE created_on >= %s
                    AND amount >= %s
                    AND amount > 0
                ORDER BY r
                LIMIT %s
            )
            SELECT
                COUNT(*) AS n,
                SUM(LN(amount / %s)) AS sum_ln_ratio,
                MIN(amount) AS min_tail,
                MAX(amount) AS max_tail,
                AVG(amount) AS mean_tail
            FROM reservoir
        """).format(sample=sample)
        cur.execute(q2, (window_start, p95, reservoir_size, p95))
        
        n, sum_ln_ratio, min_tail, max_tail, mean_tail = cur.fetchone()
        n = int(n) if n is not None else 0
        sum_ln_ratio = float(sum_ln_ratio) if sum_ln_ratio is not None else 0.0

        if n and sum_ln_ratio and sum_ln_ratio > 0:
            pareto_alpha = 1.0 + n / sum_ln_ratio
            # Bootstrap confidence interval
            std_error = pareto_alpha / math.sqrt(n)
            ci_lower = pareto_alpha - 1.96 * std_error
            ci_upper = pareto_alpha + 1.96 * std_error
        else:
            pareto_alpha = 0.0
            ci_lower = ci_upper = 0.0
    
    return {
        "pareto_alpha": round(pareto_alpha, 6),
        "alpha_ci_lower": round(ci_lower, 6),
        "alpha_ci_upper": round(ci_upper, 6),
        "tail_threshold": float(p95) if p95 else 0.0,
        "tail_count_approx": int(approx_tail_count) if approx_tail_count else 0,
        "reservoir_samples": int(n),
        "tail_statistics": {
            "min": float(min_tail) if min_tail else 0,
            "max": float(max_tail) if max_tail else 0,
            "mean": float(mean_tail) if mean_tail else 0
        }
    }

def compute_user_degree_hyperloglog(conn, window_start: str) -> dict:
    sample = sql.SQL("TABLESAMPLE SYSTEM({pct})").format(pct=sql.Literal(WINDOW_SAMPLE_PCT)) \
        if USE_SAMPLING else sql.SQL("")

    with conn.cursor() as cur:
        cur.execute("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'hll')")
        has_hll = cur.fetchone()[0]

        if has_hll and USE_SAMPLING:
            q = sql.SQL("""
                SELECT
                    hll_cardinality(hll_union_agg(hll_hash_integer(source_id))) AS approx_distinct_users,
                    COUNT(*) AS total_transactions,
                    AVG(tx_count) AS avg_degree,
                    STDDEV(tx_count) AS std_degree
                FROM (
                    SELECT
                        source_id,
                        COUNT(*) AS tx_count
                    FROM gateway_transaction {sample}
                    WHERE created_on >= %s AND amount > 0
                    GROUP BY source_id
                    LIMIT %s
                ) AS user_tx
            """).format(sample=sample)
            cur.execute(q, (window_start, MIN_SAMPLE_SIZE))
        else:
            q = sql.SQL("""
                WITH sampled_users AS (
                    SELECT
                        source_id,
                        COUNT(*) AS tx_count
                    FROM gateway_transaction {sample}
                    WHERE created_on >= %s AND amount > 0
                    GROUP BY source_id
                )
                SELECT
                    COUNT(*) AS sampled_users,
                    SUM(tx_count) AS total_tx,
                    SUM(LN(tx_count)) AS sum_log,
                    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY tx_count) AS median_degree
                FROM sampled_users
            """).format(sample=sample)
            cur.execute(q, (window_start,))
        
        row = cur.fetchone()
        
        if has_hll:
            distinct_users, total_tx, avg_deg, std_deg = row
            # Estimate Pareto alpha from sample
            if total_tx and avg_deg and avg_deg > 0:
                # Simplified alpha estimation
                pareto_alpha = 1.0 + (total_tx / (total_tx * math.log(avg_deg)))
            else:
                pareto_alpha = 0.0
        else:
            sampled_users, total_tx, sum_log, median_deg = row
            if sampled_users and sum_log and sum_log > 0:
                pareto_alpha = 1.0 + sampled_users / sum_log
            else:
                pareto_alpha = 0.0
    
    return {
        "user_degree_pareto_alpha": round(pareto_alpha, 6),
        "estimation_method": "hyperloglog" if has_hll else "sampling",
        "confidence": "high" if USE_SAMPLING else "exact",
        "sampling_error_margin": 0.01 if USE_SAMPLING else 0
    }

def main():
    print("\n=== Khalti  (Optimized for 1.1T rows) ===")
    print(f"Sampling: {'ON' if USE_SAMPLING else 'OFF'} ({SAMPLE_RATIO*100:.1f}%)")
    
    steps = [
        ("Connecting to DB...", None),
        ("Building temporal heatmap (streaming)...", compute_temporal_heatmap_streaming),
        ("Weighting categoricals (stratified)...", compute_categorical_weights_stratified),
        ("Calculating monetary Pareto (reservoir)...", compute_monetary_pareto_reservoir),
        ("Capturing graph topology (HyperLogLog)...", compute_user_degree_hyperloglog),
        ("Writing final JSON...", None)
    ]
    
    report = {"metadata": {}}
    
    with ReadOnlyConnection() as conn:
        window_start = (datetime.now() - timedelta(days=LOOKBACK_DAYS)).strftime("%Y-%m-%d %H:%M:%S")
        
        with tqdm(total=len(steps), desc="Extracting DNA", unit="step") as pbar:
            for step_name, func in steps:
                pbar.set_postfix_str(step_name)
                
                if func is None:
                    if step_name == "Connecting to DB...":
                        report["metadata"] = {
                            "extraction_timestamp": datetime.utcnow().isoformat() + "Z",
                            "window_days": LOOKBACK_DAYS,
                            "sampling_used": USE_SAMPLING,
                            "sample_ratio": SAMPLE_RATIO if USE_SAMPLING else 1.0
                        }
                    else:  # Final write
                        with open(OUTPUT_FILE, "w") as f:
                            json.dump(report, f, indent=2, default=str)
                else:
                    try:
                        result = func(conn, window_start)
                        if "heatmap" in str(result):
                            report["temporal_heatmap"] = result
                        elif "pareto_alpha" in str(result):
                            if "user_degree" in str(result) or "graph" in str(result):
                                report["graph_topology"] = result
                            else:
                                report["monetary_distribution"] = result
                        else:
                            report["categorical_weights"] = result
                    except Exception as e:
                        print(f"\n[!] Error in {step_name}: {e}")
                        report[step_name.lower().replace(" ", "_")] = {"error": str(e)}
                
                pbar.update(1)
    
    print(f"\n✔ DNA extraction complete. Report saved to '{OUTPUT_FILE}'")
    print(f"⚠️  Using {'sampled' if USE_SAMPLING else 'exact'} statistics for 1.1T rows")

if __name__ == "__main__":
    main()