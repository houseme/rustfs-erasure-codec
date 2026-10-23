//! 64-byte-aligned shard storage for the Leopard codecs.
//!
//! [`AlignedShard`] backs a shard with a heap allocation whose base address is
//! aligned to [`SHARD_ALIGNMENT`] (64 bytes). This alignment is a **cache and
//! throughput optimisation, not a correctness requirement**: every SIMD kernel
//! in this crate loads and stores with unaligned instructions
//! (`_mm256_loadu_si256` / `_mm512_loadu_si512` / `vld1q_u8` /
//! `core::ptr::read_unaligned`), so shards at any address decode correctly.
//! Aligning to a 64-byte cache line simply avoids split-line accesses.
//!
//! These helpers serve both Leopard families. LeopardGF8 and LeopardGF16 are
//! both built on `ReedSolomon<galois_8::Field>` and operate on byte-oriented
//! shards, so `alloc_aligned` / [`alloc_aligned_shards`] produce correctly
//! aligned buffers for either codec.

extern crate alloc;

use alloc::alloc::{alloc_zeroed, dealloc, handle_alloc_error};
use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::fmt;
use core::iter::FromIterator;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::slice;

use crate::ShardSlot;

/// Byte alignment of every [`AlignedShard`] allocation (one cache line).
///
/// A performance knob, not a safety invariant — see the module docs. Callers
/// that also need the shard **length** to be a multiple of this value (as the
/// Leopard codecs require) should size shards with
/// [`leopard_aligned_shard_len`](crate::galois_8::leopard_aligned_shard_len).
pub const SHARD_ALIGNMENT: usize = 64;

// Preserve the same alignment for empty shards without allocating storage.
#[repr(align(64))]
struct EmptyShardAlignment;

const _: () = assert!(core::mem::align_of::<EmptyShardAlignment>() == SHARD_ALIGNMENT);

/// A shard whose backing allocation is aligned to [`SHARD_ALIGNMENT`].
///
/// The allocation is exactly `len` bytes (see [`AlignedShard::new_zeroed`]) —
/// there is no rounding up and no spare capacity. Alignment is a cache/perf
/// optimisation; correctness never depends on it (see the module docs).
///
/// Because `AlignedShard` implements [`AsRef`], [`AsMut`] and [`FromIterator`]
/// over `u8`, a `Vec<Option<AlignedShard>>` is a valid input to reconstruction:
/// missing (`None`) slots are materialised through the `FromIterator` impl, so
/// recovered shards are 64-byte aligned like the rest.
pub struct AlignedShard {
    ptr: NonNull<u8>,
    len: usize,
}

impl AlignedShard {
    /// Allocates a zero-filled shard of **exactly** `len` bytes, 64-byte aligned.
    ///
    /// The allocation size equals `len` precisely: there is no round-up to the
    /// alignment and no excess capacity. `len == 0` yields an empty,
    /// non-allocating shard backed by a dangling (but aligned) pointer.
    pub fn new_zeroed(len: usize) -> Self {
        if len == 0 {
            return Self {
                ptr: NonNull::<EmptyShardAlignment>::dangling().cast(),
                len: 0,
            };
        }

        let layout = Layout::from_size_align(len, SHARD_ALIGNMENT)
            .expect("aligned shard layout must be valid: len or alignment overflow");
        // SAFETY: `layout` is valid (checked above). `alloc_zeroed` returns a
        // uniquely owned, zero-filled allocation or null on OOM. The returned
        // pointer is valid for `layout.size()` bytes.
        let ptr = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(layout));

        Self { ptr, len }
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let mut shard = Self::new_zeroed(data.len());
        shard.as_mut().copy_from_slice(data);
        shard
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }
}

impl Clone for AlignedShard {
    fn clone(&self) -> Self {
        Self::from_slice(self.as_ref())
    }
}

impl fmt::Debug for AlignedShard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AlignedShard")
            .field("len", &self.len)
            .field("alignment", &SHARD_ALIGNMENT)
            .finish()
    }
}

