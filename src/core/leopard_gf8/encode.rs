extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use crate::core::leopard::leopard_work_cache_fits;
use crate::errors::Error;

#[cfg(feature = "std")]
use std::sync::atomic::Ordering;

use super::ops::{
    fft_dit2, fft_dit4_full_lut_scratch, get_pair_mut, ifft_dit2, ifft_dit4_full_lut_scratch,
    slice_xor,
};
use super::work::FlatWork;

// Thread-local FlatWork cache to avoid repeated large heap allocations.
// Reuses the buffer when the encode configuration (lanes × lane_len) matches.
#[cfg(feature = "std")]
thread_local! {
    static FLAT_WORK_CACHE: std::cell::RefCell<Option<FlatWork>> =
        const { std::cell::RefCell::new(None) };
}

// Thread-local scratch buffer reused by allocation-free FFT butterflies.
// Avoids per-butterfly `to_vec()` heap allocations by reusing a single buffer
// across all radix-4 butterfly operations within an encode call.
#[cfg(feature = "std")]
thread_local! {
    static SCRATCH_CACHE: std::cell::RefCell<Option<Vec<u8>>> =
        const { std::cell::RefCell::new(None) };
}
#[cfg(feature = "std")]
use super::PROFILE8;
use super::{
    FftDit8Plan, IfftDit8Plan, IfftProfilePhase, LeopardGf8EncodeDriver, LeopardGf8Tables,
    MODULUS8, build_fft_dit8_plan, build_ifft_dit8_plan, build_leopard_gf8_encode_driver,
    init_leopard_gf8_tables,
};

/// DIT-4 butterfly implementation strategy.
///
/// Selected via `RSE_DIT4_STRATEGY` env var (default: `auto`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dit4Strategy {
    /// Safe pairwise decomposition: 4x fft_dit2 per radix-4 group.
    Decomposed,
    /// Direct 4-lane butterfly with a raw-pointer fast path and safe boundary fallback.
    Direct,
    /// Direct 4-lane butterfly using safe split_at_mut access.
    DirectSafe,
    /// Auto-select based on shard size: < 64 KiB uses Decomposed, otherwise Direct.
    Auto,
}

/// Resolve the process-wide DIT4 strategy override when `std` is available.
///
/// `no_std` builds always use `Auto`.
fn configured_dit4_mode() -> Dit4Strategy {
    #[cfg(feature = "std")]
    {
        static MODE: std::sync::OnceLock<Dit4Strategy> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("RSE_DIT4_STRATEGY")
                .ok()
                .and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
                    "decomposed" => Some(Dit4Strategy::Decomposed),
                    "direct" => Some(Dit4Strategy::Direct),
                    "direct-safe" => Some(Dit4Strategy::DirectSafe),
                    "auto" => Some(Dit4Strategy::Auto),
                    _ => None,
                })
                .unwrap_or(Dit4Strategy::Auto)
        })
    }
    #[cfg(not(feature = "std"))]
    Dit4Strategy::Auto
}

/// Resolve the requested DIT4 strategy for this shard size.
///
/// For `Auto` mode, shard sizes below 64 KiB use `Decomposed` for small-input
/// cache locality; larger shards use `Direct` to minimize per-byte passes. For
/// explicit modes, this returns the user's choice regardless of shard size.
fn active_dit4_strategy(shard_size: usize) -> Dit4Strategy {
    match configured_dit4_mode() {
        Dit4Strategy::Auto => {
            if shard_size < 64 * 1024 {
                Dit4Strategy::Decomposed
            } else {
                Dit4Strategy::Direct
            }
        }
        other => other,
    }
}

// Keep only one immutable schedule per thread. Large configurations are built
// locally instead of retaining unbounded plan memory in long-lived worker threads.
#[cfg(feature = "std")]
const MAX_CACHED_PLAN_BYTES: usize = 1024 * 1024;

