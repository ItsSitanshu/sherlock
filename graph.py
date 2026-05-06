import pandas as pd
import networkx as nx
import numpy as np
import matplotlib.pyplot as plt

from misc.viz import visualize_interactive, visualize_sampled_graph, visualize_sccs

def plot_graph_summary(G):
    fig, axes = plt.subplots(2, 2, figsize=(12, 10))
    
    # Degree distribution
    in_degrees = [d for n,d in G.in_degree()]
    out_degrees = [d for n,d in G.out_degree()]
    axes[0,0].hist(in_degrees, bins=50, alpha=0.5, label='in-degree', log=True)
    axes[0,0].hist(out_degrees, bins=50, alpha=0.5, label='out-degree', log=True)
    axes[0,0].set_xlabel('Degree')
    axes[0,0].set_ylabel('Frequency (log)')
    axes[0,0].legend()
    axes[0,0].set_title('Degree Distribution')
    
    # PageRank distribution
    pr = np.array([data['pagerank'] for n,data in G.nodes(data=True)])
    axes[0,1].hist(pr, bins=50, log=True)
    axes[0,1].set_xlabel('PageRank')
    axes[0,1].set_title('PageRank Distribution')
    
    # SCC sizes
    sccs = list(nx.strongly_connected_components(G))
    scc_sizes = [len(c) for c in sccs]
    axes[1,0].hist(scc_sizes, bins=50, log=True)
    axes[1,0].set_xlabel('SCC size')
    axes[1,0].set_title('Strongly Connected Components')
    
    # Net flow distribution (top users)
    net_flows = [data['net_flow'] for n,data in G.nodes(data=True) if 'net_flow' in data]
    axes[1,1].hist(net_flows, bins=50, log=True)
    axes[1,1].set_xlabel('Net flow (out - in)')
    axes[1,1].set_title('Net Flow Distribution')
    
    plt.tight_layout()
    plt.show()

df = pd.read_csv("data/main.csv", parse_dates=["txn_time"])
print(df.head())

G = nx.DiGraph()

from tqdm import tqdm

for _, row in tqdm(df.iterrows(), total=len(df), desc="Constructing Graph"):
    src, dst = row["source_id"], row["destination_id"]
    amt = row["amount"]
    G.add_edge(src, dst, 
        total_amount=G.get_edge_data(src, dst, {}).get("total_amount", 0) + amt,
        count=G.get_edge_data(src, dst, {}).get("count", 0) + 1,
        last_time=max(
            G.get_edge_data(src, dst, {}).get("last_time", row["txn_time"]),
            row["txn_time"]
        ))

pagerank = nx.pagerank(G, weight="total_amount")
in_degree = dict(G.in_degree())
out_degree = dict(G.out_degree())
betweenness = nx.betweenness_centrality(G, weight="total_amount")  # heavy for large graphs; use sampling

for node in G.nodes():
    G.nodes[node]["pagerank"] = pagerank.get(node, 0)
    G.nodes[node]["in_degree"] = in_degree.get(node, 0)
    G.nodes[node]["out_degree"] = out_degree.get(node, 0)
    G.nodes[node]["net_flow"] = sum(amt for _, _, amt in G.out_edges(node, data="total_amount")) - \
                                 sum(amt for _, _, amt in G.in_edges(node, data="total_amount"))

# plot_graph_summary(G)
# visualize_interactive(G, 500, 10000)
# visualize_sampled_graph(G, sample_size=10, node_attribute='pagerank', edge_weight='total_amount')
visualize_sccs(G, num_sccs=4)
