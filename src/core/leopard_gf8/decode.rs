extern crate alloc;
use crate::errors::Error;
use alloc::vec;
use alloc::vec::Vec;

use super::ops::{
    fft_dit2, fft_dit4_full_lut_scratch, fwht_variable, fwht8_mtrunc, ifft_dit2,
    ifft_dit4_full_lut_scratch, mulgf8, slice_xor,
};
use super::work::FlatWork;
use super::{
    FftDit8Plan, IfftDit8Plan, LeopardGf8Tables, MODULUS8, WORK_SIZE8, build_fft_dit8_plan,
    build_ifft_decode_dit8_plan, ceil_pow2, init_leopard_gf8_tables,
};

/// Leopard GF8 decode driver — precomputed parameters for reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeopardGf8DecodeDriver {
    pub(crate) data_shards: usize,
    pub(crate) parity_shards: usize,
    pub(crate) shard_size: usize,
    pub(crate) m: usize,
    pub(crate) n: usize,
    pub(crate) work_size: usize,
    pub(crate) chunk_size: usize,
}

pub(crate) fn build_leopard_gf8_decode_driver(
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
) -> Result<LeopardGf8DecodeDriver, Error> {
    if shard_size == 0 || !shard_size.is_multiple_of(64) {
        return Err(Error::IncorrectShardSize);
    }
    let _tables = init_leopard_gf8_tables();

    let m = ceil_pow2(parity_shards.max(1));
    if m > MODULUS8 {
        return Err(Error::TooManyShards);
    }
    let work_size = m.checked_add(data_shards).ok_or(Error::TooManyShards)?;
    if work_size > super::ORDER8 {
        return Err(Error::TooManyShards);
    }
    let n = ceil_pow2(work_size);

    Ok(LeopardGf8DecodeDriver {
        data_shards,
        parity_shards,
        shard_size,
        m,
        n,
        work_size,
        chunk_size: WORK_SIZE8,
    })
}

/// Compute error locator values using FWHT (Fast Walsh-Hadamard Transform).
///
/// The errLocs array uses a specific coordinate system:
/// - Positions 0..parity_shards: parity shard erasures
/// - Positions parity_shards..m: padding (unused parity slots, always 1)
/// - Positions m..m+data_shards: data shard erasures
///
/// Returns an ORDER8-sized array of error locator values in log domain.
#[allow(clippy::needless_range_loop)]
fn compute_error_locs(
    present: &[bool],
    driver: &LeopardGf8DecodeDriver,
    tables: &LeopardGf8Tables,
) -> [u8; super::ORDER8] {
    // Step 1: Initialize errLocs.
    // Parity erasures at 0..p, padding (p..m) always 1, data erasures at m..m+d.
    let mut err_locs = [0u8; super::ORDER8];

    // Parity shard erasures: shard index (data_shards + i) → errLocs position i.
    for i in 0..driver.parity_shards {
        err_locs[i] = u8::from(!present[driver.data_shards + i]);
    }

    // Padding positions (unused parity slots): always 1
    for i in driver.parity_shards..driver.m {
        err_locs[i] = 1;
    }

    // Data shard erasures: shard index i → errLocs position m + i.
    for i in 0..driver.data_shards {
        err_locs[driver.m + i] = u8::from(!present[i]);
    }

    // Step 2: FWHT — outer loop to ORDER8, inner loop limited by mtrunc = work_size.
    // Matches Go's `fwht8(&errLocs, m+r.dataShards)`.
    fwht8_mtrunc(&mut err_locs, driver.work_size);

    // Step 3: Pointwise multiply by log_walsh (integer-mod-255 arithmetic).
    // Store the product directly — do NOT apply exp_lut, because the second
    // FWHT expects integer-mod-255 inputs, not GF(2^8) field values.
    for i in 0..super::ORDER8 {
        if err_locs[i] == 0 {
            continue;
        }
        let product = (err_locs[i] as usize * tables.log_walsh[i] as usize) % MODULUS8;
        err_locs[i] = product as u8;
    }

    // Step 4: Apply second FWHT (full ORDER8 size).
    fwht_variable(&mut err_locs[..super::ORDER8]);

    // err_locs is now in log domain (integer-mod-255 = multiplicative group log).
    // Go uses these values directly as log-domain multipliers — NO log_lut conversion.

    err_locs
}