struct EncodePlans {
    #[cfg(feature = "std")]
    data_shards: usize,
    #[cfg(feature = "std")]
    parity_shards: usize,
    first_ifft_plan: IfftDit8Plan,
    fft_plan: FftDit8Plan,
    later_ifft_plans: Vec<IfftDit8Plan>,
    remainder_ifft_plan: Option<IfftDit8Plan>,
}

impl EncodePlans {
    fn new(
        data_shards: usize,
        parity_shards: usize,
        driver: &LeopardGf8EncodeDriver,
        tables: &LeopardGf8Tables,
    ) -> Self {
        let skew = &tables.fft_skew[driver.skew_offset..];
        let first_ifft_plan = build_ifft_dit8_plan(driver.mtrunc, driver.m, skew);
        let fft_plan = build_fft_dit8_plan(parity_shards, driver.m, &tables.fft_skew);
        let mut later_ifft_plans = Vec::new();
        let mut remainder_ifft_plan = None;
        if driver.m < data_shards {
            let mut group_offset = driver.m;
            let mut skew_offset = driver.m;
            while group_offset + driver.m <= data_shards {
                later_ifft_plans.push(build_ifft_dit8_plan(
                    driver.m,
                    driver.m,
                    &skew[skew_offset..],
                ));
                group_offset += driver.m;
                skew_offset += driver.m;
            }
            if driver.last_count != 0 {
                remainder_ifft_plan = Some(build_ifft_dit8_plan(
                    driver.last_count,
                    driver.m,
                    &skew[skew_offset..],
                ));
            }
        }
        Self {
            #[cfg(feature = "std")]
            data_shards,
            #[cfg(feature = "std")]
            parity_shards,
            first_ifft_plan,
            fft_plan,
            later_ifft_plans,
            remainder_ifft_plan,
        }
    }

    #[cfg(feature = "std")]
    fn retained_bytes(&self) -> usize {
        fn ifft_bytes(plan: &IfftDit8Plan) -> usize {
            (plan.initial_blocks.capacity() + plan.later_blocks.capacity())
                * core::mem::size_of::<super::Stage4Block>()
        }
        self.fft_plan.stage4_blocks.capacity() * core::mem::size_of::<super::Stage4Block>()
            + self.fft_plan.final_stage.capacity() * core::mem::size_of::<super::Stage2Block>()
            + ifft_bytes(&self.first_ifft_plan)
            + self.later_ifft_plans.capacity() * core::mem::size_of::<IfftDit8Plan>()
            + self.later_ifft_plans.iter().map(ifft_bytes).sum::<usize>()
            + self.remainder_ifft_plan.as_ref().map_or(0, ifft_bytes)
    }
}

#[cfg(feature = "std")]
thread_local! {
    static ENCODE_PLAN_CACHE: std::cell::RefCell<Option<EncodePlans>> =
        const { std::cell::RefCell::new(None) };
}

// Reserve no memory for caching: account for the work allocation and the
// scratch Vec's actual capacity after computation, then keep or drop both.
#[cfg(feature = "std")]
fn cache_encode_buffers(work: FlatWork, scratch: Vec<u8>) {
    let scratch_bytes = scratch
        .capacity()
        .saturating_mul(core::mem::size_of::<u8>());
    if leopard_work_cache_fits(work.allocated_bytes(), scratch_bytes) {
        FLAT_WORK_CACHE.with(|cache| *cache.borrow_mut() = Some(work));
        SCRATCH_CACHE.with(|cache| *cache.borrow_mut() = Some(scratch));
    }
}

pub(super) fn encode_skeleton<T: AsRef<[u8]>, U: AsRef<[u8]> + AsMut<[u8]>>(
    data_shards: usize,
    parity_shards: usize,
    data: &[T],
    parity: &mut [U],
) -> Result<LeopardGf8EncodeDriver, Error> {
    if data.len() != data_shards || parity.len() != parity_shards {
        return Err(Error::TooFewShards);
    }

    let shard_size = data
        .first()
        .map(|shard| shard.as_ref().len())
        .ok_or(Error::TooFewShards)?;
    build_leopard_gf8_encode_driver(data_shards, parity_shards, shard_size)
}