impl Drop for AlignedShard {
    fn drop(&mut self) {
        if self.len == 0 {
            return;
        }

        let layout = Layout::from_size_align(self.len, SHARD_ALIGNMENT)
            .expect("aligned shard layout must be valid: len or alignment overflow");
        // SAFETY: `self.ptr` was allocated from `alloc_zeroed` with the same
        // layout in `new_zeroed`. This type owns the allocation uniquely
        // (no cloning without explicit Clone impl that creates a new allocation).
        unsafe {
            dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

impl Deref for AlignedShard {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl DerefMut for AlignedShard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl AsRef<[u8]> for AlignedShard {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `self.ptr` points to `self.len` bytes allocated via `alloc_zeroed`
        // with SHARD_ALIGNMENT, or is an aligned dangling pointer when `self.len == 0`
        // (which produces a valid empty slice). The allocation is uniquely owned
        // by this value and outlives the returned reference.
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl AsMut<[u8]> for AlignedShard {
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: same as `as_ref`. `&mut self` guarantees unique mutable access
        // — no other reference to the same memory can exist concurrently.
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl FromIterator<u8> for AlignedShard {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let bytes: Vec<u8> = iter.into_iter().collect();
        Self::from_slice(&bytes)
    }
}

// SAFETY: `AlignedShard` owns a heap allocation of plain `u8` bytes with no
// interior mutability. Moving it across threads transfers ownership and cannot
// create aliased mutable references. The `NonNull<u8>` pointer is not shared
// across threads — it follows the value.
unsafe impl Send for AlignedShard {}
// SAFETY: Shared `&AlignedShard` only exposes immutable `[u8]` slices via
// `as_ref()`. Mutable access requires `&mut self`, which the borrow checker
// ensures is exclusive. No interior mutability or shared mutable state exists.
unsafe impl Sync for AlignedShard {}

/// Allocates `total_shards` zero-filled [`AlignedShard`]s of `shard_len` bytes.
///
/// Each shard is exactly `shard_len` bytes and 64-byte aligned. Suitable for
/// both LeopardGF8 and LeopardGF16, which share the `galois_8::Field` byte
/// layout. To pick a `shard_len` that Leopard will accept, use
/// [`leopard_aligned_shard_len`](crate::galois_8::leopard_aligned_shard_len).
pub fn alloc_aligned_shards(total_shards: usize, shard_len: usize) -> Vec<AlignedShard> {
    (0..total_shards)
        .map(|_| AlignedShard::new_zeroed(shard_len))
        .collect()
}

pub fn alloc_shard_slots(total_shards: usize, shard_len: usize) -> Vec<ShardSlot<Vec<u8>>> {
    (0..total_shards)
        .map(|_| ShardSlot::new_missing(vec![0u8; shard_len]))
        .collect()
}

pub fn shards_to_slots<T: Clone>(shards: &[T]) -> Vec<ShardSlot<T>> {
    shards.iter().cloned().map(ShardSlot::new_present).collect()
}

pub fn mark_missing_slots<T>(slots: &mut [ShardSlot<T>], missing_indices: &[usize]) {
    for &idx in missing_indices {
        if let Some(slot) = slots.get_mut(idx) {
            slot.mark_missing();
        }
    }
}

impl crate::ReedSolomon<super::Field> {
    /// Allocates one 64-byte-aligned shard per shard slot (`data + parity`),
    /// each `shard_len` bytes.
    ///
    /// The buffers work with either Leopard family. Size `shard_len` with
    /// [`leopard_aligned_shard_len`](crate::galois_8::leopard_aligned_shard_len)
    /// so the length is a non-zero multiple of 64 as Leopard requires.
    pub fn alloc_aligned(&self, shard_len: usize) -> Vec<AlignedShard> {
        alloc_aligned_shards(self.total_shard_count(), shard_len)
    }

    pub fn alloc_shard_slots(&self, shard_len: usize) -> Vec<ShardSlot<Vec<u8>>> {
        alloc_shard_slots(self.total_shard_count(), shard_len)
    }
}
