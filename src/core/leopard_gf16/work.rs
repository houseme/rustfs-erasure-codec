extern crate alloc;

use alloc::alloc::{alloc_zeroed, dealloc, handle_alloc_error};
use core::alloc::Layout;
use smallvec::SmallVec;

const WORK_ALIGNMENT: usize = 64;

/// Flat allocation of u16 work lanes for GF16 Leopard encode/decode.
///
/// Same pattern as the GF8 `FlatWork` but with `u16` elements.
#[derive(Debug)]
pub(crate) struct FlatWork16 {
    lanes: usize,
    lane_len: usize,
    ptr: *mut u16,
    len_elems: usize,
}

// SAFETY: FlatWork16 uniquely owns its heap buffer via a raw *mut u16, so moving it across threads is sound like a Box<[u16]>.
unsafe impl Send for FlatWork16 {}
// SAFETY: shared (&self) access only reads immutable metadata or returns lane slices bound to the borrow, so &FlatWork16 is safe to share across threads.
unsafe impl Sync for FlatWork16 {}

impl FlatWork16 {
    pub(crate) fn new(lanes: usize, lane_len: usize) -> Self {
        let len_elems = lanes
            .checked_mul(lane_len)
            .expect("FlatWork16 element count overflow");
        let len_bytes = len_elems
            .checked_mul(core::mem::size_of::<u16>())
            .expect("FlatWork16 byte count overflow");
        let layout =
            Layout::from_size_align(len_bytes, WORK_ALIGNMENT).expect("FlatWork16 layout overflow");
        // SAFETY: layout is non-zero-sized (len_bytes = lanes*lane_len*2 > 0 for the codec's inputs) with 64-byte alignment; zeroed bytes are a valid u16 bit pattern, and a null return is handled just below.
        let ptr = unsafe { alloc_zeroed(layout) }.cast::<u16>();
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        Self {
            lanes,
            lane_len,
            ptr,
            len_elems,
        }
    }

    pub(crate) fn can_reuse(&self, lanes: usize, lane_len: usize) -> bool {
        self.lanes == lanes && self.lane_len == lane_len
    }

    #[cfg(feature = "std")]
    pub(crate) fn allocated_bytes(&self) -> usize {
        self.len_elems.saturating_mul(core::mem::size_of::<u16>())
    }

    pub(crate) fn lanes(&self) -> usize {
        self.lanes
    }

    pub(crate) fn lane(&self, idx: usize) -> &[u16] {
        debug_assert!(idx < self.lanes);
        let start = idx * self.lane_len;
        // SAFETY: idx < lanes (debug-asserted), so start..start+lane_len lies within the allocation; the &self borrow ties the slice lifetime to the buffer.
        unsafe { core::slice::from_raw_parts(self.ptr.add(start), self.lane_len) }
    }

    pub(crate) fn lane_mut(&mut self, idx: usize) -> &mut [u16] {
        debug_assert!(idx < self.lanes);
        let start = idx * self.lane_len;
        // SAFETY: idx < lanes (debug-asserted), so start..start+lane_len lies within the allocation; the &mut self borrow guarantees the returned slice is unique.
        unsafe { core::slice::from_raw_parts_mut(self.ptr.add(start), self.lane_len) }
    }

    /// Build lane view pointers into the flat buffer.
    pub(crate) fn with_lane_views<R>(
        &mut self,
        lanes: usize,
        size: usize,
        f: impl FnOnce(&mut [&mut [u16]]) -> R,
    ) -> R {
        debug_assert!(size <= self.lane_len);
        debug_assert!(lanes <= self.lanes);

        let mut views: SmallVec<[&mut [u16]; 96]> = (0..lanes)
            .map(|i| {
                let start = i * self.lane_len;
                // SAFETY: i in 0..lanes with lanes <= self.lanes and size <= lane_len, so each start=i*lane_len yields a distinct, non-overlapping, in-bounds range of length size; the collected &mut slices are disjoint.
                unsafe {
                    let ptr = self.ptr.add(start);
                    &mut *core::ptr::slice_from_raw_parts_mut(ptr, size)
                }
            })
            .collect();
        f(&mut views)
    }
}

impl Drop for FlatWork16 {
    fn drop(&mut self) {
        if self.len_elems > 0 {
            let len_bytes = self.len_elems * core::mem::size_of::<u16>();
            let layout = Layout::from_size_align(len_bytes, WORK_ALIGNMENT)
                .expect("FlatWork16 layout overflow");
            // SAFETY: ptr came from alloc_zeroed with this exact layout (same len_bytes and 64-byte align), and len_elems > 0 guards against deallocating a zero-sized allocation.
            unsafe { dealloc(self.ptr.cast::<u8>(), layout) };
        }
    }
}
