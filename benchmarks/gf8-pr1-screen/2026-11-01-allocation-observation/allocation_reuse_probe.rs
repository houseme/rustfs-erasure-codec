//! Non-timing, baseline-only observation. No addresses are emitted.
use std::hint::black_box;
use std::io::Write as _;

const ITERATIONS: usize = 1024;

fn parse_faults(text: &str) -> Option<[u64; 2]> {
    let fields: Vec<_> = text
        .get(text.rfind(") ")? + 2..)?
        .split_whitespace()
        .collect();
    Some([fields.get(7)?.parse().ok()?, fields.get(9)?.parse().ok()?])
}

fn faults() -> [u64; 2] {
    parse_faults(&std::fs::read_to_string("/proc/self/stat").unwrap()).unwrap()
}

#[derive(Default)]
struct AddressCounts {
    previous: Option<usize>,
    repeated: usize,
    changed: usize,
}

impl AddressCounts {
    fn observe(&mut self, address: usize) {
        if let Some(previous) = self.previous {
            if previous == address {
                self.repeated += 1;
            } else {
                self.changed += 1;
            }
        }
        self.previous = Some(address);
    }
}

fn phase(mut recover: impl FnMut() -> (usize, usize), previous: &mut Option<usize>) -> String {
    let before = faults();
    let mut addresses = AddressCounts {
        previous: *previous,
        ..AddressCounts::default()
    };
    let mut min_capacity = usize::MAX;
    let mut max_capacity = 0;
    for _ in 0..ITERATIONS {
        let (address, capacity) = recover();
        addresses.observe(black_box(address));
        min_capacity = min_capacity.min(capacity);
        max_capacity = max_capacity.max(capacity);
    }
    let after = faults();
    *previous = addresses.previous;
    format!(
        "{{\"iterations\":{ITERATIONS},\"same_address\":{},\"different_address\":{},\"capacity_min\":{min_capacity},\"capacity_max\":{max_capacity},\"minor_faults\":{},\"major_faults\":{}}}",
        addresses.repeated,
        addresses.changed,
        after[0].checked_sub(before[0]).unwrap(),
        after[1].checked_sub(before[1]).unwrap(),
    )
}

fn main() {
    assert_eq!(base_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(next_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(rayon::current_num_threads(), 4);
    assert_eq!(std::thread::available_parallelism().unwrap().get(), 5);
    for (data, parity, len, missing) in [
        (4, 2, 1_048_576, 0),
        (10, 4, 1_048_576, 0),
        (10, 4, 1_048_577, 3),
    ] {
        let base = base_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            base_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let next = next_codec::galois_8::ReedSolomon::with_options(
            data,
            parity,
            next_codec::CodecOptions::builder()
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data + parity)
            .map(|row| {
                (0..len)
                    .map(|i| (row * 73 + i * 19 + i / 17) as u8)
                    .collect()
            })
            .collect();
        base.encode(&mut original).unwrap();
        let mut next_encoded = original.clone();
        next.encode(&mut next_encoded).unwrap();
        assert_eq!(next_encoded, original);
        drop(next_encoded);
        let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
        let mut previous = None;
        base_codec::RSE_DIAGNOSTIC_GROUP_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let a1 = phase(
            || {
                shards[missing] = None;
                base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
                let value = shards[missing].as_ref().unwrap();
                (value.as_ptr() as usize, value.capacity())
            },
            &mut previous,
        );
        assert!(
            base_codec::RSE_DIAGNOSTIC_GROUP_CALLS.load(std::sync::atomic::Ordering::Relaxed) > 0
        );
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(got, want)| got.as_deref() == Some(want.as_slice()))
        );
        next_codec::RSE_DIAGNOSTIC_GROUP_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let b1 = phase(
            || {
                shards[missing] = None;
                next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
                let value = shards[missing].as_ref().unwrap();
                (value.as_ptr() as usize, value.capacity())
            },
            &mut previous,
        );
        assert!(
            next_codec::RSE_DIAGNOSTIC_GROUP_CALLS.load(std::sync::atomic::Ordering::Relaxed) > 0
        );
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(got, want)| got.as_deref() == Some(want.as_slice()))
        );
        next_codec::RSE_DIAGNOSTIC_GROUP_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let b2 = phase(
            || {
                shards[missing] = None;
                next.reconstruct_data_opt(black_box(&mut shards)).unwrap();
                let value = shards[missing].as_ref().unwrap();
                (value.as_ptr() as usize, value.capacity())
            },
            &mut previous,
        );
        assert!(
            next_codec::RSE_DIAGNOSTIC_GROUP_CALLS.load(std::sync::atomic::Ordering::Relaxed) > 0
        );
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(got, want)| got.as_deref() == Some(want.as_slice()))
        );
        base_codec::RSE_DIAGNOSTIC_GROUP_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let a2 = phase(
            || {
                shards[missing] = None;
                base.reconstruct_data_opt(black_box(&mut shards)).unwrap();
                let value = shards[missing].as_ref().unwrap();
                (value.as_ptr() as usize, value.capacity())
            },
            &mut previous,
        );
        assert!(
            base_codec::RSE_DIAGNOSTIC_GROUP_CALLS.load(std::sync::atomic::Ordering::Relaxed) > 0
        );
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(got, want)| got.as_deref() == Some(want.as_slice()))
        );
        println!(
            "{{\"case_id\":\"allocation_observation/{data}x{parity}/{len}/missing{missing}\",\"A1\":{a1},\"B1\":{b1},\"B2\":{b2},\"A2\":{a2}}}"
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

#[cfg(test)]
mod tests {
    use super::{AddressCounts, parse_faults};

    #[test]
    fn counts_only_reuse_transitions_without_emitting_addresses() {
        let mut counts = AddressCounts::default();
        for address in [1, 1, 2, 2, 1] {
            counts.observe(address);
        }
        assert_eq!((counts.repeated, counts.changed), (2, 2));
        assert_eq!(counts.previous, Some(1));
    }

    #[test]
    fn parses_faults_with_parenthesized_process_name() {
        assert_eq!(
            parse_faults("123 (name ) x) R 1 2 3 4 5 6 7 8 9 10"),
            Some([7, 9])
        );
        assert_eq!(parse_faults("123 (short) R 1"), None);
    }
}
