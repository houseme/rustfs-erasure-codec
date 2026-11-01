//! Paired single-CPU verify probe; both versions share the same input buffers.
use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

// This baseline-only diagnostic records Linux calling-thread counters outside
// the timed samples. It cannot establish a cause for host-level variability.
struct Stage {
    samples: [f64; 3],
    iterations: [u64; 3],
    warmup_iterations: u64,
    sched: [u64; 3],
    faults: [u64; 2],
    wall_ns: u128,
}

fn parse_faults(text: &str) -> Option<[u64; 2]> {
    let body = text.get(text.rfind(") ")? + 2..)?;
    let fields: Vec<_> = body.split_whitespace().collect();
    Some([fields.get(7)?.parse().ok()?, fields.get(9)?.parse().ok()?])
}

fn snapshot() -> ([u64; 3], [u64; 2]) {
    let sched = std::fs::read_to_string("/proc/thread-self/schedstat").unwrap();
    let sched: Vec<u64> = sched
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(sched.len(), 3);
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
    (sched.try_into().unwrap(), parse_faults(&stat).unwrap())
}

fn measure(mut op: impl FnMut()) -> Stage {
    let (before_sched, before_faults) = snapshot();
    let started = Instant::now();
    let warmup = Instant::now();
    let mut warmup_iterations = 0;
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
        warmup_iterations += 1;
    }
    let mut samples = [0.0; 3];
    let mut iterations = [0; 3];
    for (sample, count) in samples.iter_mut().zip(&mut iterations) {
        let start = Instant::now();
        let mut n = 0u64;
        loop {
            for _ in 0..16 {
                op();
            }
            n += 16;
            if start.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        *sample = start.elapsed().as_secs_f64() * 1e9 / n as f64;
        *count = n;
    }
    let wall_ns = started.elapsed().as_nanos();
    let (after_sched, after_faults) = snapshot();
    Stage {
        samples,
        iterations,
        warmup_iterations,
        sched: std::array::from_fn(|i| after_sched[i].checked_sub(before_sched[i]).unwrap()),
        faults: std::array::from_fn(|i| after_faults[i].checked_sub(before_faults[i]).unwrap()),
        wall_ns,
    }
}

fn stage_json(stage: &Stage) -> String {
    format!(
        "{{\"samples_ns\":{:?},\"iterations\":{:?},\"warmup_iterations\":{},\"thread_sched\":{:?},\"faults\":{:?},\"wall_ns\":{}}}",
        stage.samples,
        stage.iterations,
        stage.warmup_iterations,
        stage.sched,
        stage.faults,
        stage.wall_ns
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_linux_faults_even_when_comm_contains_parentheses() {
        assert_eq!(
            super::parse_faults("123 (worker ) name) R 1 2 3 4 5 6 7 8 9 10 11"),
            Some([7, 9])
        );
        assert_eq!(super::parse_faults("123 (truncated) R 1"), None);
        assert_eq!(super::parse_faults("missing parentheses"), None);
    }
}

#[inline(never)]
fn common_verify(codec: &base_codec::galois_8::ReedSolomon, shards: &[Vec<u8>]) -> bool {
    codec.verify(shards).unwrap()
}

fn main() {
    assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
    assert_eq!(rayon::current_num_threads(), 1);
    let backend = base_codec::galois_8::active_backend_name();
    assert_eq!(backend, next_codec::galois_8::active_backend_name());
    for (data, parity) in [(4, 2), (10, 4)] {
        for len in [1024, 2048, 2049, 4096, 4097] {
            let base = base_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
            let next = next_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
            let mut shards: Vec<Vec<u8>> = (0..data + parity)
                .map(|row| {
                    (0..len)
                        .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                        .collect()
                })
                .collect();
            base.encode(&mut shards).unwrap();
            let mut independently_encoded = shards.clone();
            next.encode(&mut independently_encoded).unwrap();
            assert_eq!(independently_encoded, shards);
            let mut corrupt = shards.clone();
            corrupt[data][len - 1] ^= 1;
            assert!(base.verify(&shards).unwrap() && next.verify(&shards).unwrap());
            assert!(!base.verify(&corrupt).unwrap() && !next.verify(&corrupt).unwrap());
            let a1 = measure(|| assert!(common_verify(&base, black_box(&shards))));
            assert!(!base.verify(&corrupt).unwrap());
            let b1 = measure(|| assert!(common_verify(&base, black_box(&shards))));
            assert!(!next.verify(&corrupt).unwrap());
            let b2 = measure(|| assert!(common_verify(&base, black_box(&shards))));
            assert!(!next.verify(&corrupt).unwrap());
            let a2 = measure(|| assert!(common_verify(&base, black_box(&shards))));
            assert!(!base.verify(&corrupt).unwrap());
            println!(
                "{{\"case_id\":\"small_verify/{data}x{parity}/{len}\",\"backend\":\"{backend}\",\"A1\":{},\"B1\":{},\"B2\":{},\"A2\":{},\"chunk_len\":{len},\"jobs\":1,\"available_parallelism\":1}}",
                stage_json(&a1),
                stage_json(&b1),
                stage_json(&b2),
                stage_json(&a2)
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
}