// Plans depend on the entire erasure pattern, not the requested output subset
// or shard length. Owned keys keep caller buffers out of the thread-local cache.
struct DecodePlan {
    #[cfg(feature = "std")]
    data_shards: usize,
    #[cfg(feature = "std")]
    parity_shards: usize,
    #[cfg(feature = "std")]
    present: [bool; super::ORDER8],
    err_locs: [u8; super::ORDER8],
    ifft: IfftDit8Plan,
    fft: FftDit8Plan,
}

impl DecodePlan {
    fn new(present: &[bool], driver: &LeopardGf8DecodeDriver, tables: &LeopardGf8Tables) -> Self {
        let input_count = present[..driver.data_shards]
            .iter()
            .rposition(|is_present| *is_present)
            .map_or(driver.m, |index| driver.m + index + 1);
        #[cfg(feature = "std")]
        let mut key = [false; super::ORDER8];
        #[cfg(feature = "std")]
        key[..present.len()].copy_from_slice(present);
        Self {
            #[cfg(feature = "std")]
            data_shards: driver.data_shards,
            #[cfg(feature = "std")]
            parity_shards: driver.parity_shards,
            #[cfg(feature = "std")]
            present: key,
            err_locs: compute_error_locs(present, driver, tables),
            ifft: build_ifft_decode_dit8_plan(input_count, driver.n, &tables.fft_skew),
            fft: build_fft_dit8_plan(driver.work_size, driver.n, &tables.fft_skew),
        }
    }

    #[cfg(feature = "std")]
    fn matches(&self, present: &[bool], driver: &LeopardGf8DecodeDriver) -> bool {
        self.data_shards == driver.data_shards
            && self.parity_shards == driver.parity_shards
            && self.present[..present.len()] == *present
    }

    #[cfg(feature = "std")]
    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + (self.ifft.initial_blocks.capacity()
                + self.ifft.later_blocks.capacity()
                + self.fft.stage4_blocks.capacity())
                * core::mem::size_of::<super::Stage4Block>()
            + self.fft.final_stage.capacity() * core::mem::size_of::<super::Stage2Block>()
    }
}

struct DecodeBuffers {
    work: FlatWork,
    scratch: Vec<u8>,
}

impl DecodeBuffers {
    fn new(driver: &LeopardGf8DecodeDriver) -> Self {
        let chunk_cap = driver.shard_size.min(driver.chunk_size);
        Self {
            work: FlatWork::new(driver.n, chunk_cap),
            scratch: vec![0; chunk_cap],
        }
    }

    #[cfg(feature = "std")]
    fn matches(&self, driver: &LeopardGf8DecodeDriver) -> bool {
        self.work
            .can_reuse(driver.n, driver.shard_size.min(driver.chunk_size))
    }

    #[cfg(feature = "std")]
    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.work.lanes() * self.work.lane_len()
            + self.scratch.capacity()
    }
}

// One plan and one initialized working set per thread, bounded independently.
// Oversized calls use local buffers and preserve any smaller cached working set.
#[cfg(feature = "std")]
const MAX_CACHED_DECODE_PLAN_BYTES: usize = 64 * 1024;
#[cfg(feature = "std")]
const MAX_CACHED_DECODE_WORK_BYTES: usize = 4 * 1024 * 1024;

#[cfg(feature = "std")]
#[derive(Default)]
struct DecodeCache {
    plan: Option<DecodePlan>,
    buffers: Option<DecodeBuffers>,
}

#[cfg(feature = "std")]
thread_local! {
    static DECODE_CACHE: std::cell::RefCell<DecodeCache> = const {
        std::cell::RefCell::new(DecodeCache { plan: None, buffers: None })
    };
}

