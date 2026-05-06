import pandas as pd
import numpy as np
import networkx as nx
from tqdm import tqdm
import scipy.stats as stats
import multiprocessing as mp
from functools import partial
import warnings

warnings.filterwarnings('ignore')

def load_data(file_path):
    """Load data from CSV file."""
    print(f"Loading data from {file_path}...")
    return pd.read_csv(file_path)

def preprocess_data(df):
    """
    - Parse datetime
    - Sort by txn_time
    - Encode missing purpose
    - Encode status to 0/1
    """
    print("Preprocessing data...")
    df['txn_time'] = pd.to_datetime(df['txn_time'])
    df = df.sort_values('txn_time').reset_index(drop=True)
    df['purpose'] = df['purpose'].fillna('Unknown')
    df['status_int'] = df['status'].astype(int)
    return df

def calculate_entropy(amounts, purposes):
    """
    Entropy = - Σ (p * log(p)) where p = sum(amount per purpose) / total amount
    """
    if len(amounts) == 0:
        return 0
    
    # Group amounts by purpose and sum them
    purpose_sums = {}
    total_amount = 0
    for a, p in zip(amounts, purposes):
        purpose_sums[p] = purpose_sums.get(p, 0) + a
        total_amount += a
    
    if total_amount == 0:
        return 0
        
    proportions = [s / total_amount for s in purpose_sums.values()]
    return stats.entropy(proportions) if proportions else 0

def process_account_group(source_id, group, windows):
    group = group.sort_values('txn_time')
    amounts = group['amount'].values
    destinations = group['destination_id'].values
    purposes = group['purpose'].values
    statuses = group['status_int'].values
    txn_ids = group['txn_id'].values
    
    results = []
    
    for i in range(len(group)):
        txn_features = {'txn_id': txn_ids[i]}
        
        for w in windows:
            start_idx = max(0, i - w)
            window_amounts = amounts[start_idx:i]
            window_destinations = destinations[start_idx:i]
            window_purposes = purposes[start_idx:i]
            window_statuses = statuses[start_idx:i]
            
            curr_amount = amounts[i]
            curr_purpose = purposes[i]
            
            if len(window_amounts) > 0:
                avg_amt = np.mean(window_amounts)
                total_amt = np.sum(window_amounts)
                std_amt = np.std(window_amounts) if len(window_amounts) > 1 else 0
                bias_amt = curr_amount - avg_amt
                num_txns = len(window_amounts)
                unique_dest = len(set(window_destinations))
                unique_purp = len(set(window_purposes))
                unique_stat = len(set(window_statuses))
                
                ent_prev = calculate_entropy(window_amounts, window_purposes)
                # Combined for trading entropy
                purpose_counts = {}
                total_w_amt = 0
                for a, p in zip(window_amounts, window_purposes):
                    purpose_counts[p] = purpose_counts.get(p, 0) + a
                    total_w_amt += a
                
                # Add current
                purpose_counts[curr_purpose] = purpose_counts.get(curr_purpose, 0) + curr_amount
                total_combined_amt = total_w_amt + curr_amount
                
                proportions = [v / total_combined_amt for v in purpose_counts.values()]
                ent_combined = stats.entropy(proportions) if proportions else 0
                trading_entropy = ent_prev - ent_combined
            else:
                avg_amt = total_amt = std_amt = bias_amt = 0
                num_txns = unique_dest = unique_purp = unique_stat = 0
                trading_entropy = 0
            
            suffix = f"_{w}"
            txn_features[f'AvgAmountT{suffix}'] = avg_amt
            txn_features[f'TotalAmountT{suffix}'] = total_amt
            txn_features[f'StdAmountT{suffix}'] = std_amt
            txn_features[f'BiasAmountT{suffix}'] = bias_amt
            txn_features[f'NumberT{suffix}'] = num_txns
            txn_features[f'UniqueDestinationT{suffix}'] = unique_dest
            txn_features[f'UniquePurposeT{suffix}'] = unique_purp
            txn_features[f'UniqueStatusT{suffix}'] = unique_stat
            txn_features[f'TradingEntropyT{suffix}'] = trading_entropy
        
        results.append(txn_features)
    return results

