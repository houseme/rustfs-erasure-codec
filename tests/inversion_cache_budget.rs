#[cfg(feature = "std")]
use rustfs_erasure_codec::MatrixMode;
use rustfs_erasure_codec::{CodecOptions, Field, ReedSolomon, ShardSlot, galois_8};

fn fixture<F: Field>(codec: &ReedSolomon<F>) -> Vec<ShardSlot<Vec<F::Elem>>> {
    let mut shards: Vec<Vec<_>> = (0..codec.total_shard_count())
        .map(|index| {
            (0..65)
                .map(|offset| F::nth((index * 11 + offset * 7) % F::ORDER))
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards.into_iter().map(ShardSlot::new_present).collect()
}

fn reconstruct_at<F: Field>(
    codec: &ReedSolomon<F>,
    shards: &mut [ShardSlot<Vec<F::Elem>>],
    missing: usize,
) {
    let expected = shards[missing].as_inner().clone();
    shards[missing].mark_missing();
    shards[missing].as_inner_mut().fill(F::zero());
    codec.reconstruct(shards).unwrap();
    assert_eq!(shards[missing].as_inner(), &expected);
    assert!(shards[missing].is_present());
    let memory = codec.reconstruction_cache_memory();
    assert!(memory.retained_bytes <= memory.max_bytes);
    assert!(memory.entries <= codec.inversion_cache_capacity());
}

#[test]
fn default_budget_and_custom_clone_construction_preserve_the_effective_limit() {
    let codec = galois_8::ReedSolomon::new(4, 2).unwrap();
    assert_eq!(codec.inversion_cache_max_bytes(), 8 * 1024 * 1024);
    assert_eq!(codec.reconstruction_cache_memory().retained_bytes, 0);

    let codec = galois_8::ReedSolomon::with_custom_matrix(
        2,
        1,
        &[vec![1, 1]],
        CodecOptions::builder()
            .inversion_cache_max_bytes(4096)
            .build(),
    )
    .unwrap();
    let mut shards = fixture(&codec);
    reconstruct_at(&codec, &mut shards, 0);
    assert_eq!(codec.reconstruction_cache_memory().entries, 1);
    let cloned = codec.clone();
    assert_eq!(cloned.inversion_cache_max_bytes(), 4096);
    assert_eq!(cloned.reconstruction_cache_memory().retained_bytes, 0);
    assert_eq!(cloned.reconstruction_cache_memory().entries, 0);
    reconstruct_at(&cloned, &mut shards, 1);
}

#[test]
fn tiny_budget_keeps_reconstruction_correct_without_retaining_inverse_storage() {
    for enabled in [true, false] {
        let codec = galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder()
                .inversion_cache(enabled)
                .inversion_cache_max_bytes(1)
                .build(),
        )
        .unwrap();
        let mut shards = fixture(&codec);
        for index in [0, 1, 0, 3] {
            reconstruct_at(&codec, &mut shards, index);
            assert_eq!(codec.reconstruction_cache_memory().entries, 0);
            assert_eq!(codec.reconstruction_cache_memory().retained_bytes, 0);
        }
    }
}

fn check_budget<F: Field>() {
    let probe = ReedSolomon::<F>::new(40, 4).unwrap();
    let mut shards = fixture(&probe);
    reconstruct_at(&probe, &mut shards, 0);
    let entry_bytes = probe.reconstruction_cache_memory().retained_bytes;
    // 40x40 exceeds both supported inline matrix capacities.
    assert!(entry_bytes >= 40 * 40 * core::mem::size_of::<F::Elem>());

    let codec = ReedSolomon::<F>::with_options(
        40,
        4,
        CodecOptions::builder()
            .inversion_cache_capacity(100)
            .inversion_cache_max_bytes(entry_bytes)
            .build(),
    )
    .unwrap();
    for index in [0, 1, 2, 0] {
        reconstruct_at(&codec, &mut shards, index);
        assert_eq!(codec.reconstruction_cache_memory().entries, 1);
        assert_eq!(
            codec.reconstruction_cache_memory().retained_bytes,
            entry_bytes
        );
    }

    shards[0].mark_missing();
    let workspace = codec.prepare_reconstruct_workspace(&shards).unwrap();
    codec
        .reconstruct_with_workspace(&mut shards, &workspace)
        .unwrap();
    reconstruct_at(&codec, &mut shards, 1);
    // A workspace owns its matrix independently of later cache eviction.
    shards[0].mark_missing();
    codec
        .reconstruct_with_workspace(&mut shards, &workspace)
        .unwrap();
    assert_eq!(codec.reconstruction_cache_memory().entries, 1);
    assert_eq!(
        codec.reconstruction_cache_memory().retained_bytes,
        entry_bytes
    );
    assert!(
        codec
            .verify(&shards.iter().map(ShardSlot::as_inner).collect::<Vec<_>>())
            .unwrap()
    );
}

#[test]
fn matrix_bytes_and_workspace_lifetimes_are_bounded_for_gf8_and_tower_gf16() {
    check_budget::<galois_8::Field>();
    check_budget::<rustfs_erasure_codec::galois_16::Field>();
}

#[cfg(feature = "std")]
#[test]
fn concurrent_patterns_keep_memory_snapshots_within_budget() {
    let codec = galois_8::ReedSolomon::with_options(
        12,
        4,
        CodecOptions::builder()
            .matrix_mode(MatrixMode::Cauchy)
            .inversion_cache_max_bytes(4096)
            .build(),
    )
    .unwrap();
    let shards = fixture(&codec);
    std::thread::scope(|scope| {
        for offset in 0..4 {
            let codec = &codec;
            let mut shards = shards.clone();
            scope.spawn(move || {
                for index in 0..24 {
                    reconstruct_at(codec, &mut shards, (index + offset) % 12);
                }
            });
        }
    });
    let memory = codec.reconstruction_cache_memory();
    assert!(memory.entries > 0);
    assert!(memory.retained_bytes <= memory.max_bytes);
}