pub(super) fn encode_with_tables<T: AsRef<[u8]>, U: AsRef<[u8]> + AsMut<[u8]>>(
    data_shards: usize,
    parity_shards: usize,
    data: &[T],
    parity: &mut [U],
) -> Result<LeopardGf8EncodeDriver, Error> {
    let tables = init_leopard_gf8_tables();
    if data.len() != data_shards || parity.len() != parity_shards {
        return Err(Error::TooFewShards);
    }
    #[cfg(feature = "std")]
    PROFILE8.encode_calls.fetch_add(1, Ordering::Relaxed);
    let shard_size = data
        .first()
        .map(|shard| shard.as_ref().len())
        .ok_or(Error::TooFewShards)?;
    let driver = build_leopard_gf8_encode_driver(data_shards, parity_shards, shard_size)?;
    let chunk_cap = core::cmp::min(driver.shard_size, driver.chunk_size);
    let needed_lanes = driver.work_slices;
    let needed_lane_len = chunk_cap;

    // An oversized call uses only local resources, preserving the normal warm
    // configuration (including its immutable plan) in this thread's caches.
    #[cfg(feature = "std")]
    let cacheable = leopard_work_cache_fits(
        needed_lanes
            .saturating_mul(needed_lane_len)
            .saturating_mul(core::mem::size_of::<u8>()),
        needed_lane_len.saturating_mul(core::mem::size_of::<u8>()),
    );

    #[cfg(feature = "std")]
    let plans = if cacheable {
        ENCODE_PLAN_CACHE.with(|cache| {
            cache
                .take()
                .filter(|plan| {
                    plan.data_shards == data_shards && plan.parity_shards == parity_shards
                })
                .unwrap_or_else(|| EncodePlans::new(data_shards, parity_shards, &driver, tables))
        })
    } else {
        EncodePlans::new(data_shards, parity_shards, &driver, tables)
    };
    #[cfg(not(feature = "std"))]
    let plans = EncodePlans::new(data_shards, parity_shards, &driver, tables);

    // Try to reuse cached FlatWork to avoid repeated large heap allocations.
    #[cfg(feature = "std")]
    let cached_work = if cacheable {
        FLAT_WORK_CACHE.with(|cache| cache.take())
    } else {
        None
    };
    #[cfg(feature = "std")]
    let mut flat_work = cached_work
        .filter(|work| work.can_reuse(needed_lanes, needed_lane_len))
        .unwrap_or_else(|| {
            // SAFETY: the encode path initializes every active lane before reading.
            unsafe { FlatWork::new_uninit(needed_lanes, needed_lane_len) }
        });
    #[cfg(not(feature = "std"))]
    let mut flat_work = FlatWork::new(needed_lanes, needed_lane_len);

    // Pre-allocate scratch buffer for allocation-free FFT butterflies.
    // Reused across all chunks and encode calls (via thread-local on std).
    // The scratch avoids per-butterfly `to_vec()` heap allocations.
    #[cfg(feature = "std")]
    let mut scratch = if cacheable {
        SCRATCH_CACHE.with(|cache| cache.take().unwrap_or_default())
    } else {
        Vec::new()
    };
    #[cfg(not(feature = "std"))]
    let mut scratch: Vec<u8> = Vec::new();

    let work_size = core::cmp::min(driver.m * 2, flat_work.lanes());
    // Reuse the lane table across chunks without retaining references in TLS.
    flat_work.with_lane_views(work_size, chunk_cap, |work| {
        let mut offset = 0usize;
        while offset < driver.shard_size {
            #[cfg(feature = "std")]
            PROFILE8.encode_chunks.fetch_add(1, Ordering::Relaxed);
            let end = core::cmp::min(offset + driver.chunk_size, driver.shard_size);
            let size = end - offset;

            // Ensure scratch is large enough for this chunk.
            if scratch.len() < size {
                scratch.resize(size, 0);
            }

            if size < chunk_cap {
                debug_assert_eq!(end, driver.shard_size);
                super::super::leopard::truncate_lane_views(work, size);
            }

            #[cfg(feature = "std")]
            if plans.first_ifft_plan.mtrunc == plans.first_ifft_plan.m {
                PROFILE8.encode_full_groups.fetch_add(1, Ordering::Relaxed);
            }
            ifft_dit_encoder8_with_plan(
                data,
                &plans.first_ifft_plan,
                &mut work[..driver.m],
                None,
                offset,
                end,
                tables,
                IfftProfilePhase::FirstGroup,
                driver.shard_size,
                &mut scratch,
            );

            let mut group_offset = driver.m;
            for plan in &plans.later_ifft_plans {
                #[cfg(feature = "std")]
                {
                    PROFILE8
                        .encode_later_group_calls
                        .fetch_add(1, Ordering::Relaxed);
                    PROFILE8.encode_full_groups.fetch_add(1, Ordering::Relaxed);
                }
                let (xor_dst, temp_work) = work[..work_size].split_at_mut(driver.m);
                ifft_dit_encoder8_with_plan(
                    &data[group_offset..],
                    plan,
                    temp_work,
                    Some(xor_dst),
                    offset,
                    end,
                    tables,
                    IfftProfilePhase::LaterGroup,
                    driver.shard_size,
                    &mut scratch,
                );
                group_offset += driver.m;
            }

            if let Some(plan) = plans.remainder_ifft_plan.as_ref() {
                #[cfg(feature = "std")]
                PROFILE8
                    .encode_remainder_groups
                    .fetch_add(1, Ordering::Relaxed);
                let (xor_dst, temp_work) = work[..work_size].split_at_mut(driver.m);
                ifft_dit_encoder8_with_plan(
                    &data[group_offset..],
                    plan,
                    temp_work,
                    Some(xor_dst),
                    offset,
                    end,
                    tables,
                    IfftProfilePhase::RemainderGroup,
                    driver.shard_size,
                    &mut scratch,
                );
            }

            fft_dit8_with_plan(
                &mut work[..driver.m],
                &plans.fft_plan,
                tables,
                driver.shard_size,
                &mut scratch,
            );

            #[cfg(feature = "std")]
            PROFILE8.add_output_writeback(parity.len() * size);
            for (idx, output) in parity.iter_mut().enumerate() {
                output.as_mut()[offset..end].copy_from_slice(&work[idx][..size]);
            }
            offset = end;
        }
    });

    #[cfg(feature = "std")]
    if cacheable {
        cache_encode_buffers(flat_work, scratch);
    }

    #[cfg(feature = "std")]
    if cacheable && plans.retained_bytes() <= MAX_CACHED_PLAN_BYTES {
        ENCODE_PLAN_CACHE.with(|cache| {
            *cache.borrow_mut() = Some(plans);
        });
    }

    Ok(driver)
}