def _process_group_wrapper(item, windows):
    source_id, group = item
    return process_account_group(source_id, group, windows)

def compute_temporal_features(df, windows=[1, 3, 5, 10, 20, 50, 100, 500]):
    """
    Compute temporal features using multiprocessing for performance.
    """
    print("Computing temporal features with multiprocessing...")
    grouped = list(df.groupby('source_id'))
    
    # Use pool to process groups in parallel
    num_cores = mp.cpu_count()
    pool = mp.Pool(processes=num_cores)
    
    # Partial function to fix windows parameter
    process_func = partial(_process_group_wrapper, windows=windows)
    
    results_nested = []
    for res in tqdm(pool.imap_unordered(process_func, grouped), 
                    total=len(grouped), desc="Processing account groups"):
        results_nested.extend(res)
    
    pool.close()
    pool.join()
    
    return pd.DataFrame(results_nested)

def compute_graph_features(df):
    """
    Build directed graph and compute node/neighborhood/temporal graph features.
    """
    print("Computing graph features...")
    
    # Initialize Graph
    G = nx.DiGraph()
    
    # Track metrics per node
    # node_data[node] = {seen_neighbors: set(), total_amt_in: 0, total_amt_out: 0, count: 0}
    node_stats = {}
    
    # Precompute PageRank on the FULL graph first (as requested: PageRank is global/precomputed)
    print("Precomputing global PageRank...")
    full_G = nx.DiGraph()
    for _, row in df.iterrows():
        u, v, amt = row['source_id'], row['destination_id'], row['amount']
        if full_G.has_edge(u, v):
            full_G[u][v]['weight'] += amt
        else:
            full_G.add_edge(u, v, weight=amt)
    pagerank_scores = nx.pagerank(full_G, weight='weight')
    
    graph_features = []
    
    # Iterate through transactions in temporal order to build graph dynamically
    for _, row in tqdm(df.iterrows(), total=len(df), desc="Graph feature computation"):
        u = row['source_id']
        v = row['destination_id']
        amt = row['amount']
        txn_id = row['txn_id']
        
        # Node-level features (BEFORE updating with current transaction for some, 
        # but out_degree(source) usually refers to current state or snapshot)
        # We'll compute features representing state UP TO this transaction.
        
        def get_node_stat(node):
            if node not in node_stats:
                node_stats[node] = {
                    'neighbors': set(), 
                    'amt_in': 0, 
                    'amt_out': 0, 
                    'txns': 0,
                    'neighbor_amts': [] # To calculate avg_neighbor_transaction_amount
                }
            return node_stats[node]

        u_stat = get_node_stat(u)
        v_stat = get_node_stat(v)
        
        # Edge recurrence: has this pair transacted before?
        edge_recurrence = 1 if G.has_edge(u, v) else 0
        
        # Neighborhood: ratio_new_neighbors (is 'v' a new neighbor for 'u'?)
        is_new_neighbor = 1 if v not in u_stat['neighbors'] else 0
        
        # Clustering Coefficient (approximate OK)
        # We can use nx.clustering(G, u) which is local clustering coefficient
        clustering_coeff = nx.clustering(G, u) if G.has_node(u) else 0
        
        # Compute features
        feat = {
            'txn_id': txn_id,
            'out_degree_source': G.out_degree(u) if G.has_node(u) else 0,
            'in_degree_destination': G.in_degree(v) if G.has_node(v) else 0,
            'weighted_out_degree_source': u_stat['amt_out'],
            'weighted_in_degree_destination': v_stat['amt_in'],
            'total_txns_source': u_stat['txns'],
            'total_txns_destination': v_stat['txns'],
            'num_unique_neighbors_source': len(u_stat['neighbors']),
            'avg_neighbor_amt_source': np.mean(u_stat['neighbor_amts']) if u_stat['neighbor_amts'] else 0,
            'ratio_new_neighbors': is_new_neighbor, # Simplified: binary for current txn
            'pagerank_source': pagerank_scores.get(u, 0),
            'pagerank_destination': pagerank_scores.get(v, 0),
            'clustering_coeff_source': clustering_coeff,
            'edge_recurrence': edge_recurrence
        }
        
        # Update graph and stats AFTER computing features for this txn
        if G.has_edge(u, v):
            G[u][v]['weight'] += amt
        else:
            G.add_edge(u, v, weight=amt)
            
        u_stat['neighbors'].add(v)
        u_stat['amt_out'] += amt
        u_stat['txns'] += 1
        u_stat['neighbor_amts'].append(amt)
        
        v_stat['amt_in'] += amt
        v_stat['txns'] += 1
        
        graph_features.append(feat)
        
    return pd.DataFrame(graph_features)

