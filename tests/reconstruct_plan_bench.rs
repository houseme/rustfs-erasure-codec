#![cfg(feature = "std")]

#[path = "../benches/reconstruct_plans/workload.rs"]
mod workload;

use cases::expected_error;
use rustfs_erasure_codec::{Error, Field};
use rustfs_erasure_codec::{galois_8, galois_16};
use std::collections::BTreeSet;
use workload::{ACTIONS, Fixture, cases};

#[test]
fn catalog_preserves_all_shape_controls_and_unique_benchmark_keys() {
    let catalog = cases::cases();
    assert_eq!(catalog.len(), 58);
    let mut keys = BTreeSet::new();
    for field in ["gf8", "tower-gf16"] {
        for case in &catalog {
            assert!(case.missing.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(case.missing.iter().all(|i| *i < case.data + case.parity));
            for action in ACTIONS {
                assert!(keys.insert((field, action, case.data, case.parity, case.name)));
            }
        }
    }
    assert_eq!(keys.len(), 348);
    assert_eq!(
        expected_error(catalog.iter().find(|c| c.name == "quorum_short").unwrap()),
        Some(Error::IncorrectShardSize)
    );
    assert_eq!(
        expected_error(catalog.iter().find(|c| c.name == "wide_quorum").unwrap()),
        Some(Error::TooFewShardsPresent)
    );
}

#[test]
fn every_fixture_checks_full_output_and_preserves_no_output_or_error_inputs() {
    fn exercise<F: Field>() {
        for case in cases::cases() {
            let mut fixture = Fixture::<F>::new(case);
            for _ in 0..3 {
                fixture.validate();
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}

#[test]
fn repeated_operations_retain_fixture_buffers_and_warmed_inverse_cache() {
    fn exercise<F: Field>() {
        for case in cases::cases() {
            let mut fixture = Fixture::<F>::new(case);
            fixture.validate();
            let owners = fixture.shards.as_ptr();
            let payloads: Vec<_> = fixture
                .shards
                .iter()
                .map(|s| s.as_inner().as_ptr())
                .collect();
            let before = fixture.codec.reconstruction_cache_stats();
            for _ in 0..3 {
                for _ in 0..7 {
                    assert_eq!(
                        std::hint::black_box(fixture.prepare_full()).err(),
                        expected_error(&fixture.case)
                    );
                    assert_eq!(
                        std::hint::black_box(fixture.prepare_none()).err(),
                        expected_error(&fixture.case)
                    );
                    assert_eq!(
                        fixture.call_none(),
                        expected_error(&fixture.case).map_or(Ok(()), Err)
                    );
                }
            }
            assert_eq!(fixture.shards.as_ptr(), owners);
            assert_eq!(
                fixture
                    .shards
                    .iter()
                    .map(|s| s.as_inner().as_ptr())
                    .collect::<Vec<_>>(),
                payloads
            );
            let after = fixture.codec.reconstruction_cache_stats();
            assert_eq!(after.misses, before.misses);
            if cfg!(feature = "benchmark-metrics") {
                let inverse_needed = expected_error(&fixture.case).is_none()
                    && fixture.case.missing.iter().any(|i| *i < fixture.case.data);
                let queries = if inverse_needed { 21 } else { 0 };
                assert_eq!(after.requests - before.requests, queries);
                assert_eq!(after.hits - before.hits, queries);
            }
            fixture.validate();
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}