use super::ops::TransformDir;

#[allow(clippy::too_many_arguments)]
fn dit4_at<W: AsMut<[u8]>>(
    dir: TransformDir,
    work: &mut [W],
    base: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    tables: &LeopardGf8Tables,
    shard_size: usize,
    scratch: &mut [u8],
) {
    match active_dit4_strategy(shard_size) {
        Dit4Strategy::Decomposed => {
            dit4_at_decomposed(dir, work, base, dist, log_m01, log_m23, log_m02, tables);
        }
        Dit4Strategy::Direct => {
            dit4_at_direct(
                dir, work, base, dist, log_m01, log_m23, log_m02, tables, scratch,
            );
        }
        Dit4Strategy::DirectSafe => {
            dit4_at_direct_safe(
                dir, work, base, dist, log_m01, log_m23, log_m02, tables, scratch,
            );
        }
        Dit4Strategy::Auto => unreachable!("Auto resolved in active_dit4_strategy"),
    }
}

/// Strategy A: safe pairwise decomposition via get_pair_mut + fft_dit2.
/// Each byte position is touched 4 times (once per dit2 call).
#[allow(clippy::too_many_arguments)]
fn dit4_at_decomposed<W: AsMut<[u8]>>(
    dir: TransformDir,
    work: &mut [W],
    base: usize,
    dist: usize,
    log_m01: u8,
    log_m23: u8,
    log_m02: u8,
    tables: &LeopardGf8Tables,
) {
    for i in 0..dist {
        dit4_pairwise_one(dir, work, base + i, dist, log_m01, log_m23, log_m02, tables);
    }
}

