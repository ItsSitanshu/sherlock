from pyvis.network import Network
import numpy as np
import matplotlib.pyplot as plt
import networkx as nx


def visualize_interactive(G, max_nodes=200, max_edges=500):
    """Create an interactive HTML graph."""
    # Sample subgraph if too large
    if G.number_of_nodes() > max_nodes:
        # Take nodes with highest PageRank
        nodes_by_pr = sorted(G.nodes(), key=lambda n: G.nodes[n].get('pagerank', 0), reverse=True)
        sampled_nodes = nodes_by_pr[:max_nodes]
        H = G.subgraph(sampled_nodes).copy()
        # Limit edges further if needed
        if H.number_of_edges() > max_edges:
            # Keep only edges with largest total_amount
            edges_sorted = sorted(H.edges(data=True), key=lambda e: e[2].get('total_amount', 0), reverse=True)
            H = nx.DiGraph()
            H.add_nodes_from(sampled_nodes)
            H.add_edges_from(edges_sorted[:max_edges])
    else:
        H = G
    
    # pyvis network - set notebook=False to avoid template rendering issues in some environments
    net = Network(height="1200px", width="100%", directed=True, notebook=False)
    
    # Add nodes with tooltips and colors based on PageRank
    for node, data in H.nodes(data=True):
        pr = data.get('pagerank', 0)
        color = f'rgba(255, {int(255*(1-pr*10))}, 0, 0.8)'  # redder for higher PageRank
        title = f"Node: {node}<br>PageRank: {pr:.4f}<br>Degree: {data.get('in_degree',0)}/{data.get('out_degree',0)}"
        net.add_node(node, label=str(node), title=title, color=color)
    
    # Add edges with width proportional to log(amount)
    max_amt = max((data.get('total_amount', 1) for _, _, data in H.edges(data=True)), default=1)
    for u, v, data in H.edges(data=True):
        amt = data.get('total_amount', 1)
        # Normalize log width based on max amount in the subgraph
        width = 1 + 5 * (np.log1p(amt) / np.log1p(max_amt))
        title = f"Amount: {amt:.2f}<br>Count: {data.get('count',0)}"
        net.add_edge(u, v, value=width, title=title)
    
    net.write_html("graph.html")
    print("Interactive graph saved as graph.html")

def visualize_sampled_graph(G, sample_size=50, node_attribute='pagerank', edge_weight='total_amount'):
    """
    Visualize a random sample of nodes and their induced subgraph.
    
    Parameters:
    - G: NetworkX DiGraph
    - sample_size: number of nodes to sample
    - node_attribute: attribute for node coloring (e.g., 'pagerank', 'in_degree', 'net_flow')
    - edge_weight: edge attribute for edge width/thickness
    """
    # Sample nodes (prefer high-degree nodes if you want to see important structure)
    nodes = list(G.nodes())
    if len(nodes) > sample_size:
        # Stratified: mix top-degree and random
        degrees = dict(G.degree())
        top_nodes = sorted(degrees, key=degrees.get, reverse=True)[:sample_size//2]
        random_nodes = np.random.choice([n for n in nodes if n not in top_nodes], 
                                        size=sample_size - len(top_nodes), replace=False)
        sampled_nodes = list(set(top_nodes + list(random_nodes)))
    else:
        sampled_nodes = nodes
    
    # Induced subgraph
    H = G.subgraph(sampled_nodes).copy()
    
    # Node colors based on attribute
    node_values = [H.nodes[n].get(node_attribute, 0) for n in H.nodes()]
    # Normalize for colormap
    vmin, vmax = min(node_values), max(node_values)
    if vmax - vmin == 0:
        node_colors = 'blue'
    else:
        node_colors = [plt.cm.viridis((val - vmin)/(vmax - vmin)) for val in node_values]
    
    # Edge widths based on total_amount (log scale)
    edge_widths = []
    max_amt = max((data.get(edge_weight, 1) for _, _, data in H.edges(data=True)), default=1)
    for u, v, data in H.edges(data=True):
        amt = data.get(edge_weight, 1)
        # log scale to avoid huge differences
        width = 0.5 + 2 * np.log1p(amt) / np.log1p(max_amt)
        edge_widths.append(width)
    
    # Layout (spring layout works well for moderate sizes)
    pos = nx.spring_layout(H, k=1, iterations=50)
    
    fig, ax = plt.subplots(figsize=(12, 10))
    nx.draw_networkx_nodes(H, pos, node_color=node_colors, node_size=100, alpha=0.8, ax=ax)
    nx.draw_networkx_edges(H, pos, width=edge_widths, alpha=0.5, arrows=True, arrowsize=10, ax=ax)
    nx.draw_networkx_labels(H, pos, font_size=8, ax=ax)
    
    # Add colorbar
    sm = plt.cm.ScalarMappable(cmap='viridis', norm=plt.Normalize(vmin, vmax))
    sm.set_array([])
    fig.colorbar(sm, ax=ax, label=node_attribute)
    ax.set_title(f"Sampled Graph ({len(H.nodes)} nodes, {len(H.edges)} edges)")
    ax.axis('off')
    plt.tight_layout()
    plt.show()

def visualize_sccs(G, num_sccs=4):
    """
    Find Strongly Connected Components and visualize the largest ones in a grid.
    """
    sccs = sorted(nx.strongly_connected_components(G), key=len, reverse=True)
    
    fig, axes = plt.subplots(2, 2, figsize=(15, 12))
    axes = axes.flatten()
    
    for i in range(min(num_sccs, len(sccs))):
        scc_nodes = sccs[i]
        H = G.subgraph(scc_nodes).copy()
        ax = axes[i]
        
        # Layout for this specific component
        pos = nx.spring_layout(H, k=1.5, iterations=50)
        
        # Node coloring based on PageRank if available
        node_colors = [H.nodes[n].get('pagerank', 0) for n in H.nodes()]
        
        nx.draw_networkx_nodes(H, pos, ax=ax, node_size=300, 
                               node_color=node_colors, cmap=plt.cm.coolwarm)
        nx.draw_networkx_edges(H, pos, ax=ax, alpha=0.6, arrows=True)
        nx.draw_networkx_labels(H, pos, ax=ax, font_size=8)
        
        ax.set_title(f"SCC {i+1} (Size: {len(scc_nodes)})")
        ax.axis('off')
        
    plt.tight_layout()
    plt.show()