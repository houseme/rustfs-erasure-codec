// These cases isolate the existing generic scalar path. SIMD dispatch may
// acquire a different number of user-provided views before coding.
#![cfg(not(any(
    feature = "simd-neon",
    feature = "simd-ssse3",
    feature = "simd-avx2",
    feature = "simd-avx512",
    feature = "simd-gfni",
    feature = "simd-vsx",
)))]

use std::cell::Cell;

use rustfs_erasure_codec::galois_8::ReedSolomon;

struct ShrinkingView {
    data: Vec<u8>,
    remaining: Cell<usize>,
}

impl AsRef<[u8]> for ShrinkingView {
    fn as_ref(&self) -> &[u8] {
        let remaining = self.remaining.get();
        self.remaining.set(remaining.saturating_sub(1));
        if remaining == 0 { &[] } else { &self.data }
    }
}

#[test]
fn ordinary_verify_handles_a_view_that_shrinks_after_the_first_matrix_pass() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    for len in [2048, 2049, 4096, 4097] {
        // The fourth data view stays valid for validation and one coding pass.
        // A second pass must not introduce an out-of-bounds read or panic.
        let mut shards: Vec<Vec<u8>> = (0..6)
            .map(|row| (0..len).map(|i| (row * 73 + i * 19) as u8).collect())
            .collect();
        codec.encode(&mut shards).unwrap();
        let views: Vec<_> = shards
            .into_iter()
            .enumerate()
            .map(|(index, data)| ShrinkingView {
                data,
                remaining: Cell::new(if index == 3 { 2 } else { usize::MAX }),
            })
            .collect();
        assert_eq!(codec.verify(&views), Ok(true), "len={len}");
    }
}