/// Strategy B: direct 4-lane butterfly with a raw-pointer fast path.
/// Uses raw pointer arithmetic for the common case (all 4 lanes in bounds),
/// falls back to safe pairwise decomposition for boundary cases.
/// Uses pre-allocated `scratch` buffer and pre-split nibble tables.
#[allow(clippy::too_many_arguments)]
fn dit4_at_direct<W: AsMut<[u8]>>(
    dir: TransformDir,
    work: &mut [W],
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
    let lut01 = &mul01.value;
    let lut23 = &mul23.value;
    let lut02 = &mul02.value;

    // Phase 1: bulk — all iterations guaranteed d < work.len(), no bounds check.
    let bulk_end = dist.min(work.len().saturating_sub(base + dist * 3));
    for i in 0..bulk_end {
        let a = base + i;
        let b = a + dist;
        let c = a + dist * 2;
        let d = a + dist * 3;
        // SAFETY: a < b < c < d < work.len(), all indices are distinct.
        unsafe {
            let ptr = work.as_mut_ptr();
            let a_ref = (*ptr.add(a)).as_mut();
            let b_ref = (*ptr.add(b)).as_mut();
            let c_ref = (*ptr.add(c)).as_mut();
            let d_ref = (*ptr.add(d)).as_mut();
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
                        lut01,
                        &mul01.low,
                        &mul01.high,
                        lut23,
                        &mul23.low,
                        &mul23.high,
                        lut02,
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
                        lut01,
                        &mul01.low,
                        &mul01.high,
                        lut23,
                        &mul23.low,
                        &mul23.high,
                        lut02,
                        &mul02.low,
                        &mul02.high,
                        scratch,
                    );
                }
            }
        }
    }

    // Phase 2: tail — boundary cases with fallback.
    for i in bulk_end..dist {
        let a = base + i;
        let d = a + dist * 3;
        if d < work.len() {
            let b = a + dist;
            let c = a + dist * 2;
            // SAFETY: d < work.len() checked above and a < b < c < d (dist > 0),
            // so all four indices are in bounds and pairwise distinct — the
            // four &mut lane views never alias.
            unsafe {
                let ptr = work.as_mut_ptr();
                let a_ref = (*ptr.add(a)).as_mut();
                let b_ref = (*ptr.add(b)).as_mut();
                let c_ref = (*ptr.add(c)).as_mut();
                let d_ref = (*ptr.add(d)).as_mut();
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
                            lut01,
                            &mul01.low,
                            &mul01.high,
                            lut23,
                            &mul23.low,
                            &mul23.high,
                            lut02,
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
                            lut01,
                            &mul01.low,
                            &mul01.high,
                            lut23,
                            &mul23.low,
                            &mul23.high,
                            lut02,
                            &mul02.low,
                            &mul02.high,
                            scratch,
                        );
                    }
                }
            }
        } else {
            dit4_pairwise_one(dir, work, a, dist, log_m01, log_m23, log_m02, tables);
        }
    }
}

