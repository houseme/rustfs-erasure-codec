//! Paired single-CPU verify probe; both versions share the same input buffers.
use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

fn measure(mut op: impl FnMut()) -> ([f64; 3], [u64; 3]) {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_secs(1) {
        op();
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
    (samples, iterations)
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
                "{{\"case_id\":\"small_verify/{data}x{parity}/{len}\",\"backend\":\"{backend}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"chunk_len\":{len},\"jobs\":1,\"available_parallelism\":1}}",
                a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1
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
