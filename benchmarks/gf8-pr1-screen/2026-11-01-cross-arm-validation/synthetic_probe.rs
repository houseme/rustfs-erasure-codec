//! Synthetic gate control; this performs no codec operations or timing.
use std::io::Write as _;

fn main() {
    let program = std::env::args().next().unwrap();
    let value = if program.contains("shift") { 106 } else { 100 };
    for case in [
        "retained_recovery_row/4x2/1048576/missing0",
        "retained_recovery_row/10x4/1048576/missing0",
        "retained_recovery_row/10x4/1048577/missing3",
    ] {
        let stage =
            format!("{{\"samples_ns\":[{value},{value},{value}],\"iterations\":[10,10,10]}}");
        println!(
            "{{\"case_id\":\"{case}\",\"jobs\":4,\"available_parallelism\":5,\"chunk_len\":64,\"A1\":{stage},\"B1\":{stage},\"B2\":{stage},\"A2\":{stage}}}"
        );
        std::io::stdout().flush().unwrap();
        let mut acknowledgement = String::new();
        if std::io::stdin().read_line(&mut acknowledgement).unwrap() == 0
            || acknowledgement.trim() != "continue"
        {
            std::process::exit(2);
        }
    }
}