/// Strategy C: direct 4-lane butterfly, fully safe via split_at_mut chains.
/// Each byte position is touched once (single-pass), but has extra index
/// arithmetic overhead from 3 split_at_mut calls per iteration.
/// Uses pre-allocated `scratch` buffer and pre-split nibble tables.
#[allow(clippy::too_many_arguments)]
fn dit4_at_direct_safe<W: AsMut<[u8]>>(
    dir: TransformDir,
    work: &mut [W],
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
    let lut01 = &mul01.value;
    let lut23 = &mul23.value;
    let lut02 = &mul02.value;

    for i in 0..dist {
        let a = base + i;
        let d = a + dist * 3;
        if d < work.len() {
            let b = a + dist;
            let c = a + dist * 2;
            let (left_bc, right_d) = work.split_at_mut(d);
            let (left_b, right_c) = left_bc.split_at_mut(c);
            let (left_a, right_b) = left_b.split_at_mut(b);
            let a_ref = left_a[a].as_mut();
            let b_ref = right_b[0].as_mut();
            let c_ref = right_c[0].as_mut();
            let d_ref = right_d[0].as_mut();
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
                        lut01,
                        &mul01.low,
                        &mul01.high,
                        lut23,
                        &mul23.low,
                        &mul23.high,
                        lut02,
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
                        lut01,
                        &mul01.low,
                        &mul01.high,
                        lut23,
                        &mul23.low,
                        &mul23.high,
                        lut02,
                        &mul02.low,
                        &mul02.high,
                        scratch,
                    );
                }
            }
        } else {
            dit4_pairwise_one(dir, work, a, dist, log_m01, log_m23, log_m02, tables);
        }
    }
}

/// Single-iteration pairwise decomposition for boundary cases.
/// Used by direct and direct-safe strategies when d >= work.len().
#[allow(clippy::too_many_arguments)]
fn dit4_pairwise_one<W: AsMut<[u8]>>(
    dir: TransformDir,
    work: &mut [W],
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
    let has_a = a < work.len();
    let has_b = b < work.len();
    let has_c = c < work.len();
    let has_d = d < work.len();
    let available = has_a as usize + has_b as usize + has_c as usize + has_d as usize;
    if available < 2 {
        return;
    }
    match dir {
        TransformDir::Forward => {
            if has_a
                && has_c
                && let Some((r1, r2)) = get_pair_mut(work, a, c)
            {
                fft_dit2(r1.as_mut(), r2.as_mut(), log_m02, tables);
            }
            if has_b
                && has_d
                && let Some((r1, r2)) = get_pair_mut(work, b, d)
            {
                fft_dit2(r1.as_mut(), r2.as_mut(), log_m02, tables);
            }
            if has_a
                && has_b
                && let Some((r1, r2)) = get_pair_mut(work, a, b)
            {
                fft_dit2(r1.as_mut(), r2.as_mut(), log_m01, tables);
            }
            if has_c
                && has_d
                && let Some((r1, r2)) = get_pair_mut(work, c, d)
            {
                fft_dit2(r1.as_mut(), r2.as_mut(), log_m23, tables);
            }
        }
        TransformDir::Inverse => {
            if has_a
                && has_b
                && let Some((r1, r2)) = get_pair_mut(work, a, b)
            {
                ifft_dit2(r1.as_mut(), r2.as_mut(), log_m01, tables);
            }
            if has_c
                && has_d
                && let Some((r1, r2)) = get_pair_mut(work, c, d)
            {
                ifft_dit2(r1.as_mut(), r2.as_mut(), log_m23, tables);
            }
            if has_a
                && has_c
                && let Some((r1, r2)) = get_pair_mut(work, a, c)
            {
                ifft_dit2(r1.as_mut(), r2.as_mut(), log_m02, tables);
            }
            if has_b
                && has_d
                && let Some((r1, r2)) = get_pair_mut(work, b, d)
            {
                ifft_dit2(r1.as_mut(), r2.as_mut(), log_m02, tables);
            }
        }
    }
}

#[allow(clippy::needless_range_loop)]
fn zero_trailing_lanes<W: AsMut<[u8]>>(work: &mut [W], start_lane: usize, count: usize) {
    for i in start_lane..start_lane + count {
        work[i].as_mut().fill(0);
    }
}

