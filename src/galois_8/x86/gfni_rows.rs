//! Checked GFNI dot products for bounded Classic coding rows.
use core::arch::x86_64::{
    _mm256_gf2p8affine_epi64_epi8, _mm256_loadu_si256, _mm256_set1_epi64x, _mm256_setzero_si256,
    _mm256_storeu_si256, _mm256_xor_si256, _mm512_gf2p8affine_epi64_epi8, _mm512_loadu_si512,
    _mm512_set1_epi64, _mm512_setzero_si512, _mm512_storeu_si512, _mm512_xor_si512,
};

use super::super::{BackendId, GFNI_MULTIPLY_MATRICES, active_backend_id};
use crate::Error;

type DotFn = unsafe fn(&[u8], &[&[u8]], &mut [u8], usize);

/// The callback is private and selected only from the immutable GFNI backend.
#[derive(Clone, Copy)]
pub(crate) struct SelectedRowKernel {
    code: DotFn,
    width: usize,
}

impl SelectedRowKernel {
    fn from_backend(backend: BackendId) -> Option<Self> {
        match backend {
            BackendId::RustGfniAvx2 => Some(Self {
                code: dot_avx2,
                width: 32,
            }),
            BackendId::RustGfniAvx512 => Some(Self {
                code: dot_avx512,
                width: 64,
            }),
            _ => None,
        }
    }

    /// Amortize preparation across multiple outputs and a full four-vector
    /// batch. Single-output rows retain coefficient streaming: its matrix stays
    /// in a register across the complete input instead of being reloaded per
    /// row tile. This policy also preserves the existing small-input path.
    pub(crate) fn select(inputs: usize, outputs: usize, shard_len: usize) -> Option<Self> {
        if !(4..=32).contains(&inputs) || !(2..=4).contains(&outputs) || shard_len < 128 {
            return None;
        }
        static SELECTED: spin::Once<Option<SelectedRowKernel>> = spin::Once::new();
        SELECTED
            .call_once(|| Self::from_backend(active_backend_id()))
            .filter(|kernel| shard_len >= kernel.width * 4)
    }

