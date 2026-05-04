import os
import psycopg2
import pandas as pd
import numpy as np
import json
from datetime import datetime, timedelta
from dotenv import load_dotenv

load_dotenv()

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
        print(f"Database Connection Error: {e}")
        exit(1)

#  ANALYTICS PARAMETERS
LOOKBACK_DAYS = 14        
VELOCITY_THRESHOLD = 40  
DEVICE_THRESHOLD = 1     
BURST_THRESHOLD_SEC = 1800 

def fetch_data(conn):
    """Fetches a high-fidelity slice for heuristic analysis efficiently in one query."""
    start = (datetime.now() - timedelta(days=LOOKBACK_DAYS)).strftime('%Y-%m-%d 00:00:00')
    
    query = f"""
    SELECT 
        issuee_id AS user_id,
        acquiree_id AS merchant_id,
        amount::float,
        device_id,
        created_on AT TIME ZONE 'Asia/Kathmandu' AS created_at
    FROM qrapp_fonepaytransaction
    WHERE created_on >= '{start}'
    AND amount > 0;
    """
    df = pd.read_sql_query(query, conn)
    df['created_at'] = pd.to_datetime(df['created_at'])
    return df

def generate_comprehensive_metrics(df):
    """Calculates all 2.1, 2.2, and 2.3 metrics"""
    metrics = {}
    
    # Pre-sort for temporal and relational calcs
    df = df.sort_values(by=['user_id', 'created_at'])
    amounts = df['amount']

    #  § 2.1 - Distribution Metrics
    p95 = amounts.quantile(0.95)
    tail = amounts[amounts >= p95]
    alpha = 1.0 / np.log(tail / p95).mean() if not tail.empty and p95 > 0 else 0

    metrics["distribution"] = {
        "mean": float(amounts.mean()),
        "variance": float(amounts.var()),
        "skewness": float(amounts.skew()),
        "kurtosis": float(amounts.kurt()),
        "pareto_alpha_tail": float(alpha),
        "percentile_bands": {
            f"p{int(q*100)}": float(amounts.quantile(q)) 
            for q in np.arange(0.1, 1.0, 0.1)
        }
    }

    #  § 2.2 - Temporal Dynamics
    df['hour'] = df['created_at'].dt.hour
    df['day_of_week'] = df['created_at'].dt.dayofweek
    df['inter_arrival_sec'] = df.groupby('user_id')['created_at'].diff().dt.total_seconds()

    metrics["temporal_dynamics"] = {
        "hour_of_day_dist": {str(k): float(v) for k, v in df['hour'].value_counts(normalize=True).items()},
        "day_of_week_dist": {str(k): float(v) for k, v in df['day_of_week'].value_counts(normalize=True).items()},
        "inter_arrival": {
            "mean_sec": float(df['inter_arrival_sec'].mean()),
            "median_sec": float(df['inter_arrival_sec'].median())
        },
        "session_clustering": {
            "burst_behavior_pct": float((df['inter_arrival_sec'] < BURST_THRESHOLD_SEC).mean())
        }
    }

    #  § 2.3 - Relational Structure (Graphs)
    edges = df.groupby(['user_id', 'merchant_id']).size()
    user_degree = df.groupby('user_id')['merchant_id'].nunique()
    devices_per_user = df.groupby('user_id')['device_id'].nunique()

    cov_df = df[['amount', 'inter_arrival_sec']].dropna()
    cov_matrix = cov_df.cov().to_dict()

    metrics["relational_structure"] = {
        "user_merchant_degree": {
            "mean": float(user_degree.mean()),
            "max": int(user_degree.max())
        },
        "edge_weight_distribution": {
            "mean_tx_per_edge": float(edges.mean()),
            "max_tx_per_edge": int(edges.max()),
            "repeat_tx_frequency": float((edges > 1).mean())
        },
        "unique_devices_per_user": {
            "mean": float(devices_per_user.mean()),
            "max": int(devices_per_user.max())
        },
        "feature_covariance": {
            k: {k2: float(v2) for k2, v2 in v.items()} 
            for k, v in cov_matrix.items()
        }
    }

    return metrics, df

def detect_anomalies(df):
    """Filters user profiles heuristically based on velocity and device counts."""
    profiles = df.groupby('user_id').agg(
        tx_count=('amount', 'count'),
        unique_devices=('device_id', 'nunique')
    )
    
    high_risk = profiles[
        (profiles['tx_count'] > VELOCITY_THRESHOLD) | 
        (profiles['unique_devices'] >= DEVICE_THRESHOLD)
    ]
    return high_risk.sort_values(by='tx_count', ascending=False)

def main():
    print(f"[{datetime.now()}] Initializing Sherlock-Alpha Scan...")
    conn = get_conn()

    print(" -> Fetching temporal slice...")
    full_df = fetch_data(conn)
    
    if full_df.empty:
        print("No records found in the specified window.")
        conn.close()
        return

    print(" -> Computing structural metrics and heuristics...")
    metrics_report, enriched_df = generate_comprehensive_metrics(full_df)
    
    print(" -> Identifying Anomalies...")
    anomalies = detect_anomalies(enriched_df)

    report = {
        "metadata": {
            "window_days": LOOKBACK_DAYS,
            "total_records_analyzed": len(full_df),
            "generated_at": datetime.now().isoformat()
        },
        "system_metrics": metrics_report,
        "anomalies": anomalies.to_dict(orient='index')
    }

    output_file = 'sherlock_metrics_producer.json'
    with open(output_file, 'w') as f:
        json.dump(report, f, indent=4)

    print(f"\nScan Complete. Report exported to: '{output_file}'")
    print(f"Identified {len(anomalies)} anomalous user profiles.")
    conn.close()

if __name__ == "__main__":
    main()