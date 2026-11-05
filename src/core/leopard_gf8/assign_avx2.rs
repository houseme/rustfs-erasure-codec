/// AVX2 nibble lookup for direct assignment, with the same prebuilt tables.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn mul(
    dst: &mut [u8],
    src: &[u8],
    low: &[u8; 16],
    high: &[u8; 16],
    lut: &[u8; 256],
) {
    use core::arch::x86_64::{
        _mm_loadu_si128, _mm256_and_si256, _mm256_broadcastsi128_si256, _mm256_loadu_si256,
        _mm256_set1_epi8, _mm256_shuffle_epi8, _mm256_srli_epi64, _mm256_storeu_si256,
        _mm256_xor_si256,
    };
    // SAFETY: both lookup tables contain 16 bytes.
    let low_tbl = _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(low.as_ptr().cast()) });
    // SAFETY: the high lookup table contains 16 bytes.
    let high_tbl = _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(high.as_ptr().cast()) });
    let mask = _mm256_set1_epi8(0x0f);
    let (src32, src_tail) = src.as_chunks::<32>();
    let (dst32, dst_tail) = dst.as_chunks_mut::<32>();
    for (source, destination) in src32.iter().zip(dst32.iter_mut()) {
        // SAFETY: these are complete disjoint 32-byte chunks.
        let value = unsafe { _mm256_loadu_si256(source.as_ptr().cast()) };
        let lo = _mm256_and_si256(value, mask);
        let hi = _mm256_and_si256(_mm256_srli_epi64::<4>(value), mask);
        let product = _mm256_xor_si256(
            _mm256_shuffle_epi8(low_tbl, lo),
            _mm256_shuffle_epi8(high_tbl, hi),
        );
        // SAFETY: the destination chunk contains 32 writable bytes.
        unsafe { _mm256_storeu_si256(destination.as_mut_ptr().cast(), product) };
    }
    for (d, s) in dst_tail.iter_mut().zip(src_tail.iter()) {
        *d = lut[*s as usize];
    }
}