fn fft_dit8_with_plan<W: AsMut<[u8]>>(
    work: &mut [W],
    plan: &FftDit8Plan,
    tables: &LeopardGf8Tables,
    shard_size: usize,
    scratch: &mut [u8],
) {
    #[cfg(feature = "std")]
    PROFILE8.fft_stage_calls.fetch_add(1, Ordering::Relaxed);
    for block in &plan.stage4_blocks {
        dit4_at(
            TransformDir::Forward,
            work,
            block.r,
            block.dist,
            block.log_m01,
            block.log_m23,
            block.log_m02,
            tables,
            shard_size,
            scratch,
        );
    }

    for stage in &plan.final_stage {
        let r = stage.r;
        let (left, right) = work[r..r + stage.dist + 1].split_at_mut(stage.dist);
        fft_dit2(left[0].as_mut(), right[0].as_mut(), stage.log_m, tables);
    }
}

#[allow(clippy::too_many_arguments)]
fn ifft_dit_encoder8_with_plan<T: AsRef<[u8]>, W: AsMut<[u8]>>(
    data: &[T],
    plan: &IfftDit8Plan,
    work: &mut [W],
    mut xor_dst: Option<&mut [W]>,
    start: usize,
    end: usize,
    tables: &LeopardGf8Tables,
    _phase: IfftProfilePhase,
    shard_size: usize,
    scratch: &mut [u8],
) {
    #[cfg(feature = "std")]
    PROFILE8.add_ifft_calls(_phase);
    let _size = end - start;

    if plan.initial_blocks.is_empty() {
        for (idx, slot) in work.iter_mut().take(plan.mtrunc).enumerate() {
            slot.as_mut()
                .copy_from_slice(&data[idx].as_ref()[start..end]);
        }
        #[cfg(feature = "std")]
        PROFILE8.add_input_copy_bytes(_phase, plan.mtrunc * _size);
        zero_trailing_lanes(work, plan.mtrunc, plan.m - plan.mtrunc);
        #[cfg(feature = "std")]
        PROFILE8.add_zero_fill_bytes(_phase, (plan.m - plan.mtrunc) * _size);
    } else {
        for block in &plan.initial_blocks {
            let available = core::cmp::min(plan.mtrunc.saturating_sub(block.r), 4);
            for i in 0..available {
                work[block.r + i]
                    .as_mut()
                    .copy_from_slice(&data[block.r + i].as_ref()[start..end]);
            }
            #[cfg(feature = "std")]
            PROFILE8.add_input_copy_bytes(_phase, available * _size);
            for slot in work
                .iter_mut()
                .skip(block.r + available)
                .take(4usize.saturating_sub(available))
            {
                slot.as_mut().fill(0);
            }
            #[cfg(feature = "std")]
            PROFILE8.add_zero_fill_bytes(_phase, (4usize.saturating_sub(available)) * _size);

            dit4_at(
                TransformDir::Inverse,
                work,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
                shard_size,
                scratch,
            );
        }

        zero_trailing_lanes(
            work,
            plan.clear_start,
            plan.m.saturating_sub(plan.clear_start),
        );
        #[cfg(feature = "std")]
        PROFILE8.add_zero_fill_bytes(_phase, plan.m.saturating_sub(plan.clear_start) * _size);

        for block in &plan.later_blocks {
            dit4_at(
                TransformDir::Inverse,
                work,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
                shard_size,
                scratch,
            );
        }
    }

    if let Some(stage) = plan.final_stage {
        for i in 0..stage.dist {
            let (left, right) = work[i..i + stage.dist + 1].split_at_mut(stage.dist);
            ifft_dit2(left[0].as_mut(), right[0].as_mut(), stage.log_m, tables);
        }
    }

    if let Some(xor_dst) = xor_dst.as_mut() {
        for idx in 0..plan.m {
            let src = &*work[idx].as_mut();
            #[cfg(feature = "std")]
            PROFILE8.add_xor_bytes(_phase, src.len());
            slice_xor(src, xor_dst[idx].as_mut());
        }
    }
}

