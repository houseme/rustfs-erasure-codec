//! Checked AVX2 and AVX-512BW split-table dot products for bounded Classic coding rows.
//!
//! Adapted from <https://github.com/boylee1111/rustfs-erasure-codec/pull/1>.
//!
//! For serial reconstruction on CPUs using `RustAvx2` or `RustAvx512`, four
//! vector accumulators retain partial sums across the validated survivor views,
//! avoiding intermediate read-modify-write passes over the output buffer.

#[cfg(rse_x86_avx512)]
use core::arch::x86_64::__m128i;
use core::arch::x86_64::_mm_loadu_si128;
#[cfg(rse_x86_avx2)]
use core::arch::x86_64::{
    __m256i, _mm256_and_si256, _mm256_broadcastsi128_si256, _mm256_loadu_si256, _mm256_set1_epi8,
    _mm256_setzero_si256, _mm256_shuffle_epi8, _mm256_srli_epi64, _mm256_storeu_si256,
    _mm256_xor_si256,
};
#[cfg(rse_x86_avx512)]
use core::arch::x86_64::{
    __m512i, _mm512_and_si512, _mm512_broadcast_i32x4, _mm512_loadu_si512, _mm512_set1_epi8,
    _mm512_setzero_si512, _mm512_shuffle_epi8, _mm512_srli_epi64, _mm512_storeu_si512,
    _mm512_xor_si512,
};

use super::super::{BackendId, active_backend_id};
use crate::Error;

type DotFn = unsafe fn(&[u8], &[&[u8]], &mut [u8], usize);

#[cfg(test)]
static EXECUTIONS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Checked split-table multi-input row kernel for `RustAvx2` and `RustAvx512`.
#[derive(Clone, Copy)]
pub(crate) struct SelectedTableRowKernel {
    code: DotFn,
    width: usize,
}

impl SelectedTableRowKernel {
    pub(crate) fn from_backend(backend: BackendId) -> Option<Self> {
        match backend {
            #[cfg(rse_x86_avx2)]
            BackendId::RustAvx2 => Some(Self {
                code: dot_table_avx2,
                width: 32,
            }),
            #[cfg(rse_x86_avx512)]
            BackendId::RustAvx512 => Some(Self {
                code: dot_table_avx512,
                width: 64,
            }),
            _ => None,
        }
    }

    /// Select the active split-table row dot-product kernel for 4..=16 inputs
    /// and 1..=4 outputs when the shard length covers at least one 4-vector batch.
    pub(crate) fn select(inputs: usize, outputs: usize, shard_len: usize) -> Option<Self> {
        if !(4..=16).contains(&inputs) || !(1..=4).contains(&outputs) || shard_len < 128 {
            return None;
        }
        static SELECTED: spin::Once<Option<SelectedTableRowKernel>> = spin::Once::new();
        SELECTED
            .call_once(|| Self::from_backend(active_backend_id()))
            .filter(|kernel| shard_len >= kernel.width * 4)
    }