fn with_decode_resources<R>(
    present: &[bool],
    driver: &LeopardGf8DecodeDriver,
    tables: &LeopardGf8Tables,
    run: impl FnOnce(&DecodePlan, &mut DecodeBuffers) -> R,
) -> R {
    #[cfg(feature = "std")]
    {
        DECODE_CACHE.with(|cache| {
            // Reentrant use falls back to local storage without panicking or
            // exposing a second mutable view of cached work buffers.
            let Ok(mut cache) = cache.try_borrow_mut() else {
                return run(
                    &DecodePlan::new(present, driver, tables),
                    &mut DecodeBuffers::new(driver),
                );
            };
            if !cache
                .plan
                .as_ref()
                .is_some_and(|plan| plan.matches(present, driver))
            {
                let plan = DecodePlan::new(present, driver, tables);
                if plan.retained_bytes() > MAX_CACHED_DECODE_PLAN_BYTES {
                    return run(&plan, &mut DecodeBuffers::new(driver));
                }
                cache.plan = Some(plan);
            }
            let DecodeCache { plan, buffers } = &mut *cache;
            let plan = plan.as_ref().expect("decode plan was initialized above");
            if !buffers
                .as_ref()
                .is_some_and(|buffers| buffers.matches(driver))
            {
                let mut fresh = DecodeBuffers::new(driver);
                if fresh.retained_bytes() > MAX_CACHED_DECODE_WORK_BYTES {
                    return run(plan, &mut fresh);
                }
                *buffers = Some(fresh);
            }
            run(
                plan,
                buffers
                    .as_mut()
                    .expect("decode buffers were initialized above"),
            )
        })
    }
    #[cfg(not(feature = "std"))]
    {
        run(
            &DecodePlan::new(present, driver, tables),
            &mut DecodeBuffers::new(driver),
        )
    }
}

/// Perform Leopard GF8 reconstruction using Forney's algorithm.
///
/// Recovery formula: `Original = -ErrLocator * FFT(Derivative(IFFT(ErrLocator * ReceivedData)))`
///
/// # Arguments
/// * `present` - Slice indicating which shards are present (true = present, false = missing).
///   Length must be `data_shards + parity_shards`.
///   Parity shards at indices `data_shards..total`, data at `0..data_shards`.
/// * `outputs` - One output buffer per shard. Empty buffers skip publication for
///   present or unrequested shards; nonempty buffers must match the input length.
/// * `input_data` - Slice of input shard data for present shards. Only present shard
///   data is read; missing shard entries are ignored.
#[allow(clippy::needless_range_loop)]
pub(crate) fn reconstruct_with_tables(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
    tables: &LeopardGf8Tables,
) -> Result<(), Error> {
    let shard_size = super::super::reconstruct::validate_leopard_decode_buffers(
        present,
        outputs,
        input_data,
        data_shards,
        parity_shards,
    )?;
    if present.iter().all(|is_present| *is_present)
        || outputs.iter().all(|output| output.is_empty())
    {
        return Ok(());
    }

    let driver = build_leopard_gf8_decode_driver(data_shards, parity_shards, shard_size)?;

    with_decode_resources(present, &driver, tables, |plan, buffers| {
        reconstruct_with_resources(present, outputs, input_data, &driver, tables, plan, buffers)
    })
}

#[allow(clippy::needless_range_loop)]
fn reconstruct_with_resources(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    driver: &LeopardGf8DecodeDriver,
    tables: &LeopardGf8Tables,
    plan: &DecodePlan,
    buffers: &mut DecodeBuffers,
) -> Result<(), Error> {
    let data_shards = driver.data_shards;
    let parity_shards = driver.parity_shards;
    let total_shards = data_shards + parity_shards;
    let DecodePlan {
        err_locs,
        ifft: ifft_plan,
        fft: fft_plan,
        ..
    } = plan;
    let DecodeBuffers { work, scratch } = buffers;

    // Process in chunks.
    let mut offset = 0usize;
    while offset < driver.shard_size {
        let end = core::cmp::min(offset + driver.chunk_size, driver.shard_size);
        let size = end - offset;

        // Step 1: Multiply received data by error locator values.
        // Parity shards go into work[0..m], data shards go into work[m..m+data_shards].
        for i in 0..data_shards {
            let work_idx = driver.m + i;
            if present[i] {
                let data = input_data[i].as_ref().ok_or(Error::TooFewShardsPresent)?;
                mulgf8(
                    &mut work.lane_mut(work_idx)[..size],
                    &data[offset..end],
                    err_locs[work_idx],
                    tables,
                );
            } else {
                work.lane_mut(work_idx)[..size].fill(0);
            }
        }
        for i in 0..parity_shards {
            let shard_idx = data_shards + i;
            if present[shard_idx] {
                let data = input_data[shard_idx]
                    .as_ref()
                    .ok_or(Error::TooFewShardsPresent)?;
                mulgf8(
                    &mut work.lane_mut(i)[..size],
                    &data[offset..end],
                    err_locs[i],
                    tables,
                );
            } else {
                work.lane_mut(i)[..size].fill(0);
            }
        }
        // Zero padding slots (unused parity positions parity_shards..m).
        for i in parity_shards..driver.m {
            work.lane_mut(i)[..size].fill(0);
        }
        // Zero remaining work slots (beyond work_size).
        for i in driver.work_size..driver.n {
            work.lane_mut(i)[..size].fill(0);
        }

        // Step 2: IFFT on work buffer.
        ifft_dit_decode8_with_plan(
            work,
            size,
            ifft_plan,
            tables,
            driver.n,
            &mut scratch[..size],
        );

        // Step 3: Formal derivative.
        compute_formal_derivative(work, driver.n, size);

        // Step 4: FFT on work buffer.
        fft_dit_decode8_with_plan(work, size, fft_plan, tables, driver.n, &mut scratch[..size]);

        // Step 5: Recover missing shards.
        for i in 0..total_shards {
            if present[i] || outputs[i].is_empty() {
                continue;
            }
            let (work_idx, err_idx) = if i >= data_shards {
                (i - data_shards, i - data_shards)
            } else {
                (driver.m + i, driver.m + i)
            };
            let inv_err = (MODULUS8 as u8).wrapping_sub(err_locs[err_idx]);
            mulgf8(
                &mut outputs[i][offset..end],
                &work.lane(work_idx)[..size],
                inv_err,
                tables,
            );
        }
        offset = end;
    }

    Ok(())
}

