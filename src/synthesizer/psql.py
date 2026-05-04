import os
import psycopg2
import pandas as pd
import numpy as np
import json
from datetime import datetime, timedelta
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv()

print(os.getenv("HELLO"))

# --- CONNECTION SETTINGS FROM ENV ---
def get_conn():
    try:
        return psycopg2.connect(
            dbname=os.getenv("DB_NAME"),
            user=os.getenv("DB_USER"),
            password=os.getenv("DB_PASSWORD"),
            host=os.getenv("DB_HOST"),
            port=os.getenv("DB_PORT")
        )
    except Exception as e:
        print(f"Error connecting to database: {e}")
        exit(1)

# --- ANALYTICS PARAMETERS ---
LOOKBACK_DAYS = 14        
SAMPLE_PERCENT = 1.0     
VELOCITY_THRESHOLD = 40  
DEVICE_THRESHOLD = 3     

def fetch_daily_slice(conn, date_obj):
    """Fetches a high-fidelity slice for heuristic analysis."""
    start = date_obj.strftime('%Y-%m-%d 00:00:00')
    end = (date_obj + timedelta(days=1)).strftime('%Y-%m-%d 00:00:00')
    
    query = f"""
    SELECT 
        issuee_id AS user_id,
        acquiree_id AS merchant_id,
        amount,
        device_id,
        created_on AT TIME ZONE 'Asia/Kathmandu' AS created_at
    FROM qrapp_fonepaytransaction
    WHERE created_on >= '{start}' AND created_on < '{end}'
    AND amount > 0;
    """
    return pd.read_sql_query(query, conn)

def get_distribution_profile(conn):
    """Calculates global distribution metrics using block-level sampling."""
    query = f"""
    SELECT 
        ABS(transaction_effect)::float AS amount
    FROM gateway_wallethistory TABLESAMPLE SYSTEM ({SAMPLE_PERCENT})
    WHERE created_on > NOW() - INTERVAL '{LOOKBACK_DAYS} days';
    """
    df = pd.read_sql_query(query, conn)
    
    if df.empty:
        return {}

    p = np.percentile(df['amount'], [50, 90, 95, 99, 10])
    mu = df['amount'].mean()
    sigma = df['amount'].std()
    
    tail = df['amount'][df['amount'] >= p[2]]
    alpha = 1.0 / np.log(tail / p[2]).mean() if not tail.empty else 0
    
    return {
        "mean": round(mu, 2),
        "stddev": round(sigma, 2),
        "p50": p[0], "p95": p[2], "p99": p[3], "p10": p[4],
        "pareto_alpha": round(alpha, 4)
    }

def main():
    print(f"[{datetime.now()}] Initializing Sherlock-Alpha Scan...")
    conn = get_conn()
    all_slices = []

    # 1. Temporal Data Collection
    for i in range(1, LOOKBACK_DAYS + 1):
        target_date = datetime.now() - timedelta(days=i)
        print(f" -> Processing Slice: {target_date.date()}")
        all_slices.append(fetch_daily_slice(conn, target_date))

    full_df = pd.concat(all_slices, ignore_index=True)

    # 2. Heuristic Fraud Detection
    print(" -> Computing behavioral heuristics...")
    user_profiles = full_df.groupby('user_id').agg({
        'amount': ['count', 'sum', 'mean', 'std'],
        'device_id': 'nunique',
        'merchant_id': 'nunique',
        'created_at': lambda x: (x.max() - x.min()).total_seconds() / 3600
    })

    user_profiles.columns = [
        'tx_count', 'total_volume', 'avg_amount', 'std_amount', 
        'unique_devices', 'unique_merchants', 'activity_span_hrs'
    ]

    # 3. Identifying Anomalies
    high_risk = user_profiles[
        (user_profiles['tx_count'] > VELOCITY_THRESHOLD) | 
        (user_profiles['unique_devices'] >= DEVICE_THRESHOLD)
    ].copy()

    # 4. Global Baseline
    print(" -> Calculating global distribution baseline...")
    baseline = get_distribution_profile(conn)

    # 5. Final Report
    report = {
        "metadata": {
            "window_days": LOOKBACK_DAYS,
            "total_records_analyzed": len(full_df),
            "generated_at": datetime.now().isoformat()
        },
        "baseline_metrics": baseline,
        "anomalies": high_risk.sort_values(by='tx_count', ascending=False).to_dict(orient='index')
    }

    output_file = 'sherlock_fraud_report.json'
    with open(output_file, 'w') as f:
        json.dump(report, f, indent=4)

    print(f"\nScan Complete. Report: '{output_file}'")
    print(f"Identified {len(high_risk)} anomalous user profiles.")
    conn.close()

if __name__ == "__main__":
    main()