    /// Validate all views before any output write and execute each chunk.
    pub(crate) fn execute(
        self,
        rows: &[&[u8]],
        inputs: &[&[u8]],
        outputs: &mut [&mut [u8]],
        shard_len: usize,
        chunk_len: usize,
    ) -> Result<(), Error> {
        if inputs.len() < 4 {
            return Err(Error::TooFewDataShards);
        }
        if inputs.len() > 16 {
            return Err(Error::TooManyDataShards);
        }
        if outputs.is_empty() {
            return Err(Error::TooFewParityShards);
        }
        if outputs.len() > 4 {
            return Err(Error::TooManyParityShards);
        }
        if rows.len() != outputs.len() || rows.iter().any(|row| row.len() != inputs.len()) {
            return Err(Error::InvalidCustomMatrix);
        }
        if chunk_len == 0
            || inputs.iter().any(|v| v.len() != shard_len)
            || outputs.iter().any(|v| v.len() != shard_len)
        {
            return Err(Error::IncorrectShardSize);
        }
        #[cfg(test)]
        EXECUTIONS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        for start in (0..shard_len).step_by(chunk_len) {
            let end = start.saturating_add(chunk_len).min(shard_len);
            for (row, output) in rows.iter().zip(outputs.iter_mut()) {
                // SAFETY: selection verified the active AVX2/AVX-512BW ISA;
                // all input/output slice bounds cover `start..end`.
                unsafe { (self.code)(row, inputs, &mut output[start..end], start) };
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct TableTerm {
    input: *const u8,
    low_tbl: *const u8,
    high_tbl: *const u8,
}

fn scalar_tail(row: &[u8], inputs: &[&[u8]], output: &mut [u8], start: usize) {
    let end = start + output.len();
    output.fill(0);
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient == 0 {
            continue;
        }
        for (byte, &inp) in output.iter_mut().zip(&input[start..end]) {
            *byte ^= if coefficient == 1 {
                inp
            } else {
                super::super::mul(coefficient, inp)
            };
        }
    }
}

#[cfg(rse_x86_avx2)]
/// # Safety
/// Caller must ensure `avx2` is supported, `inputs` has 4..=16 slices covering
/// `start..start + output.len()`, and `row.len() == inputs.len()`.
#[target_feature(enable = "avx2")]
unsafe fn dot_table_avx2(row: &[u8], inputs: &[&[u8]], output: &mut [u8], start: usize) {
    let mut terms = [TableTerm {
        input: core::ptr::null(),
        low_tbl: core::ptr::null(),
        high_tbl: core::ptr::null(),
    }; 16];
    let mut general = 0usize;
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient > 1 {
            let (lh, hh) = super::load_table_halves(coefficient);
            terms[general] = TableTerm {
                input: input.as_ptr(),
                low_tbl: lh.as_ptr(),
                high_tbl: hh.as_ptr(),
            };
            general += 1;
        }
    }
    let mut count = general;
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient == 1 {
            terms[count] = TableTerm {
                input: input.as_ptr(),
                low_tbl: core::ptr::null(),
                high_tbl: core::ptr::null(),
            };
            count += 1;
        }
    }
    if count == 0 {
        output.fill(0);
        return;
    }

    let nibble_mask: __m256i = _mm256_set1_epi8(0x0f);
    let width = 32usize;
    let batch = width * 4;
    let batched = output.len() / batch * batch;
    let done = output.len() / width * width;

    for offset in (0..batched).step_by(batch) {
        let at = start + offset;
        let mut a = _mm256_setzero_si256();
        let mut b = _mm256_setzero_si256();
        let mut c = _mm256_setzero_si256();
        let mut d = _mm256_setzero_si256();
        // SAFETY: `offset + 128 <= output.len()` and all inputs cover `start + offset + 128`.
        unsafe {
            for plan in &terms[..general] {
                let low_tbl = _mm256_broadcastsi128_si256(_mm_loadu_si128(plan.low_tbl.cast()));
                let high_tbl = _mm256_broadcastsi128_si256(_mm_loadu_si128(plan.high_tbl.cast()));
                let input = plan.input.add(at);

                let v0 = _mm256_loadu_si256(input.cast());
                let v1 = _mm256_loadu_si256(input.add(width).cast());
                let v2 = _mm256_loadu_si256(input.add(width * 2).cast());
                let v3 = _mm256_loadu_si256(input.add(width * 3).cast());

                a = _mm256_xor_si256(
                    a,
                    _mm256_xor_si256(
                        _mm256_shuffle_epi8(low_tbl, _mm256_and_si256(v0, nibble_mask)),
                        _mm256_shuffle_epi8(
                            high_tbl,
                            _mm256_and_si256(_mm256_srli_epi64::<4>(v0), nibble_mask),
                        ),
                    ),
                );
                b = _mm256_xor_si256(
                    b,
                    _mm256_xor_si256(
                        _mm256_shuffle_epi8(low_tbl, _mm256_and_si256(v1, nibble_mask)),
                        _mm256_shuffle_epi8(
                            high_tbl,
                            _mm256_and_si256(_mm256_srli_epi64::<4>(v1), nibble_mask),
                        ),
                    ),
                );
                c = _mm256_xor_si256(
                    c,
                    _mm256_xor_si256(
                        _mm256_shuffle_epi8(low_tbl, _mm256_and_si256(v2, nibble_mask)),
                        _mm256_shuffle_epi8(
                            high_tbl,
                            _mm256_and_si256(_mm256_srli_epi64::<4>(v2), nibble_mask),
                        ),
                    ),
                );
                d = _mm256_xor_si256(
                    d,
                    _mm256_xor_si256(
                        _mm256_shuffle_epi8(low_tbl, _mm256_and_si256(v3, nibble_mask)),
                        _mm256_shuffle_epi8(
                            high_tbl,
                            _mm256_and_si256(_mm256_srli_epi64::<4>(v3), nibble_mask),
                        ),
                    ),
                );
            }
            for plan in &terms[general..count] {
                let input = plan.input.add(at);
                a = _mm256_xor_si256(a, _mm256_loadu_si256(input.cast()));
                b = _mm256_xor_si256(b, _mm256_loadu_si256(input.add(width).cast()));
                c = _mm256_xor_si256(c, _mm256_loadu_si256(input.add(width * 2).cast()));
                d = _mm256_xor_si256(d, _mm256_loadu_si256(input.add(width * 3).cast()));
            }
            let destination = output.as_mut_ptr().add(offset);
            _mm256_storeu_si256(destination.cast(), a);
            _mm256_storeu_si256(destination.add(width).cast(), b);
            _mm256_storeu_si256(destination.add(width * 2).cast(), c);
            _mm256_storeu_si256(destination.add(width * 3).cast(), d);
        }
    }

    for offset in (batched..done).step_by(width) {
        let at = start + offset;
        let mut sum = _mm256_setzero_si256();
        // SAFETY: `offset + 32 <= output.len()` and all inputs cover `start + offset + 32`.
        unsafe {
            for plan in &terms[..general] {
                let low_tbl = _mm256_broadcastsi128_si256(_mm_loadu_si128(plan.low_tbl.cast()));
                let high_tbl = _mm256_broadcastsi128_si256(_mm_loadu_si128(plan.high_tbl.cast()));
                let v = _mm256_loadu_si256(plan.input.add(at).cast());
                sum = _mm256_xor_si256(
                    sum,
                    _mm256_xor_si256(
                        _mm256_shuffle_epi8(low_tbl, _mm256_and_si256(v, nibble_mask)),
                        _mm256_shuffle_epi8(
                            high_tbl,
                            _mm256_and_si256(_mm256_srli_epi64::<4>(v), nibble_mask),
                        ),
                    ),
                );
            }
            for plan in &terms[general..count] {
                sum = _mm256_xor_si256(sum, _mm256_loadu_si256(plan.input.add(at).cast()));
            }
            _mm256_storeu_si256(output.as_mut_ptr().add(offset).cast(), sum);
        }
    }

    if done < output.len() {
        scalar_tail(row, inputs, &mut output[done..], start + done);
    }
}

#[cfg(rse_x86_avx512)]
/// # Safety
/// Caller must ensure `avx512f` and `avx512bw` are supported, `inputs` has 4..=16 slices
/// covering `start..start + output.len()`, and `row.len() == inputs.len()`.
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn dot_table_avx512(row: &[u8], inputs: &[&[u8]], output: &mut [u8], start: usize) {
    let mut terms = [TableTerm {
        input: core::ptr::null(),
        low_tbl: core::ptr::null(),
        high_tbl: core::ptr::null(),
    }; 16];
    let mut general = 0usize;
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient > 1 {
            let (lh, hh) = super::load_table_halves(coefficient);
            terms[general] = TableTerm {
                input: input.as_ptr(),
                low_tbl: lh.as_ptr(),
                high_tbl: hh.as_ptr(),
            };
            general += 1;
        }
    }
    let mut count = general;
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient == 1 {
            terms[count] = TableTerm {
                input: input.as_ptr(),
                low_tbl: core::ptr::null(),
                high_tbl: core::ptr::null(),
            };
            count += 1;
        }
    }
    if count == 0 {
        output.fill(0);
        return;
    }

    let nibble_mask: __m512i = _mm512_set1_epi8(0x0f);
    let width = 64usize;
    let batch = width * 4;
    let batched = output.len() / batch * batch;
    let done = output.len() / width * width;

    for offset in (0..batched).step_by(batch) {
        let at = start + offset;
        let mut a = _mm512_setzero_si512();
        let mut b = _mm512_setzero_si512();
        let mut c = _mm512_setzero_si512();
        let mut d = _mm512_setzero_si512();
        // SAFETY: `offset + 256 <= output.len()` and all inputs cover `start + offset + 256`.
        unsafe {
            for plan in &terms[..general] {
                let low128: __m128i = _mm_loadu_si128(plan.low_tbl.cast());
                let high128: __m128i = _mm_loadu_si128(plan.high_tbl.cast());
                let low_tbl = _mm512_broadcast_i32x4(low128);
                let high_tbl = _mm512_broadcast_i32x4(high128);
                let input = plan.input.add(at);

                let v0 = _mm512_loadu_si512(input.cast());
                let v1 = _mm512_loadu_si512(input.add(width).cast());
                let v2 = _mm512_loadu_si512(input.add(width * 2).cast());
                let v3 = _mm512_loadu_si512(input.add(width * 3).cast());

                a = _mm512_xor_si512(
                    a,
                    _mm512_xor_si512(
                        _mm512_shuffle_epi8(low_tbl, _mm512_and_si512(v0, nibble_mask)),
                        _mm512_shuffle_epi8(
                            high_tbl,
                            _mm512_and_si512(_mm512_srli_epi64::<4>(v0), nibble_mask),
                        ),
                    ),
                );
                b = _mm512_xor_si512(
                    b,
                    _mm512_xor_si512(
                        _mm512_shuffle_epi8(low_tbl, _mm512_and_si512(v1, nibble_mask)),
                        _mm512_shuffle_epi8(
                            high_tbl,
                            _mm512_and_si512(_mm512_srli_epi64::<4>(v1), nibble_mask),
                        ),
                    ),
                );
                c = _mm512_xor_si512(
                    c,
                    _mm512_xor_si512(
                        _mm512_shuffle_epi8(low_tbl, _mm512_and_si512(v2, nibble_mask)),
                        _mm512_shuffle_epi8(
                            high_tbl,
                            _mm512_and_si512(_mm512_srli_epi64::<4>(v2), nibble_mask),
                        ),
                    ),
                );
                d = _mm512_xor_si512(
                    d,
                    _mm512_xor_si512(
                        _mm512_shuffle_epi8(low_tbl, _mm512_and_si512(v3, nibble_mask)),
                        _mm512_shuffle_epi8(
                            high_tbl,
                            _mm512_and_si512(_mm512_srli_epi64::<4>(v3), nibble_mask),
                        ),
                    ),
                );
            }
            for plan in &terms[general..count] {
                let input = plan.input.add(at);
                a = _mm512_xor_si512(a, _mm512_loadu_si512(input.cast()));
                b = _mm512_xor_si512(b, _mm512_loadu_si512(input.add(width).cast()));
                c = _mm512_xor_si512(c, _mm512_loadu_si512(input.add(width * 2).cast()));
                d = _mm512_xor_si512(d, _mm512_loadu_si512(input.add(width * 3).cast()));
            }
            let destination = output.as_mut_ptr().add(offset);
            _mm512_storeu_si512(destination.cast(), a);
            _mm512_storeu_si512(destination.add(width).cast(), b);
            _mm512_storeu_si512(destination.add(width * 2).cast(), c);
            _mm512_storeu_si512(destination.add(width * 3).cast(), d);
        }
    }

    for offset in (batched..done).step_by(width) {
        let at = start + offset;
        let mut sum = _mm512_setzero_si512();
        // SAFETY: `offset + 64 <= output.len()` and all inputs cover `start + offset + 64`.
        unsafe {
            for plan in &terms[..general] {
                let low128: __m128i = _mm_loadu_si128(plan.low_tbl.cast());
                let high128: __m128i = _mm_loadu_si128(plan.high_tbl.cast());
                let low_tbl = _mm512_broadcast_i32x4(low128);
                let high_tbl = _mm512_broadcast_i32x4(high128);
                let v = _mm512_loadu_si512(plan.input.add(at).cast());
                sum = _mm512_xor_si512(
                    sum,
                    _mm512_xor_si512(
                        _mm512_shuffle_epi8(low_tbl, _mm512_and_si512(v, nibble_mask)),
                        _mm512_shuffle_epi8(
                            high_tbl,
                            _mm512_and_si512(_mm512_srli_epi64::<4>(v), nibble_mask),
                        ),
                    ),
                );
            }
            for plan in &terms[general..count] {
                sum = _mm512_xor_si512(sum, _mm512_loadu_si512(plan.input.add(at).cast()));
            }
            _mm512_storeu_si512(output.as_mut_ptr().add(offset).cast(), sum);
        }
    }

    if done < output.len() {
        scalar_tail(row, inputs, &mut output[done..], start + done);
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::{vec, vec::Vec};

    fn compare(kernel: SelectedTableRowKernel, n: usize, p: usize, len: usize, chunk: usize) {
        let data: Vec<Vec<u8>> = (0..n)
            .map(|j| {
                (0..len + 2)
                    .map(|i| (j * 73 + i * 109 + 17) as u8)
                    .collect()
            })
            .collect();
        let inputs: Vec<&[u8]> = data.iter().map(|shard| &shard[1..len + 1]).collect();
        let rows: Vec<Vec<u8>> = (0..p)
            .map(|j| {
                (0..n)
                    .map(|i| match (i + j) % 4 {
                        0 => 0,
                        1 => 1,
                        _ => (i * 37 + j * 19 + 3) as u8,
                    })
                    .collect()
            })
            .collect();
        let row_refs: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
        let mut output = vec![vec![0xa5; len + 2]; p];
        let mut expected = vec![vec![0u8; len]; p];
        for (row, out) in rows.iter().zip(&mut expected) {
            for index in 0..len {
                out[index] = row.iter().zip(&inputs).fold(0, |sum, (&c, data)| {
                    sum ^ super::super::super::mul(c, data[index])
                });
            }
        }
        let mut views: Vec<&mut [u8]> = output
            .iter_mut()
            .map(|shard| &mut shard[1..len + 1])
            .collect();
        kernel
            .execute(&row_refs, &inputs, &mut views, len, chunk)
            .unwrap();
        for (actual, expected) in output.iter().zip(&expected) {
            assert_eq!(actual[0], 0xa5);
            assert_eq!(actual[len + 1], 0xa5);
            assert_eq!(
                &actual[1..len + 1],
                expected,
                "n={n}, p={p}, len={len}, chunk={chunk}"
            );
        }
    }

    #[test]
    fn reconstruction_selects_available_table_backend() {
        const CHILD: &str = "RSE_PR1_TABLE_BACKEND_CHILD";
        if let Ok(expected) = std::env::var(CHILD) {
            use crate::galois_8::{self, ReedSolomon};
            use core::sync::atomic::Ordering;
            assert_eq!(galois_8::active_backend_name(), expected);
            let codec = ReedSolomon::new(10, 4).unwrap();
            let mut shards: Vec<Vec<u8>> = (0..14)
                .map(|row| (0..1025).map(|i| (row * 73 + i * 19) as u8).collect())
                .collect();
            codec.encode(&mut shards).unwrap();
            let mut lost: Vec<_> = shards.iter().cloned().map(Some).collect();
            lost[3] = None;
            EXECUTIONS.store(0, Ordering::Relaxed);
            codec.reset_runtime_profile_stats();
            codec.reconstruct_data_opt(&mut lost).unwrap();
            assert_eq!(lost[3].as_deref(), Some(shards[3].as_slice()));
            #[cfg(feature = "benchmark-metrics")]
            {
                let metrics = codec.runtime_profile_stats();
                assert_eq!(metrics.code_some_serial_calls, 1);
                assert_eq!(metrics.code_some_total_bytes, 1025 * 10);
                assert_eq!(metrics.code_some_total_chunks, 1);
            }
            let executions = EXECUTIONS.load(Ordering::Relaxed);
            if expected == "scalar-rust" {
                assert_eq!(executions, 0, "scalar override selected a table kernel");
            } else {
                assert!(executions > 0, "table path was not exercised");
            }

            // A same-sized custom field must explicitly expose byte views;
            // field order and representation alone do not opt it into SIMD.
            struct GenericGf8;
            impl crate::Field for GenericGf8 {
                const ORDER: usize = 256;
                type Elem = u8;
                fn add(a: u8, b: u8) -> u8 {
                    galois_8::add(a, b)
                }
                fn mul(a: u8, b: u8) -> u8 {
                    galois_8::mul(a, b)
                }
                fn div(a: u8, b: u8) -> u8 {
                    galois_8::div(a, b)
                }
                fn exp(a: u8, n: usize) -> u8 {
                    galois_8::exp(a, n)
                }
                fn zero() -> u8 {
                    0
                }
                fn one() -> u8 {
                    1
                }
                fn nth_internal(n: usize) -> u8 {
                    n as u8
                }
            }
            let generic = crate::ReedSolomon::<GenericGf8>::new(10, 4).unwrap();
            let mut lost: Vec<_> = shards.iter().cloned().map(Some).collect();
            lost[3] = None;
            EXECUTIONS.store(0, Ordering::Relaxed);
            generic.reconstruct_data(&mut lost).unwrap();
            assert_eq!(lost[3].as_deref(), Some(shards[3].as_slice()));
            assert_eq!(
                EXECUTIONS.load(Ordering::Relaxed),
                0,
                "custom field opted out of byte views"
            );
            return;
        }
        let mut backends = vec!["scalar-rust"];
        #[cfg(rse_x86_avx2)]
        if std::is_x86_feature_detected!("avx2") {
            backends.push("rust-avx2");
        }
        #[cfg(rse_x86_avx512)]
        if std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw") {
            backends.push("rust-avx512");
        }
        for backend in backends {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "galois_8::x86::table_rows::tests::reconstruction_selects_available_table_backend"])
                .env(CHILD, backend)
                .env("RSE_BACKEND_OVERRIDE", backend)
                .status()
                .unwrap();
            assert!(status.success(), "backend={backend}");
        }
    }

    #[test]
    fn malformed_views_are_rejected_before_outputs_are_modified() {
        assert!(SelectedTableRowKernel::select(3, 1, 1024).is_none());
        assert!(SelectedTableRowKernel::select(17, 1, 1024).is_none());
        assert!(SelectedTableRowKernel::select(4, 0, 1024).is_none());
        assert!(SelectedTableRowKernel::select(4, 5, 1024).is_none());
        assert!(SelectedTableRowKernel::select(4, 1, 127).is_none());
        let kernel = if std::is_x86_feature_detected!("avx2") {
            SelectedTableRowKernel::from_backend(BackendId::RustAvx2)
        } else {
            None
        };
        let kernel = kernel.or_else(|| {
            if std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
            {
                SelectedTableRowKernel::from_backend(BackendId::RustAvx512)
            } else {
                None
            }
        });
        let Some(kernel) = kernel else { return };
        let source = [0x42u8; 129];
        let inputs: [&[u8]; 4] = [
            &source[..128],
            &source[..128],
            &source[..128],
            &source[..127],
        ];
        let rows: [&[u8]; 1] = [&[1, 2, 3, 4]];
        let mut output = [0xa5; 128];
        let result = kernel.execute(&rows, &inputs, &mut [&mut output], 128, 128);
        assert_eq!(result, Err(Error::IncorrectShardSize));
        assert_eq!(output, [0xa5; 128]);
        let inputs: [&[u8]; 4] = [&source[..128]; 4];
        let result = kernel.execute(&[&[1, 2, 3]], &inputs, &mut [&mut output], 128, 128);
        assert_eq!(result, Err(Error::InvalidCustomMatrix));
        assert_eq!(output, [0xa5; 128]);
        let result = kernel.execute(&rows, &inputs, &mut [&mut output], 128, 0);
        assert_eq!(result, Err(Error::IncorrectShardSize));
        assert_eq!(output, [0xa5; 128]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn table_rows_match_scalar_for_avx2_and_avx512() {
        let avx2 = std::is_x86_feature_detected!("avx2");
        let avx512 =
            std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw");
        for (supported, backend) in [(avx2, BackendId::RustAvx2), (avx512, BackendId::RustAvx512)] {
            if !supported {
                continue;
            }
            let Some(kernel) = SelectedTableRowKernel::from_backend(backend) else {
                continue;
            };
            for n in [4, 6, 8, 12, 16] {
                for p in [1, 2, 4] {
                    for len in [0, 1, 31, 32, 33, 63, 64, 65, 128, 255, 256, 257, 1025] {
                        compare(kernel, n, p, len, 256);
                    }
                }
            }
        }
    }
}