/// IFFT for the decode path using `FlatWork`.
fn ifft_dit_decode8_with_plan(
    work: &mut FlatWork,
    size: usize,
    plan: &IfftDit8Plan,
    tables: &LeopardGf8Tables,
    n: usize,
    scratch: &mut [u8],
) {
    if plan.initial_blocks.is_empty() {
        for idx in plan.mtrunc..plan.m {
            work.lane_mut(idx)[..size].fill(0);
        }
    } else {
        for block in &plan.initial_blocks {
            let available = core::cmp::min(plan.mtrunc.saturating_sub(block.r), 4);
            for slot_idx in (block.r + available)..(block.r + 4) {
                if slot_idx < n {
                    work.lane_mut(slot_idx)[..size].fill(0);
                }
            }

            dit4_decode_at(
                TransformDir::Inverse,
                work,
                size,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
                scratch,
            );
        }

        for idx in plan.clear_start..plan.m {
            if idx < n {
                work.lane_mut(idx)[..size].fill(0);
            }
        }

        for block in &plan.later_blocks {
            dit4_decode_at(
                TransformDir::Inverse,
                work,
                size,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
                scratch,
            );
        }
    }

    if let Some(stage) = plan.final_stage {
        for i in 0..stage.dist {
            let a = i;
            let b = i + stage.dist;
            if b < n
                && let Some((ra, rb)) = get_pair_mut_flat(work, a, b, size)
            {
                ifft_dit2(ra, rb, stage.log_m, tables);
            }
        }
    }
}

/// FFT for the decode path using `FlatWork`.
fn fft_dit_decode8_with_plan(
    work: &mut FlatWork,
    size: usize,
    plan: &FftDit8Plan,
    tables: &LeopardGf8Tables,
    n: usize,
    scratch: &mut [u8],
) {
    for block in &plan.stage4_blocks {
        dit4_decode_at(
            TransformDir::Forward,
            work,
            size,
            block.r,
            block.dist,
            block.log_m01,
            block.log_m23,
            block.log_m02,
            tables,
            scratch,
        );
    }

    for stage in &plan.final_stage {
        let a = stage.r;
        let b = stage.r + stage.dist;
        if b < n
            && let Some((ra, rb)) = get_pair_mut_flat(work, a, b, size)
        {
            fft_dit2(ra, rb, stage.log_m, tables);
        }
    }
}

/// Compute the formal derivative of the polynomial represented by work slots.
///
/// For i in 1..n: XOR work[i..i+width] into work[i-width..i]
/// where width = ((i ^ (i-1)) + 1) >> 1.
/// Go: slicesXor(work[i-width:i], work[i:i+width]) → lower ^= higher
fn compute_formal_derivative(work: &mut FlatWork, n: usize, size: usize) {
    for i in 1..n {
        let width = ((i ^ (i - 1)) + 1) >> 1;
        for j in 0..width {
            let dst_idx = i - width + j;
            let src_idx = i + j;
            if src_idx < n
                && dst_idx < n
                && let Some((s, d)) = get_pair_mut_flat(work, src_idx, dst_idx, size)
            {
                slice_xor(s, d);
            }
        }
    }
}