#[allow(dead_code)]
pub(super) fn fft_dit8<W: AsRef<[u8]> + AsMut<[u8]>>(
    work: &mut [W],
    mtrunc: usize,
    m: usize,
    skew_lut: &[u8; MODULUS8],
    tables: &LeopardGf8Tables,
) {
    let plan = build_fft_dit8_plan(mtrunc, m, skew_lut);
    // Allocate butterfly scratch here; this path is not performance-critical.
    let lane_len = work.first().map_or(0, |w| w.as_ref().len());
    let mut scratch = vec![0u8; lane_len];
    fft_dit8_with_plan(work, &plan, tables, 0, &mut scratch);
}

#[cfg(all(test, feature = "std"))]
mod encode_cache_budget_tests {
    use super::*;
    use crate::core::leopard::LEOPARD_MAX_CACHED_WORK_BYTES;

    fn clear_caches() {
        FLAT_WORK_CACHE.with(|cache| *cache.borrow_mut() = None);
        SCRATCH_CACHE.with(|cache| *cache.borrow_mut() = None);
        ENCODE_PLAN_CACHE.with(|cache| *cache.borrow_mut() = None);
    }

    fn small_cache_pointers() -> (*const u8, *const u8, *const u8) {
        let work = FLAT_WORK_CACHE.with(|cache| {
            let cache = cache.borrow();
            let work = cache.as_ref().unwrap();
            assert!(work.can_reuse(8, 64));
            work.lane(0).as_ptr().cast::<u8>()
        });
        let scratch =
            SCRATCH_CACHE.with(|cache| cache.borrow().as_ref().unwrap().as_ptr().cast::<u8>());
        let plan = ENCODE_PLAN_CACHE.with(|cache| {
            let cache = cache.borrow();
            let plan = cache.as_ref().unwrap();
            assert_eq!((plan.data_shards, plan.parity_shards), (10, 4));
            plan.first_ifft_plan.initial_blocks.as_ptr().cast::<u8>()
        });
        (work, scratch, plan)
    }

    #[test]
    fn oversized_encode_preserves_small_warm_buffers_and_plan() {
        clear_caches();
        let data: Vec<_> = (0..10).map(|index| vec![index as u8; 64]).collect();
        let mut parity = vec![vec![0; 64]; 4];
        encode_with_tables(10, 4, &data, &mut parity).unwrap();
        let expected = parity.clone();
        let warmed = small_cache_pointers();
        encode_with_tables(10, 4, &data, &mut parity).unwrap();
        assert_eq!(small_cache_pointers(), warmed);

        // The work allocation alone equals the budget. Scratch makes the
        // request oversized, so none of its resources may replace the cache.
        let large_data = vec![vec![0x5a; 32768]; 64];
        let mut large_parity = vec![vec![0; 32768]; 64];
        let driver = encode_with_tables(64, 64, &large_data, &mut large_parity).unwrap();
        let lane_len = (driver.shard_size / core::mem::size_of::<u8>()).min(driver.chunk_size);
        let work_bytes = driver.work_slices * lane_len * core::mem::size_of::<u8>();
        assert_eq!(work_bytes, LEOPARD_MAX_CACHED_WORK_BYTES);
        assert!(!leopard_work_cache_fits(
            work_bytes,
            lane_len * core::mem::size_of::<u8>()
        ));
        assert_eq!(small_cache_pointers(), warmed);

        parity.iter_mut().for_each(|shard| shard.fill(0xa5));
        encode_with_tables(10, 4, &data, &mut parity).unwrap();
        assert_eq!(parity, expected);
        assert_eq!(small_cache_pointers(), warmed);
    }

    #[test]
    fn cache_return_accounts_for_scratch_capacity_instead_of_length() {
        clear_caches();
        let work = FlatWork::new(1, 64);
        let mut scratch =
            Vec::<u8>::with_capacity(LEOPARD_MAX_CACHED_WORK_BYTES / core::mem::size_of::<u8>());
        scratch.push(0);
        assert_eq!(scratch.len(), 1);
        cache_encode_buffers(work, scratch);
        FLAT_WORK_CACHE.with(|cache| assert!(cache.borrow().is_none()));
        SCRATCH_CACHE.with(|cache| assert!(cache.borrow().is_none()));
    }
}
