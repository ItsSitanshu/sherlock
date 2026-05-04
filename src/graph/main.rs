use sherlock_graph::build_and_analyse_unified;
use std::io;

fn main() -> io::Result<()> {
    let (_graph, _scc_count, cycles) = build_and_analyse_unified("transactions.csv", 6)?;

    for (i, cycle) in cycles.iter().take(10).enumerate() {
        println!("cycle {}: {:?}", i, cycle);
    }

    Ok(())
}
