use rustfs_erasure_codec::Error;
use rustfs_erasure_codec::galois_8::ReedSolomon;

#[test]
fn join_preserves_requested_prefix_and_caps_at_available_elements() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let shards = [&[1, 2, 3][..], &[4, 5, 6], &[7, 8, 9], &[10, 11, 12]];
    let expected: Vec<u8> = (1..=12).collect();
    for requested in [0, 1, 3, 4, 11, 12, 13, usize::MAX] {
        assert_eq!(
            codec.join(&shards, requested).unwrap(),
            expected[..requested.min(expected.len())]
        );
    }
    // Repeated borrowed shards are valid; their logical total need not equal
    // the amount of distinct backing storage.
    let shared = [&[9, 8, 7][..]; 4];
    assert_eq!(codec.join(&shared, 5).unwrap(), [9, 8, 7, 9, 8]);
}

#[test]
fn join_validates_the_complete_layout_even_for_an_empty_requested_prefix() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    assert_eq!(codec.join(&[&[1][..]; 3], 0), Err(Error::TooFewDataShards));
    assert_eq!(codec.join(&[&[1][..]; 5], 0), Err(Error::TooManyDataShards));
    assert_eq!(codec.join(&[&[][..]; 4], 0), Err(Error::EmptyShard));
    assert_eq!(
        codec.join(&[&[1][..], &[2], &[3], &[4, 5]], 0),
        Err(Error::IncorrectShardSize)
    );
}

#[cfg(target_pointer_width = "32")]
#[test]
fn join_small_prefix_when_shared_shards_have_a_total_over_usize_max() {
    // Only 32 MiB of backing storage, shared by 128 valid byte slices. Their
    // logical total is 4 GiB, which exceeds a 32-bit usize. This exercises the
    // real public byte codec without forged slices or a synthetic Field.
    let codec = ReedSolomon::new(128, 1).unwrap();
    let mut storage = vec![0u8; 1 << 25];
    storage[..5].copy_from_slice(&[1, 3, 5, 7, 9]);
    let shards = vec![storage.as_slice(); 128];
    assert_eq!(codec.join(&shards, 5).unwrap(), [1, 3, 5, 7, 9]);
    assert!(codec.join(&shards, 0).unwrap().is_empty());
}
