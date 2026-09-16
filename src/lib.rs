//! This crate provides an encoder/decoder for Reed-Solomon erasure code.
//!
//! Please note that erasure coding means errors are not directly detected or corrected,
//! but missing data pieces (shards) can be reconstructed given that
//! the configuration provides high enough redundancy.
//!
//! You will have to implement error detection separately (e.g. via checksums)
//! and simply leave out the corrupted shards when attempting to reconstruct
//! the missing data.
#![allow(dead_code)]
#![cfg_attr(not(feature = "std"), no_std)]
// The PowerPC VSX backend uses `core::arch::powerpc64` intrinsics and a
// `#[target_feature(enable = "vsx")]`, both of which are still unstable in Rust.
// `simd-vsx` therefore requires a nightly toolchain; the feature gates are only
// activated when actually building that backend, so stable builds for every
// other target are unaffected.
#![cfg_attr(
    all(feature = "simd-vsx", target_arch = "powerpc64"),
    feature(stdarch_powerpc, powerpc_target_feature)
)]

#[cfg(test)]
#[macro_use]
extern crate quickcheck;

#[cfg(test)]
extern crate rand;

extern crate smallvec;

use ::core::iter::FromIterator;

#[macro_use]
mod macros;

mod core;
mod errors;
mod matrix;

#[cfg(test)]
mod tests;

pub mod galois_16;
pub mod galois_8;

pub use crate::errors::Error;
pub use crate::errors::SBSError;

pub use crate::core::CodecFamily;
pub use crate::core::CodecOptions;
#[cfg(feature = "std")]
pub use crate::core::LeopardGf8ProfileStats;

pub use crate::core::LeopardMode;
pub use crate::core::MatrixMode;
#[cfg(feature = "std")]
pub use crate::core::PARALLEL_POLICY_VERSION;
#[cfg(feature = "std")]
pub use crate::core::ParallelDecision;
#[cfg(feature = "std")]
pub use crate::core::ParallelPolicy;
#[cfg(feature = "std")]
pub use crate::core::ReconstructionCacheAnalysis;
#[cfg(feature = "std")]
pub use crate::core::ReconstructionCacheStats;
pub use crate::core::ReedSolomon;
#[cfg(feature = "std")]
pub use crate::core::RuntimeProfileStats;
pub use crate::core::ShardByShard;
pub use crate::core::VerifyWorkspace;
#[cfg(feature = "std")]
pub use crate::core::stream;
pub use crate::core::{LEOPARD_SHARD_MULTIPLE, leopard_aligned_shard_len};

#[cfg(feature = "std")]
pub fn leopard_gf8_profile_stats() -> LeopardGf8ProfileStats {
    crate::core::leopard_gf8_profile_stats()
}

#[cfg(feature = "std")]
pub fn reset_leopard_gf8_profile_stats() {
    crate::core::reset_leopard_gf8_profile_stats()
}

// TODO: Can be simplified once https://github.com/rust-lang/rfcs/issues/2505 is resolved
#[cfg(not(feature = "std"))]
use libm::log2f as log2;
#[cfg(feature = "std")]
fn log2(n: f32) -> f32 {
    n.log2()
}

/// A finite field to perform encoding over.
pub trait Field: Sized {
    /// The order of the field. This is a limit on the number of shards
    /// in an encoding.
    const ORDER: usize;

    /// The representational type of the field.
    type Elem: Default + Clone + Copy + PartialEq + ::core::fmt::Debug;

    /// Add two elements together.
    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem;

    /// Multiply two elements together.
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem;

    /// Divide `a` by `b`.
    ///
    /// Field implementations in this crate return zero for division by zero.
    /// Callers that need strict arithmetic should reject a zero divisor before
    /// calling this method.
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem;

    /// Raise `a` to the nth power.
    fn exp(a: Self::Elem, n: usize) -> Self::Elem;

    /// The "zero" element or additive identity.
    fn zero() -> Self::Elem;

    /// The "one" element or multiplicative identity.
    fn one() -> Self::Elem;

    fn nth_internal(n: usize) -> Self::Elem;

    /// Return `Some` when `n < ORDER`, otherwise `None`.
    fn nth_checked(n: usize) -> Option<Self::Elem> {
        if n >= Self::ORDER {
            None
        } else {
            Some(Self::nth_internal(n))
        }
    }

    /// Yield the nth element of the field.
    ///
    /// For out-of-range `n`, returns a wrapping value in production and triggers a
    /// debug assertion.
    /// Assignment is arbitrary but must be unique to `n`.
    fn nth(n: usize) -> Self::Elem {
        Self::nth_checked(n).unwrap_or_else(|| {
            debug_assert!(
                false,
                "Field::nth received out-of-range index: {} for GF(2^{}) order {}",
                n,
                log2(Self::ORDER as f32) as usize,
                Self::ORDER
            );
            let fallback_n = n % Self::ORDER.max(1);
            Self::nth_internal(fallback_n)
        })
    }