/// Helper to get two mutable lane references from FlatWork.
fn get_pair_mut_flat(
    work: &mut FlatWork,
    i: usize,
    j: usize,
    size: usize,
) -> Option<(&mut [u8], &mut [u8])> {
    if i == j || i >= work.lanes() || j >= work.lanes() {
        return None;
    }
    // SAFETY: i != j, both in bounds, and FlatWork lanes don't overlap.
    unsafe {
        let ptr = work as *mut FlatWork;
        let lane_i = &mut (*ptr).lane_mut(i)[..size];
        let lane_j = &mut (*ptr).lane_mut(j)[..size];
        Some((lane_i, lane_j))
    }
}

use super::ops::TransformDir;

/// DIT-4 butterfly for the decode path using `FlatWork`.
#[allow(clippy::too_many_arguments)]
fn dit4_decode_at(
    dir: TransformDir,
    work: &mut FlatWork,
    size: usize,
    base: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    tables: &LeopardGf8Tables,
    scratch: &mut [u8],
) {
    let mul01 = &tables.mul_luts[log_m01 as usize];
    let mul23 = &tables.mul_luts[log_m23 as usize];
    let mul02 = &tables.mul_luts[log_m02 as usize];

    for i in 0..dist {
        let a = base + i;
        let _b = a + dist;
        let _c = a + dist * 2;
        let d = a + dist * 3;
        if d >= work.lanes() {
            dit4_decode_pairwise_one(dir, work, size, a, dist, log_m01, log_m23, log_m02, tables);
            continue;
        }

        dit4_decode_direct(
            dir, work, size, a, dist, log_m01, log_m23, log_m02, mul01, mul23, mul02, scratch,
        );
    }
}

/// Direct 4-lane butterfly using FlatWork split.
#[allow(clippy::too_many_arguments)]
fn dit4_decode_direct(
    dir: TransformDir,
    work: &mut FlatWork,
    size: usize,
    a: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    mul01: &super::Mul8Lut,
    mul23: &super::Mul8Lut,
    mul02: &super::Mul8Lut,
    scratch: &mut [u8],
) {
    let b = a + dist;
    let c = a + dist * 2;
    let d = a + dist * 3;

    // SAFETY: a, b, c, d are all distinct (a < b < c < d) and < work.lanes().
    let (a_ref, b_ref, c_ref, d_ref) = unsafe {
        let ptr = work as *mut FlatWork;
        let a_ref = &mut (*ptr).lane_mut(a)[..size];
        let b_ref = &mut (*ptr).lane_mut(b)[..size];
        let c_ref = &mut (*ptr).lane_mut(c)[..size];
        let d_ref = &mut (*ptr).lane_mut(d)[..size];
        (a_ref, b_ref, c_ref, d_ref)
    };

    match dir {
        TransformDir::Forward => {
            fft_dit4_full_lut_scratch(
                a_ref,
                b_ref,
                c_ref,
                d_ref,
                log_m01,
                log_m23,
                log_m02,
                &mul01.value,
                &mul01.low,
                &mul01.high,
                &mul23.value,
                &mul23.low,
                &mul23.high,
                &mul02.value,
                &mul02.low,
                &mul02.high,
                scratch,
            );
        }
        TransformDir::Inverse => {
            ifft_dit4_full_lut_scratch(
                a_ref,
                b_ref,
                c_ref,
                d_ref,
                log_m01,
                log_m23,
                log_m02,
                &mul01.value,
                &mul01.low,
                &mul01.high,
                &mul23.value,
                &mul23.low,
                &mul23.high,
                &mul02.value,
                &mul02.low,
                &mul02.high,
                scratch,
            );
        }
    }
}

