use sherlock_graph::{build_and_analyse, DataPaths};
use std::io;

fn main() -> io::Result<()> {
    let paths = DataPaths {
        wallet_history:   Some("data/wallet_history.csv"),
        qr_transactions:  Some("data/qr_transactions.csv"),
        service_payments: Some("data/service_payments.csv"),
        remittance:       Some("data/remittance.csv"),
        disbursements:    Some("data/disbursements.csv"),
    };

    let (_graph, _scc_count, cycles) = build_and_analyse(paths, 6)?;

    for (i, cycle) in cycles.iter().take(10).enumerate() {
        println!("cycle {}: {:?}", i, cycle);
    }

    Ok(())
}