    /// Multiply a slice of elements by another. Writes into the output slice.
    ///
    /// # Panics
    /// Panics if the output slice does not have equal length to the input.
    fn mul_slice(elem: Self::Elem, input: &[Self::Elem], out: &mut [Self::Elem]) {
        assert_eq!(input.len(), out.len());

        for (i, o) in input.iter().zip(out) {
            *o = Self::mul(elem, *i)
        }
    }

    /// Multiply a slice of elements by another, adding each result to the corresponding value in
    /// `out`.
    ///
    /// # Panics
    /// Panics if the output slice does not have equal length to the input.
    fn mul_slice_add(elem: Self::Elem, input: &[Self::Elem], out: &mut [Self::Elem]) {
        assert_eq!(input.len(), out.len());

        for (i, o) in input.iter().zip(out) {
            *o = Self::add(*o, Self::mul(elem, *i))
        }
    }
}

pub type ReconstructInitResult<'a, F> =
    Result<&'a mut [<F as Field>::Elem], Result<&'a mut [<F as Field>::Elem], Error>>;

/// A reusable shard container for reconstruction workflows.
///
/// Unlike `Option<Vec<_>>`, this keeps ownership of the backing buffer even when
/// the shard is marked missing, which lets callers reuse preallocated storage
/// across repeated reconstruct calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardSlot<T> {
    data: T,
    present: bool,
}

impl<T> ShardSlot<T> {
    /// Create a shard slot whose data is already present.
    pub fn new_present(data: T) -> Self {
        Self {
            data,
            present: true,
        }
    }

    /// Create a shard slot whose buffer exists but is currently marked missing.
    pub fn new_missing(data: T) -> Self {
        Self {
            data,
            present: false,
        }
    }

    /// Create a shard slot with an explicit presence flag.
    pub fn with_present(data: T, present: bool) -> Self {
        Self { data, present }
    }

    /// Returns whether the shard is currently marked present.
    pub fn is_present(&self) -> bool {
        self.present
    }

    /// Mark the shard as present.
    pub fn mark_present(&mut self) {
        self.present = true;
    }

    /// Mark the shard as missing while retaining ownership of its buffer.
    pub fn mark_missing(&mut self) {
        self.present = false;
    }

    /// Access the inner storage regardless of presence state.
    pub fn as_inner(&self) -> &T {
        &self.data
    }

    /// Mutably access the inner storage regardless of presence state.
    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Consume the slot and return the inner storage.
    pub fn into_inner(self) -> T {
        self.data
    }
}

/// Container that may hold a shard.
///
/// This trait is used in reconstruction, where some of the shards
/// may be unknown.
pub trait ReconstructShard<F: Field> {
    /// The size of the shard data; `None` if empty.
    fn len(&self) -> Option<usize>;

    fn is_empty(&self) -> bool {
        self.len().is_none()
    }

    /// Get a mutable reference to the shard data, returning `None` if uninitialized.
    fn get(&mut self) -> Option<&mut [F::Elem]>;

    /// Get a mutable reference to the shard data, initializing it to the
    /// given length if it was `None`. Returns an error if initialization fails.
    fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, F>;
}

impl<F: Field, T: AsRef<[F::Elem]> + AsMut<[F::Elem]> + FromIterator<F::Elem>> ReconstructShard<F>
    for Option<T>
{
    fn len(&self) -> Option<usize> {
        self.as_ref().map(|x| x.as_ref().len())
    }

    fn get(&mut self) -> Option<&mut [F::Elem]> {
        self.as_mut().map(|x| x.as_mut())
    }

    fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, F> {
        let is_some = self.is_some();
        let x = self
            .get_or_insert_with(|| ::core::iter::repeat_n(F::zero(), len).collect())
            .as_mut();

        if is_some { Ok(x) } else { Err(Ok(x)) }
    }
}

impl<F: Field, T: AsRef<[F::Elem]> + AsMut<[F::Elem]>> ReconstructShard<F> for (T, bool) {
    fn len(&self) -> Option<usize> {
        if !self.1 {
            None
        } else {
            Some(self.0.as_ref().len())
        }
    }

    fn get(&mut self) -> Option<&mut [F::Elem]> {
        if !self.1 { None } else { Some(self.0.as_mut()) }
    }

    fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, F> {
        let x = self.0.as_mut();
        if x.len() == len {
            if self.1 {
                Ok(x)
            } else {
                self.1 = true;
                Err(Ok(x))
            }
        } else {
            Err(Err(Error::IncorrectShardSize))
        }
    }
}

impl<F: Field, T: AsRef<[F::Elem]> + AsMut<[F::Elem]>> ReconstructShard<F> for ShardSlot<T> {
    fn len(&self) -> Option<usize> {
        if self.present {
            Some(self.data.as_ref().len())
        } else {
            None
        }
    }

    fn get(&mut self) -> Option<&mut [F::Elem]> {
        if self.present {
            Some(self.data.as_mut())
        } else {
            None
        }
    }

    fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, F> {
        let x = self.data.as_mut();
        if x.len() == len {
            if self.present {
                Ok(x)
            } else {
                self.present = true;
                Err(Ok(x))
            }
        } else {
            Err(Err(Error::IncorrectShardSize))
        }
    }
}