/// Pairwise fallback for boundary cases in decode path.
#[allow(clippy::too_many_arguments)]
fn dit4_decode_pairwise_one(
    dir: TransformDir,
    work: &mut FlatWork,
    size: usize,
    a: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    tables: &LeopardGf8Tables,
) {
    let b = a + dist;
    let c = a + dist * 2;
    let d = a + dist * 3;
    let has_a = a < work.lanes();
    let has_b = b < work.lanes();
    let has_c = c < work.lanes();
    let has_d = d < work.lanes();
    let available = has_a as usize + has_b as usize + has_c as usize + has_d as usize;
    if available < 2 {
        return;
    }

    match dir {
        TransformDir::Forward => {
            if has_a
                && has_c
                && let Some((r1, r2)) = get_pair_mut_flat(work, a, c, size)
            {
                fft_dit2(r1, r2, log_m02, tables);
            }
            if has_b
                && has_d
                && let Some((r1, r2)) = get_pair_mut_flat(work, b, d, size)
            {
                fft_dit2(r1, r2, log_m02, tables);
            }
            if has_a
                && has_b
                && let Some((r1, r2)) = get_pair_mut_flat(work, a, b, size)
            {
                fft_dit2(r1, r2, log_m01, tables);
            }
            if has_c
                && has_d
                && let Some((r1, r2)) = get_pair_mut_flat(work, c, d, size)
            {
                fft_dit2(r1, r2, log_m23, tables);
            }
        }
        TransformDir::Inverse => {
            if has_a
                && has_b
                && let Some((r1, r2)) = get_pair_mut_flat(work, a, b, size)
            {
                ifft_dit2(r1, r2, log_m01, tables);
            }
            if has_c
                && has_d
                && let Some((r1, r2)) = get_pair_mut_flat(work, c, d, size)
            {
                ifft_dit2(r1, r2, log_m23, tables);
            }
            if has_a
                && has_c
                && let Some((r1, r2)) = get_pair_mut_flat(work, a, c, size)
            {
                ifft_dit2(r1, r2, log_m02, tables);
            }
            if has_b
                && has_d
                && let Some((r1, r2)) = get_pair_mut_flat(work, b, d, size)
            {
                ifft_dit2(r1, r2, log_m02, tables);
            }
        }
    }
}

#[cfg(test)]
mod decode_driver_boundary_tests {
    #[test]
    fn permits_the_full_field_transform_order() {
        let driver =
            super::build_leopard_gf8_decode_driver(super::super::ORDER8 - 4, 4, 64).unwrap();
        assert_eq!(driver.n, super::super::ORDER8);
        assert_eq!(
            super::build_leopard_gf8_decode_driver(super::super::ORDER8 - 3, 4, 64),
            Err(crate::Error::TooManyShards),
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod decode_cache_tests {
    use super::*;

    #[test]
    fn cache_retains_only_bounded_owned_resources() {
        let tables = init_leopard_gf8_tables();
        let mut present = vec![true; 14];
        present[0] = false;
        let small = build_leopard_gf8_decode_driver(10, 4, WORK_SIZE8).unwrap();
        with_decode_resources(&present, &small, tables, |_, _| ());
        DECODE_CACHE.with(|cache| {
            let cache = cache.borrow();
            assert!(cache.plan.as_ref().unwrap().retained_bytes() <= MAX_CACHED_DECODE_PLAN_BYTES);
            assert!(
                cache.buffers.as_ref().unwrap().retained_bytes() <= MAX_CACHED_DECODE_WORK_BYTES
            );
        });

        let mut large_present = vec![true; 256];
        large_present[0] = false;
        let large = build_leopard_gf8_decode_driver(252, 4, WORK_SIZE8).unwrap();
        with_decode_resources(&large_present, &large, tables, |_, buffers| {
            assert!(buffers.retained_bytes() > MAX_CACHED_DECODE_WORK_BYTES);
        });
        DECODE_CACHE.with(|cache| {
            let cache = cache.borrow();
            assert!(cache.plan.as_ref().unwrap().retained_bytes() <= MAX_CACHED_DECODE_PLAN_BYTES);
            assert!(cache.buffers.as_ref().unwrap().matches(&small));
        });
    }

    #[test]
    fn reentrant_resource_use_falls_back_to_separate_initialized_storage() {
        let tables = init_leopard_gf8_tables();
        let mut present = vec![true; 14];
        present[0] = false;
        let driver = build_leopard_gf8_decode_driver(10, 4, 64).unwrap();
        with_decode_resources(&present, &driver, tables, |_, outer| {
            outer.work.lane_mut(0).fill(0x7b);
            let outer_ptr = outer.work.lane(0).as_ptr();
            with_decode_resources(&present, &driver, tables, |_, inner| {
                assert_ne!(inner.work.lane(0).as_ptr(), outer_ptr);
                assert!(inner.work.lane(0).iter().all(|byte| *byte == 0));
            });
            assert!(outer.work.lane(0).iter().all(|byte| *byte == 0x7b));
        });
    }
}