def build_feature_tensor(df, temporal_cols, windows):
    """
    Build numpy tensor: shape = (num_samples, num_features_per_window, num_time_windows)
    """
    print("Building feature tensor...")
    num_samples = len(df)
    num_windows = len(windows)
    
    # Map feature names to their base names (without window suffix)
    # Temporal features are like 'AvgAmountT_1', 'AvgAmountT_3', etc.
    base_features = [c.split('_')[0] for c in temporal_cols if '_1' in c]
    num_base_features = len(base_features)
    
    tensor = np.zeros((num_samples, num_base_features, num_windows))
    
    for i, w in enumerate(windows):
        for j, base in enumerate(base_features):
            col_name = f"{base}_{w}"
            tensor[:, j, i] = df[col_name].values
            
    return tensor

def save_results(df, tensor, labels, output_dir='data/proc'):
    """
    Save the DataFrame (CSV), Tensor (NPY), and Labels (NPY).
    """
    import os
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)
        print(f"Created directory: {output_dir}")

    df_path = os.path.join(output_dir, 'features.csv')
    tensor_path = os.path.join(output_dir, 'feature_tensor.npy')
    labels_path = os.path.join(output_dir, 'labels.npy')

    print(f"Saving results to {output_dir}...")
    df.to_csv(df_path, index=False)
    np.save(tensor_path, tensor)
    np.save(labels_path, labels)
    print("Saving complete.")

def main():
    # Configuration
    DATA_PATH = 'data/main.csv'
    WINDOWS = [1, 3, 5, 10, 20, 50, 100, 500]
    
    # 1. Load & Preprocess
    raw_df = load_data(DATA_PATH)
    df = preprocess_data(raw_df)
    
    # 2. Compute Temporal Features
    temp_feats_df = compute_temporal_features(df, windows=WINDOWS)
    
    # 3. Compute Graph Features
    graph_feats_df = compute_graph_features(df)
    
    # 4. Merge all features
    final_df = df.merge(temp_feats_df, on='txn_id').merge(graph_feats_df, on='txn_id')
    
    # 5. Build Tensor
    # Extract columns that belong to temporal windows
    temporal_cols = [c for c in temp_feats_df.columns if any(f"_{w}" in c for w in WINDOWS)]
    feature_tensor = build_feature_tensor(final_df, temporal_cols, WINDOWS)
    
    # 6. Labels
    labels = final_df['status_int'].values
    
    print("\nPipeline Complete!")
    print(f"Final DataFrame shape: {final_df.shape}")
    print(f"Feature Tensor shape: {feature_tensor.shape}")
    print(f"Labels shape: {labels.shape}")

    # 7. Save Results
    save_results(final_df, feature_tensor, labels)
    
    return final_df, feature_tensor, labels

if __name__ == "__main__":
    final_df, feature_tensor, labels = main()
