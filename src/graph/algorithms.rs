use crate::types::SherlockGraph;
use std::collections::HashMap;

pub struct TarjanScc;

impl TarjanScc {
    pub fn compute(graph: &mut SherlockGraph) -> usize {
        let n = graph.nodes.len();
        let mut indices = vec![-1i64; n];
        let mut lowlinks = vec![-1i64; n];
        let mut on_stack = vec![false; n];
        let mut stack = Vec::new();
        let mut index_counter = 0i64;
        let mut scc_counter = 0usize;

        // Iterative Tarjan's to avoid stack overflow on large graphs
        struct Frame { u: usize, edge_cursor: usize }
        let mut call_stack: Vec<Frame> = Vec::new();

        for start in 0..n {
            if indices[start] != -1 { continue; }

            call_stack.push(Frame { u: start, edge_cursor: 0 });
            indices[start] = index_counter;
            lowlinks[start] = index_counter;
            index_counter += 1;
            stack.push(start);
            on_stack[start] = true;

            while let Some(frame) = call_stack.last_mut() {
                let u = frame.u;
                let ec = frame.edge_cursor;

                if ec < graph.adj_list[u].len() {
                    let v = graph.adj_list[u][ec].target_idx;
                    frame.edge_cursor += 1;

                    if indices[v] == -1 {
                        // Tree edge – recurse
                        indices[v] = index_counter;
                        lowlinks[v] = index_counter;
                        index_counter += 1;
                        stack.push(v);
                        on_stack[v] = true;
                        call_stack.push(Frame { u: v, edge_cursor: 0 });
                    } else if on_stack[v] {
                        let lv = indices[v];
                        let lu = lowlinks[u];
                        lowlinks[u] = lu.min(lv);
                    }
                } else {
                    // Done with u – propagate lowlink to parent, then maybe pop SCC
                    call_stack.pop();
                    if let Some(parent) = call_stack.last() {
                        let pu = parent.u;
                        let lu = lowlinks[pu];
                        lowlinks[pu] = lu.min(lowlinks[u]);
                    }
                    if lowlinks[u] == indices[u] {
                        loop {
                            let v = stack.pop().unwrap();
                            on_stack[v] = false;
                            graph.nodes[v].scc_id = Some(scc_counter);
                            if v == u { break; }
                        }
                        scc_counter += 1;
                    }
                }
            }
        }
        scc_counter
    }
}

pub fn find_cycles_within_sccs(graph: &SherlockGraph, k: usize) -> Vec<Vec<usize>> {
    let mut all_cycles = Vec::new();
    let n = graph.nodes.len();
    for start_node in 0..n {
        if let Some(scc_id) = graph.nodes[start_node].scc_id {
            let mut path = vec![start_node];
            let mut visited = vec![false; n];
            dfs_cycle_find(
                graph, start_node, start_node, scc_id, k,
                &mut path, &mut visited, &mut all_cycles,
            );
        }
    }
    all_cycles
}

fn dfs_cycle_find(
    graph: &SherlockGraph,
    start_node: usize,
    current_node: usize,
    scc_id: usize,
    k: usize,
    path: &mut Vec<usize>,
    visited: &mut Vec<bool>,
    all_cycles: &mut Vec<Vec<usize>>,
) {
    if path.len() > k { return; }
    visited[current_node] = true;
    for edge in &graph.adj_list[current_node] {
        let next = edge.target_idx;
        if graph.nodes[next].scc_id == Some(scc_id) {
            if next == start_node && path.len() > 1 {
                all_cycles.push(path.clone());
            } else if !visited[next] {
                path.push(next);
                dfs_cycle_find(graph, start_node, next, scc_id, k, path, visited, all_cycles);
                path.pop();
            }
        }
    }
    visited[current_node] = false;
}

// ─────────────────────────────────────────────
// 9. Summary stats
// ─────────────────────────────────────────────

pub fn print_summary(graph: &SherlockGraph, scc_count: usize, cycle_count: usize, scc_ms: u128) {
    let edge_count: usize = graph.adj_list.iter().map(|v| v.len()).sum();
    let non_trivial = {
        let mut freq: HashMap<usize, usize> = HashMap::new();
        for n in &graph.nodes {
            if let Some(id) = n.scc_id { *freq.entry(id).or_insert(0) += 1; }
        }
        freq.values().filter(|&&c| c > 1).count()
    };
    println!("\n┌─ SherlockGraph Summary ──────────────────────────");
    println!("│  nodes          : {}", graph.nodes.len());
    println!("│  edges          : {}", edge_count);
    println!("│  avg out-degree : {:.2}", edge_count as f64 / graph.nodes.len().max(1) as f64);
    println!("│  total SCCs     : {}", scc_count);
    println!("│  non-trivial    : {}  (potential cycles)", non_trivial);
    println!("│  cycles found   : {}", cycle_count);
    println!("│  SCC latency    : {}ms", scc_ms);
    println!("└──────────────────────────────────────────────────");
}
