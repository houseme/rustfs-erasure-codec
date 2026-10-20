#[cfg(test)]
mod dynamic_semantics {
    use rustfs_erasure_codec::__rse_dynamic_buffers as observer;
    use rustfs_erasure_codec::{CodecFamily, CodecOptions, Field, ReedSolomon, ShardSlot};

    fn current_source() -> bool {
        matches!(
            std::env::var("RSE_DYNAMIC_CURRENT_SOURCE").as_deref(),
            Ok("1")
        )
    }

    #[derive(Debug, PartialEq)]
    struct Outcome<E: core::fmt::Debug + PartialEq> {
        result: Result<(), String>,
        present: Vec<bool>,
        bytes: Vec<Vec<E>>,
    }

    fn run<F: Field>(
        codec: &ReedSolomon<F>,
        original: &[Vec<F::Elem>],
        scenario: usize,
        active: bool,
    ) -> Outcome<F::Elem> {
        let mut root = active.then(|| observer::Root::start(__RSE_PAGE_SIZE__));
        if let Some(root) = &mut root {
            root.quiet();
            observer::observation(0);
        }
        let epoch = observer::Epoch::begin();
        let mut shards: Vec<_> = original
            .iter()
            .cloned()
            .map(ShardSlot::new_present)
            .collect();
        let size = original[0].len();
        let data = codec.data_shard_count();
        shards[0] = ShardSlot::new_missing(vec![F::one(); size]);
        if scenario == 3 {
            shards[1] = ShardSlot::new_missing(vec![F::one(); size - 1]);
        } else {
            shards[data].mark_missing();
        }
        if scenario == 2 {
            shards.last_mut().unwrap().as_inner_mut().truncate(size - 1);
        }
        let result = if scenario == 1 {
            let mut required = vec![false; shards.len()];
            required[0] = true;
            codec.reconstruct_some(&mut shards, &required)
        } else {
            codec.reconstruct(&mut shards)
        };
        let result = result.map_err(|error| format!("{error:?}"));
        let present = shards.iter().map(ShardSlot::is_present).collect();
        let bytes = shards.into_iter().map(ShardSlot::into_inner).collect();
        drop(epoch);
        drop(root);
        if active {
            assert_eq!(observer::errors(), 0);
        }
        Outcome {
            result,
            present,
            bytes,
        }
    }

    fn check<F: Field>(codec: ReedSolomon<F>, legacy_leopard: bool) {
        let mut original: Vec<Vec<F::Elem>> = (0..6)
            .map(|shard| {
                (0..1024)
                    .map(|index| F::nth((index * 7 + shard * 11) % F::ORDER))
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        for scenario in 0..4 {
            // Published Leopard does not validate every present length before
            // constructing raw views. Do not execute that unsupported malformed
            // input on the old implementation; the candidate must reject it.
            if scenario == 2 && legacy_leopard && !current_source() {
                continue;
            }
            let plain = run(&codec, &original, scenario, false);
            let observed = run(&codec, &original, scenario, true);
            assert_eq!(
                plain, observed,
                "observer changed API result, publication or bytes"
            );
            match scenario {
                0 => {
                    assert!(observed.result.is_ok());
                    assert_eq!(observed.bytes, original);
                    assert!(observed.present.iter().all(|v| *v));
                }
                1 => {
                    assert!(observed.result.is_ok());
                    assert!(observed.present[0]);
                    if __RSE_CURRENT_SOURCE__ {
                        assert!(!observed.present[4]);
                    }
                    assert_eq!(observed.bytes[0], original[0]);
                }
                _ => {
                    if __RSE_CURRENT_SOURCE__ {
                        assert!(observed.result.is_err());
                    }
                }
            }
            if scenario == 3 && __RSE_CURRENT_SOURCE__ && observed.present[0] {
                assert_eq!(
                    observed.bytes[0], original[0],
                    "late failure published undecoded bytes"
                );
            }
        }
    }

    #[test]
    fn active_observer_preserves_normal_selective_invalid_and_late_failure_semantics() {
        for family in [
            CodecFamily::Classic,
            CodecFamily::LeopardGF8,
            CodecFamily::LeopardGF16,
        ] {
            check(
                ReedSolomon::<rustfs_erasure_codec::galois_8::Field>::with_options(
                    4,
                    2,
                    CodecOptions::builder().codec_family(family).build(),
                )
                .unwrap(),
                family != CodecFamily::Classic,
            );
        }
        check(
            ReedSolomon::<rustfs_erasure_codec::galois_16::Field>::new(4, 2).unwrap(),
            false,
        );
    }
}