    /// Check every view before the first output write. Chunk boundaries remain
    /// those chosen by the existing Classic scheduler.
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
        if inputs.len() > 32 {
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
        for start in (0..shard_len).step_by(chunk_len) {
            let end = start.saturating_add(chunk_len).min(shard_len);
            for (row, output) in rows.iter().zip(outputs.iter_mut()) {
                // SAFETY: selection follows the detected immutable GFNI ISA;
                // all coefficient counts are 4..=32 and every input spans
                // start..end. The exclusive output chunk has exactly that
                // length. No view or dimension can change during execution.
                unsafe { (self.code)(row, inputs, &mut output[start..end], start) };
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Term {
    input: *const u8,
    matrix: i64,
}

const _: () = assert!(core::mem::size_of::<Term>() * 32 == 512);

fn scalar_tail(row: &[u8], inputs: &[&[u8]], output: &mut [u8], start: usize) {
    let end = start + output.len();
    output.fill(0);
    for (&coefficient, input) in row.iter().zip(inputs) {
        if coefficient == 0 {
            continue;
        }
        for (byte, &input) in output.iter_mut().zip(&input[start..end]) {
            *byte ^= if coefficient == 1 {
                input
            } else {
                super::super::mul(coefficient, input)
            };
        }
    }
}

macro_rules! dot_kernel {
    ($name:ident, $features:literal, $width:literal, $zero:ident, $load:ident,
     $broadcast:ident, $affine:ident, $xor:ident, $store:ident) => {
        /// # Safety
        /// The CPU supports the declared ISA. There are 4..=32 equally sized
        /// inputs and the same number of coefficients; every input covers
        /// start..start+output.len(). Output is a valid exclusive byte slice.
        #[target_feature(enable = $features)]
        unsafe fn $name(row: &[u8], inputs: &[&[u8]], output: &mut [u8], start: usize) {
            // Keep a bounded initialized plan: general products first, then
            // unit coefficients. Zero coefficients require no input loads.
            let mut terms = [Term {
                input: core::ptr::null(),
                matrix: 0,
            }; 32];
            let mut general = 0;
            for (&coefficient, input) in row.iter().zip(inputs) {
                if coefficient > 1 {
                    terms[general] = Term {
                        input: input.as_ptr(),
                        matrix: GFNI_MULTIPLY_MATRICES[coefficient as usize],
                    };
                    general += 1;
                }
            }
            let mut count = general;
            for (&coefficient, input) in row.iter().zip(inputs) {
                if coefficient == 1 {
                    terms[count].input = input.as_ptr();
                    count += 1;
                }
            }
            if count == 0 {
                output.fill(0);
                return;
            }
            let (groups, tail) = terms[..general].as_chunks::<4>();
            let (units, unit_tail) = terms[general..count].as_chunks::<4>();
            let done = output.len() / $width * $width;
            // Reuse each matrix broadcast and input pointer for four vectors.
            // The four independent output sums remain in registers throughout
            // the complete row; no intermediate output is loaded or stored.
            let batch = $width * 4;
            let batched = output.len() / batch * batch;
            for offset in (0..batched).step_by(batch) {
                let at = start + offset;
                let mut a = $zero();
                let mut b = $zero();
                let mut c = $zero();
                let mut d = $zero();
                // SAFETY: each batch covers four complete vectors inside the
                // checked output chunk and every input. Every live term keeps
                // its borrowed allocation provenance. The ISA and unaligned
                // access requirements are identical to the single-vector path.
                unsafe {
                    for plan in &terms[..general] {
                        let input = plan.input.add(at);
                        let matrix = $broadcast(plan.matrix);
                        a = $xor(a, $affine::<0>($load(input.cast()), matrix));
                        b = $xor(b, $affine::<0>($load(input.add($width).cast()), matrix));
                        c = $xor(c, $affine::<0>($load(input.add($width * 2).cast()), matrix));
                        d = $xor(d, $affine::<0>($load(input.add($width * 3).cast()), matrix));
                    }
                    for plan in &terms[general..count] {
                        let input = plan.input.add(at);
                        a = $xor(a, $load(input.cast()));
                        b = $xor(b, $load(input.add($width).cast()));
                        c = $xor(c, $load(input.add($width * 2).cast()));
                        d = $xor(d, $load(input.add($width * 3).cast()));
                    }
                    let destination = output.as_mut_ptr().add(offset);
                    $store(destination.cast(), a);
                    $store(destination.add($width).cast(), b);
                    $store(destination.add($width * 2).cast(), c);
                    $store(destination.add($width * 3).cast(), d);
                }
            }
            for offset in (batched..done).step_by($width) {
                let at = start + offset;
                let mut a = $zero();
                let mut b = $zero();
                let mut c = $zero();
                let mut d = $zero();
                // SAFETY: each full vector lies inside the validated input
                // range and output chunk. Every live term retains the input allocation provenance; only
                // initialized term prefixes are read. Matrix words are initialized, and the ISA is
                // established by this function's contract. Unaligned loads and
                // stores impose no stronger caller alignment requirement.
                unsafe {
                    for plan in groups {
                        a = $xor(
                            a,
                            $affine::<0>(
                                $load(plan[0].input.add(at).cast()),
                                $broadcast(plan[0].matrix),
                            ),
                        );
                        b = $xor(
                            b,
                            $affine::<0>(
                                $load(plan[1].input.add(at).cast()),
                                $broadcast(plan[1].matrix),
                            ),
                        );
                        c = $xor(
                            c,
                            $affine::<0>(
                                $load(plan[2].input.add(at).cast()),
                                $broadcast(plan[2].matrix),
                            ),
                        );
                        d = $xor(
                            d,
                            $affine::<0>(
                                $load(plan[3].input.add(at).cast()),
                                $broadcast(plan[3].matrix),
                            ),
                        );
                    }
                    for plan in units {
                        a = $xor(a, $load(plan[0].input.add(at).cast()));
                        b = $xor(b, $load(plan[1].input.add(at).cast()));
                        c = $xor(c, $load(plan[2].input.add(at).cast()));
                        d = $xor(d, $load(plan[3].input.add(at).cast()));
                    }
                    let mut sum = $xor($xor(a, b), $xor(c, d));
                    for plan in tail {
                        sum = $xor(
                            sum,
                            $affine::<0>($load(plan.input.add(at).cast()), $broadcast(plan.matrix)),
                        );
                    }
                    for plan in unit_tail {
                        sum = $xor(sum, $load(plan.input.add(at).cast()));
                    }
                    $store(output.as_mut_ptr().add(offset).cast(), sum);
                }
            }
            if done < output.len() {
                scalar_tail(row, inputs, &mut output[done..], start + done);
            }
        }
    };
}

dot_kernel!(
    dot_avx2,
    "gfni,avx2",
    32,
    _mm256_setzero_si256,
    _mm256_loadu_si256,
    _mm256_set1_epi64x,
    _mm256_gf2p8affine_epi64_epi8,
    _mm256_xor_si256,
    _mm256_storeu_si256
);
dot_kernel!(
    dot_avx512,
    "gfni,avx512f,avx512bw",
    64,
    _mm512_setzero_si512,
    _mm512_loadu_si512,
    _mm512_set1_epi64,
    _mm512_gf2p8affine_epi64_epi8,
    _mm512_xor_si512,
    _mm512_storeu_si512
);

#[cfg(test)]
mod tests;
