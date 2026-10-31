//! Implementation of GF(2^8): the finite field with 2^8 elements.

include!(concat!(env!("OUT_DIR"), "/table.rs"));

// Fixed-coefficient multiplication is an 8x8 linear map over GF(2).
// Derive its GFNI rows from the existing field table, so the polynomial and
// byte representation have a single source of truth. GFNI numbers matrix
// rows from the most-significant byte of each 64-bit word.
#[cfg(any(rse_x86_gfni, all(test, feature = "simd-gfni")))]
const fn gfni_multiply_matrices() -> [i64; 256] {
    let mut matrices = [0i64; 256];
    let mut coefficient = 0;
    while coefficient < 256 {
        let mut matrix = 0u64;
        let mut input_bit = 0;
        while input_bit < 8 {
            let product = MUL_TABLE[coefficient][1 << input_bit];
            let mut output_bit = 0;
            while output_bit < 8 {
                matrix |=
                    (((product >> output_bit) & 1) as u64) << ((7 - output_bit) * 8 + input_bit);
                output_bit += 1;
            }
            input_bit += 1;
        }
        matrices[coefficient] = matrix as i64;
        coefficient += 1;
    }
    matrices
}

#[cfg(any(rse_x86_gfni, all(test, feature = "simd-gfni")))]
static GFNI_MULTIPLY_MATRICES: [i64; 256] = gfni_multiply_matrices();

pub(crate) mod aarch64;
mod aligned;
mod backend;
pub(crate) mod codegen;
mod policy;
mod ppc64;
mod profile;
mod scalar;
pub(crate) mod x86;

pub use crate::{LEOPARD_SHARD_MULTIPLE, leopard_aligned_shard_len};
pub use aligned::{
    AlignedShard, SHARD_ALIGNMENT, alloc_aligned_shards, alloc_shard_slots, mark_missing_slots,
    shards_to_slots,
};
pub use backend::{BackendId, BackendKind};
#[cfg(feature = "std")]
pub use policy::OptionVecReconstructWorkspace;
#[cfg(feature = "std")]
pub(crate) use policy::resolve_runtime_parallel_policy_cache;
#[cfg(feature = "std")]
pub use profile::RustNeonProfileStats;
#[cfg(feature = "std")]
pub use profile::{reset_rust_neon_profile_stats, rust_neon_profile_stats};

/// The field GF(2^8).
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Field;

impl crate::Field for Field {
    const ORDER: usize = 256;
    type Elem = u8;

    fn stable_format() -> Option<(&'static str, usize)> {
        Some(("rustfs.gf8.poly-11d.u8.v1", 1))
    }

    fn write_format_element(element: u8, output: &mut [u8]) -> Option<usize> {
        if output.len() != 1 {
            return None;
        }
        output[0] = element;
        Some(1)
    }

    #[inline]
    fn as_gf8_slice(slice: &[u8]) -> Option<&[u8]> {
        Some(slice)
    }

    #[inline]
    fn as_gf8_slice_mut(slice: &mut [u8]) -> Option<&mut [u8]> {
        Some(slice)
    }

    fn add(a: u8, b: u8) -> u8 {
        add(a, b)
    }

    fn mul(a: u8, b: u8) -> u8 {
        mul(a, b)
    }

    fn div(a: u8, b: u8) -> u8 {
        div(a, b)
    }

    fn exp(elem: u8, n: usize) -> u8 {
        exp(elem, n)
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

    fn mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
        mul_slice(c, input, out)
    }

    fn mul_slice_add(c: u8, input: &[u8], out: &mut [u8]) {
        mul_slice_xor(c, input, out)
    }
}

/// Type alias of ReedSolomon over GF(2^8).
pub type ReedSolomon = crate::ReedSolomon<Field>;

/// Type alias of ShardByShard over GF(2^8).
pub type ShardByShard<'a> = crate::ShardByShard<'a, Field>;

/// Add two elements.
pub fn add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Subtract `b` from `a`.
#[cfg(test)]
pub fn sub(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Multiply two elements.
pub fn mul(a: u8, b: u8) -> u8 {
    MUL_TABLE[a as usize][b as usize]
}

/// Divide one element by another. If `b` is `0`, this returns `0` instead of panicking.
pub fn div(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let log_a = LOG_TABLE[a as usize];
    let log_b = LOG_TABLE[b as usize];
    let mut log_result = log_a as isize - log_b as isize;
    if log_result < 0 {
        log_result += 255;
    }
    EXP_TABLE[log_result as usize]
}

/// Compute a^n for any `usize` exponent, with `0^0 = 1`.
///
/// Nonzero elements have order dividing 255. Reducing the exponent before
/// multiplication bounds the lookup calculation independently of its magnitude.
pub fn exp(a: u8, n: usize) -> u8 {
    if n == 0 {
        1
    } else if a == 0 {
        0
    } else {
        let period = <Field as crate::Field>::ORDER - 1;
        let log_a = LOG_TABLE[a as usize] as usize;
        let log_result = (log_a * (n % period)) % period;
        EXP_TABLE[log_result]
    }
}

/// Multiply each byte in `input` by `c` in GF(2^8), writing results to `out`.
pub fn mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
    (backend::active_backend().mul_slice)(c, input, out);
}

/// XOR-multiply: `out[i] ^= gf_mul(c, input[i])` for each byte.
pub fn mul_slice_xor(c: u8, input: &[u8], out: &mut [u8]) {
    (backend::active_backend().mul_slice_xor)(c, input, out);
}

/// Returns the name of the currently active GF(2^8) backend.
pub fn active_backend_name() -> &'static str {
    backend::active_backend().name
}

/// Returns the kind (Scalar or RustSimd) of the active backend.
pub fn active_backend_kind() -> BackendKind {
    backend::active_backend().kind
}

/// Returns the identifier of the active backend.
pub fn active_backend_id() -> BackendId {
    backend::active_backend().id
}

#[cfg(all(rse_x86_gfni, feature = "std"))]
pub(crate) fn selected_backend_id() -> Option<BackendId> {
    backend::selected_backend_id()
}

pub(crate) fn generated_encode_allowed() -> bool {
    backend::generated_encode_allowed()
}

#[cfg(test)]
fn mul_slice_scalar_for_test(c: u8, input: &[u8], out: &mut [u8]) {
    scalar::mul_slice_pure_rust(c, input, out);
}

#[cfg(test)]
fn mul_slice_xor_scalar_for_test(c: u8, input: &[u8], out: &mut [u8]) {
    scalar::mul_slice_xor_pure_rust(c, input, out);
}

#[cfg(test)]
mod tests;
