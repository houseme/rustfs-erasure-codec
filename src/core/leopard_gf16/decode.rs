extern crate alloc;

use alloc::vec::Vec;
use smallvec::SmallVec;

use crate::errors::Error;

use super::ops::{
    fft_dit2_16, fft_dit4_16, fwht16_mtrunc, fwht16_variable, ifft_dit2_16, ifft_dit4_16, mulgf16,
    slice_xor_u16,
};
use super::work::FlatWork16;
use super::{
    FftDit16Plan, IfftDit16Plan, LeopardGf16Tables, MODULUS16, ORDER16, WORK_SIZE16,
    build_ifft_decode_dit16_plan, ceil_pow2, init_leopard_gf16_tables,
};

// At most one workspace is retained per calling/Rayon worker thread. FFT
// schedules are immutable and cached separately; these buffers contain only
// transient shard data and are fully initialized over each active chunk.
#[cfg(feature = "std")]
const MAX_CACHED_DECODE_BYTES: usize = 4 * 1024 * 1024;

#[cfg(feature = "std")]
fn cacheable_decode_size(lanes: usize, chunk_len: usize) -> bool {
    lanes
        .saturating_add(2)
        .saturating_mul(chunk_len)
        .saturating_mul(core::mem::size_of::<u16>())
        <= MAX_CACHED_DECODE_BYTES
}

struct DecodeBuffers {
    work: FlatWork16,
    // Input layout conversion and FFT butterflies use scratch at disjoint stages.
    scratch: Vec<u16>,
    out_scratch: Vec<u16>,
}

impl DecodeBuffers {
    fn new(lanes: usize, chunk_len: usize) -> Self {
        Self {
            work: FlatWork16::new(lanes, chunk_len),
            scratch: alloc::vec![0; chunk_len],
            out_scratch: alloc::vec![0; chunk_len],
        }
    }

    #[cfg(feature = "std")]
    fn retained_bytes(&self) -> usize {
        self.work.allocated_bytes().saturating_add(
            self.scratch
                .capacity()
                .saturating_add(self.out_scratch.capacity())
                .saturating_mul(core::mem::size_of::<u16>()),
        )
    }
}

#[cfg(feature = "std")]
thread_local! {
    static DECODE_BUFFER_CACHE: std::cell::RefCell<Option<DecodeBuffers>> =
        const { std::cell::RefCell::new(None) };
}

fn with_decode_buffers<R>(
    lanes: usize,
    chunk_len: usize,
    operation: impl FnOnce(&mut DecodeBuffers) -> R,
) -> R {
    // Decide cache eligibility before allocating. An oversized workspace is
    // still needed for this operation, but is never retained after it finishes.
    #[cfg(feature = "std")]
    let cacheable = cacheable_decode_size(lanes, chunk_len);
    #[cfg(feature = "std")]
    let mut buffers = DECODE_BUFFER_CACHE.with(|cache| {
        cache
            .take()
            .filter(|buffers| buffers.work.can_reuse(lanes, chunk_len))
            .unwrap_or_else(|| DecodeBuffers::new(lanes, chunk_len))
    });
    #[cfg(not(feature = "std"))]
    let mut buffers = DecodeBuffers::new(lanes, chunk_len);

    // Release the RefCell borrow before computation, so nested operations never
    // borrow an in-use workspace. On panic the local workspace is simply dropped.
    let result = operation(&mut buffers);
    #[cfg(feature = "std")]
    if cacheable && buffers.retained_bytes() <= MAX_CACHED_DECODE_BYTES {
        DECODE_BUFFER_CACHE.with(|cache| *cache.borrow_mut() = Some(buffers));
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeopardGf16DecodeDriver {
    pub(crate) data_shards: usize,
    pub(crate) parity_shards: usize,
    pub(crate) shard_size: usize,
    pub(crate) m: usize,
    pub(crate) n: usize,
    pub(crate) work_size: usize,
    pub(crate) chunk_size: usize,
}

pub(crate) fn build_leopard_gf16_decode_driver(
    data_shards: usize,
    parity_shards: usize,
    shard_size: usize,
) -> Result<LeopardGf16DecodeDriver, Error> {
    if shard_size == 0 || !shard_size.is_multiple_of(64) {
        return Err(Error::IncorrectShardSize);
    }
    let _tables = init_leopard_gf16_tables();

    let m = ceil_pow2(parity_shards.max(1));
    if m > MODULUS16 {
        return Err(Error::TooManyShards);
    }
    let work_size = m.checked_add(data_shards).ok_or(Error::TooManyShards)?;
    if work_size > super::ORDER16 {
        return Err(Error::TooManyShards);
    }
    let n = ceil_pow2(work_size);

    Ok(LeopardGf16DecodeDriver {
        data_shards,
        parity_shards,
        shard_size,
        m,
        n,
        work_size,
        chunk_size: WORK_SIZE16,
    })
}

#[allow(clippy::needless_range_loop)]
fn compute_error_locs16(
    missing_parity: &[usize],
    missing_data: &[usize],
    driver: &LeopardGf16DecodeDriver,
    tables: &LeopardGf16Tables,
) -> [u16; super::ORDER16] {
    let mut err_locs = [0u16; super::ORDER16];

    for &shard_idx in missing_parity {
        let pos = shard_idx - driver.data_shards;
        err_locs[pos] = 1;
    }

    for i in driver.parity_shards..driver.m {
        err_locs[i] = 1;
    }

    for &shard_idx in missing_data {
        err_locs[driver.m + shard_idx] = 1;
    }

    fwht16_mtrunc(&mut err_locs, ORDER16, driver.work_size);

    for i in 0..super::ORDER16 {
        if err_locs[i] == 0 {
            continue;
        }
        let product = (err_locs[i] as u64 * tables.log_walsh[i] as u64) % MODULUS16 as u64;
        err_locs[i] = product as u16;
    }

    fwht16_variable(&mut err_locs[..super::ORDER16]);

    err_locs
}

/// The erasure-pattern-dependent part of a GF(2^16) Leopard decode: the error
/// locator (two full-length FWHTs + a point-wise multiply, the fixed per-call
/// cost that dominates small-shard reconstructs) and the IFFT/FFT schedules.
///
/// It is a pure function of `(data_shards, parity_shards, present)` and the
/// global tables — **not** of `shard_size` (the schedules use `driver.n` /
/// `driver.work_size`, which derive only from the shard counts). So it can be
/// memoised across every reconstruct that shares the same shard configuration
/// and erasure pattern (e.g. a degraded array recovering many objects behind the
/// same missing disks), which is what [`decode_plan_for`] does.
pub(crate) struct LeopardGf16DecodePlan {
    err_locs: [u16; super::ORDER16],
    input_count: usize,
    ifft_plan: IfftDit16Plan,
    fft_plan: FftDit16Plan,
}

fn build_decode_plan(
    driver: &LeopardGf16DecodeDriver,
    present: &[bool],
    missing_parity: &[usize],
    missing_data: &[usize],
    tables: &LeopardGf16Tables,
) -> LeopardGf16DecodePlan {
    let err_locs = compute_error_locs16(missing_parity, missing_data, driver, tables);

    // input_count = m + last-present-data-index + 1 (matches Go's inputCount).
    let mut input_count = driver.m;
    for (i, &p) in present.iter().take(driver.data_shards).enumerate() {
        if p {
            input_count = driver.m + i + 1;
        }
    }

    let ifft_plan = build_ifft_decode_dit16_plan(input_count, driver.n, &tables.fft_skew);
    let fft_plan = super::build_fft_dit16_plan(driver.work_size, driver.n, &tables.fft_skew);

    LeopardGf16DecodePlan {
        err_locs,
        input_count,
        ifft_plan,
        fft_plan,
    }
}

/// Memoising wrapper over [`build_decode_plan`], keyed by
/// `(data_shards, parity_shards, present)`. `std` only — `no_std` builds the plan
/// fresh each call (no global allocator/mutex assumptions, and reconstruct is
/// cold there).
#[cfg(feature = "std")]
fn decode_plan_for(
    driver: &LeopardGf16DecodeDriver,
    present: &[bool],
    missing_parity: &[usize],
    missing_data: &[usize],
    tables: &LeopardGf16Tables,
) -> alloc::sync::Arc<LeopardGf16DecodePlan> {
    use alloc::sync::Arc;
    use hashlink::LruCache;
    use parking_lot::Mutex;
    use std::sync::OnceLock;

    type Key = (usize, usize, SmallVec<[bool; 256]>);
    // Bound the entry count, not total bytes: each error locator alone is
    // 128 KiB, with additional shape-dependent FFT schedules and an owned key.
    // A handful of erasure patterns per configuration is typical.
    const CACHE_CAPACITY: usize = 16;
    static CACHE: OnceLock<Mutex<LruCache<Key, Arc<LeopardGf16DecodePlan>>>> = OnceLock::new();

    let key = (
        driver.data_shards,
        driver.parity_shards,
        present.iter().copied().collect::<SmallVec<[bool; 256]>>(),
    );
    let cache = CACHE.get_or_init(|| Mutex::new(LruCache::new(CACHE_CAPACITY)));

    if let Some(plan) = cache.lock().get(&key) {
        return plan.clone();
    }
    // Build outside the lock (the FWHTs are the expensive part); a concurrent
    // duplicate build at most wastes work, never corrupts (the value is a pure
    // function of the key).
    let plan = Arc::new(build_decode_plan(
        driver,
        present,
        missing_parity,
        missing_data,
        tables,
    ));
    cache.lock().insert(key, plan.clone());
    plan
}

#[cfg(test)]
pub(crate) fn reconstruct_with_tables16(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
    tables: &LeopardGf16Tables,
) -> Result<(), Error> {
    reconstruct_with_tables16_with_options(
        present,
        outputs,
        input_data,
        data_shards,
        parity_shards,
        tables,
        0,
    )
}

#[allow(clippy::needless_range_loop)]
pub(crate) fn reconstruct_with_tables16_with_options(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
    tables: &LeopardGf16Tables,
    max_parallel_jobs: usize,
) -> Result<(), Error> {
    #[cfg(not(feature = "std"))]
    let _ = max_parallel_jobs;
    let shard_size = super::super::reconstruct::validate_leopard_decode_buffers(
        present,
        outputs,
        input_data,
        data_shards,
        parity_shards,
    )?;
    let total_shards = data_shards + parity_shards;
    if present.iter().all(|is_present| *is_present)
        || outputs.iter().all(|output| output.is_empty())
    {
        return Ok(());
    }

    let driver = build_leopard_gf16_decode_driver(data_shards, parity_shards, shard_size)?;
    let shard_u16_len = shard_size / 2;

    let mut missing_parity = SmallVec::<[usize; 32]>::new();
    let mut missing_data = SmallVec::<[usize; 32]>::new();
    for i in data_shards..total_shards {
        if !present[i] {
            missing_parity.push(i);
        }
    }
    for i in 0..data_shards {
        if !present[i] {
            missing_data.push(i);
        }
    }

    let total_missing = missing_parity.len() + missing_data.len();
    if total_missing > parity_shards {
        return Err(Error::TooFewShardsPresent);
    }
    if total_missing == 0 {
        return Ok(());
    }

    // The error locator + FFT/IFFT schedules depend only on the shard counts and
    // erasure pattern (not the data or shard size), so memoise them (std). This
    // is the fixed per-call cost — two full-length FWHTs — that dominates
    // small-shard and repeated same-pattern reconstructs.
    #[cfg(feature = "std")]
    let plan = decode_plan_for(&driver, present, &missing_parity, &missing_data, tables);
    #[cfg(not(feature = "std"))]
    let plan = build_decode_plan(&driver, present, &missing_parity, &missing_data, tables);
    let err_locs = &plan.err_locs;
    let ifft_plan = &plan.ifft_plan;
    let fft_plan = &plan.fft_plan;

    let chunk_cap = core::cmp::min(shard_u16_len, driver.chunk_size);

    // Recovery targets: (shard index, work lane, inverse error-locator value) for
    // each missing shard. The order is reused for the per-chunk output slices.
    let missing: SmallVec<[(usize, usize, u16); 32]> = (0..total_shards)
        .filter(|&i| !present[i] && !outputs[i].is_empty())
        .map(|i| {
            let (work_idx, err_idx) = if i >= data_shards {
                (i - data_shards, i - data_shards)
            } else {
                (driver.m + i, driver.m + i)
            };
            (
                i,
                work_idx,
                (MODULUS16 as u16).wrapping_sub(err_locs[err_idx]),
            )
        })
        .collect();

    // Each `driver.chunk_size` (64 KiB) window is an independent IFFT ->
    // formal-derivative -> FFT job writing a disjoint output byte range, so shards
    // larger than one chunk parallelise across the rayon pool. Single-chunk shards
    // (<= 64 KiB) and `no_std` take the serial path, reusing one set of buffers.
    #[cfg(feature = "std")]
    if max_parallel_jobs != 1 && shard_u16_len.div_ceil(driver.chunk_size) >= 2 {
        return reconstruct_chunks_parallel(
            &driver,
            present,
            outputs,
            input_data,
            err_locs,
            ifft_plan,
            fft_plan,
            tables,
            &missing,
            shard_u16_len,
            chunk_cap,
            max_parallel_jobs,
        );
    }

    with_decode_buffers(driver.n, chunk_cap, |buffers| {
        let DecodeBuffers {
            work,
            scratch,
            out_scratch,
        } = buffers;
        let mut offset = 0usize;
        while offset < shard_u16_len {
            let end = core::cmp::min(offset + driver.chunk_size, shard_u16_len);
            let size = end - offset;

            compute_reconstruct_chunk(
                work, scratch, offset, end, size, present, input_data, err_locs, ifft_plan,
                fft_plan, tables, &driver,
            )?;

            if out_scratch.len() < size {
                out_scratch.resize(size, 0);
            }
            // Step 5: scale each missing shard's transform by the inverse error locator
            // and write it straight into user byte layout at this chunk's byte range.
            for &(shard_idx, work_idx, inv_err) in &missing {
                mulgf16(
                    &mut out_scratch[..size],
                    &work.lane(work_idx)[..size],
                    inv_err,
                    tables,
                );
                super::ops::work_u16_to_user_bytes(
                    &out_scratch[..size],
                    &mut outputs[shard_idx][offset * 2..end * 2],
                );
            }
            offset = end;
        }
        Ok(())
    })
}

/// Steps 1-4 of a single reconstruct chunk: multiply the received shards by the
/// error locator, then IFFT -> formal derivative -> FFT. Leaves the recovered
/// transform in `work` for the caller's Step 5 (per-missing-shard scaling +
/// write-out). Shared by the serial and parallel reconstruct paths.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn compute_reconstruct_chunk(
    work: &mut FlatWork16,
    scratch: &mut Vec<u16>,
    offset: usize,
    end: usize,
    size: usize,
    present: &[bool],
    input_data: &[Option<&[u8]>],
    err_locs: &[u16; ORDER16],
    ifft_plan: &IfftDit16Plan,
    fft_plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    driver: &LeopardGf16DecodeDriver,
) -> Result<(), Error> {
    if scratch.len() < size {
        scratch.resize(size, 0);
    }

    // Step 1: multiply received data by error locator values.
    // Work layout: lanes 0..m = parity, lanes m..m+data_shards = data.
    for i in 0..driver.data_shards {
        let work_idx = driver.m + i;
        if present[i] {
            let input = input_data[i].ok_or(Error::TooFewShardsPresent)?;
            super::ops::user_bytes_to_work_u16_into(
                &input[offset * 2..end * 2],
                &mut scratch[..size],
            );
            mulgf16(
                &mut work.lane_mut(work_idx)[..size],
                &scratch[..size],
                err_locs[work_idx],
                tables,
            );
        } else {
            work.lane_mut(work_idx)[..size].fill(0);
        }
    }
    for i in 0..driver.parity_shards {
        let shard_idx = driver.data_shards + i;
        if present[shard_idx] {
            let input = input_data[shard_idx].ok_or(Error::TooFewShardsPresent)?;
            super::ops::user_bytes_to_work_u16_into(
                &input[offset * 2..end * 2],
                &mut scratch[..size],
            );
            mulgf16(
                &mut work.lane_mut(i)[..size],
                &scratch[..size],
                err_locs[i],
                tables,
            );
        } else {
            work.lane_mut(i)[..size].fill(0);
        }
    }
    for i in driver.parity_shards..driver.m {
        work.lane_mut(i)[..size].fill(0);
    }
    for i in driver.work_size..driver.n {
        work.lane_mut(i)[..size].fill(0);
    }

    // Step 2: IFFT.
    ifft_dit_decode16_with_plan(work, size, ifft_plan, tables, driver.n, scratch);
    // Step 3: formal derivative.
    compute_formal_derivative16(work, driver.n, size);
    // Step 4: FFT.
    fft_dit_decode16_with_plan(work, size, fft_plan, tables, driver.n, scratch);

    Ok(())
}

/// Reconstruct each 64 KiB chunk on the rayon pool. Chunks touch disjoint output
/// byte ranges, so the per-shard outputs are split into per-chunk mutable slices
/// (a safe `chunks_mut` transpose) and each job owns its slice group plus a
/// per-thread work buffer reused across the jobs that thread runs.
#[cfg(feature = "std")]
#[allow(clippy::too_many_arguments)]
fn reconstruct_chunks_parallel(
    driver: &LeopardGf16DecodeDriver,
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    err_locs: &[u16; ORDER16],
    ifft_plan: &IfftDit16Plan,
    fft_plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    missing: &[(usize, usize, u16)],
    shard_u16_len: usize,
    chunk_cap: usize,
    max_parallel_jobs: usize,
) -> Result<(), Error> {
    use rayon::prelude::*;

    let chunk_bytes = driver.chunk_size * 2;
    // One mutable-chunk iterator per missing shard, in the same order as `missing`.
    let mut per_shard_chunks: Vec<_> = outputs
        .iter_mut()
        .enumerate()
        .filter(|(i, out)| !present[*i] && !out.is_empty())
        .map(|(_, out)| out.chunks_mut(chunk_bytes))
        .collect();

    // Transpose to per-chunk jobs: (offset, size, one output slice per missing shard).
    let mut jobs: Vec<(usize, usize, Vec<&mut [u8]>)> = Vec::new();
    let mut offset = 0usize;
    while offset < shard_u16_len {
        let end = core::cmp::min(offset + driver.chunk_size, shard_u16_len);
        let slices: Vec<&mut [u8]> = per_shard_chunks
            .iter_mut()
            .map(|it| it.next().expect("chunk count matches shard length"))
            .collect();
        jobs.push((offset, end - offset, slices));
        offset = end;
    }
    drop(per_shard_chunks);

    let available_jobs = rayon::current_num_threads().max(1);
    let max_jobs = if max_parallel_jobs == 0 {
        available_jobs
    } else {
        max_parallel_jobs.min(available_jobs).max(1)
    };
    let chunks_per_job = jobs.len().div_ceil(max_jobs).max(1);
    jobs.par_chunks_mut(chunks_per_job).try_for_each(|group| {
        with_decode_buffers(driver.n, chunk_cap, |buffers| {
            let DecodeBuffers {
                work,
                scratch,
                out_scratch,
            } = buffers;
            for (offset, size, chunk_out) in group {
                let (offset, size) = (*offset, *size);
                compute_reconstruct_chunk(
                    work,
                    scratch,
                    offset,
                    offset + size,
                    size,
                    present,
                    input_data,
                    err_locs,
                    ifft_plan,
                    fft_plan,
                    tables,
                    driver,
                )?;
                if out_scratch.len() < size {
                    out_scratch.resize(size, 0);
                }
                for (k, &(_, work_idx, inv_err)) in missing.iter().enumerate() {
                    mulgf16(
                        &mut out_scratch[..size],
                        &work.lane(work_idx)[..size],
                        inv_err,
                        tables,
                    );
                    super::ops::work_u16_to_user_bytes(&out_scratch[..size], chunk_out[k]);
                }
            }
            Ok(())
        })
    })
}

pub(super) fn ifft_dit_decode16_with_plan(
    work: &mut FlatWork16,
    size: usize,
    plan: &IfftDit16Plan,
    tables: &LeopardGf16Tables,
    n: usize,
    scratch: &mut [u16],
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

            dit4_decode_at_16(
                false,
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
            dit4_decode_at_16(
                false,
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
                && let Some((ra, rb)) = get_pair_mut_flat16(work, a, b, size)
            {
                ifft_dit2_16(ra, rb, stage.log_m, tables);
            }
        }
    }
}

pub(super) fn fft_dit_decode16_with_plan(
    work: &mut FlatWork16,
    size: usize,
    plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    n: usize,
    scratch: &mut [u16],
) {
    for block in &plan.stage4_blocks {
        dit4_decode_at_16(
            true,
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
            && let Some((ra, rb)) = get_pair_mut_flat16(work, a, b, size)
        {
            fft_dit2_16(ra, rb, stage.log_m, tables);
        }
    }
}

fn compute_formal_derivative16(work: &mut FlatWork16, n: usize, size: usize) {
    for i in 1..n {
        let width = ((i ^ (i - 1)) + 1) >> 1;
        for j in 0..width {
            let lo_idx = i - width + j;
            let hi_idx = i + j;
            if hi_idx < n && lo_idx < n {
                // Go: slicesXor(work[i-width:i], work[i:i+width]) modifies v1=work[i-width:i]
                // slicesXor(v1, v2) → for each j: sliceXor(v2[j], v1[j]) → v1[j] ^= v2[j]
                // So: work[lo_idx] ^= work[hi_idx], i.e., lo ^= hi
                if let Some((lo, hi)) = get_pair_mut_flat16(work, lo_idx, hi_idx, size) {
                    slice_xor_u16(lo, hi);
                }
            }
        }
    }
}

fn get_pair_mut_flat16(
    work: &mut FlatWork16,
    i: usize,
    j: usize,
    size: usize,
) -> Option<(&mut [u16], &mut [u16])> {
    if i == j || i >= work.lanes() || j >= work.lanes() {
        return None;
    }
    // SAFETY: i != j and both < lanes (checked above); lane_mut(i) and lane_mut(j) return slices over disjoint lanes, so the two &mut [u16] from the raw-pointer reborrow never alias.
    unsafe {
        let ptr = work as *mut FlatWork16;
        let lane_i = (*ptr).lane_mut(i);
        let lane_j = (*ptr).lane_mut(j);
        Some((&mut lane_i[..size], &mut lane_j[..size]))
    }
}

#[allow(clippy::too_many_arguments)]
fn dit4_decode_at_16(
    forward: bool,
    work: &mut FlatWork16,
    size: usize,
    base: usize,
    dist: usize,
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
    tables: &LeopardGf16Tables,
    scratch: &mut [u16],
) {
    for i in 0..dist {
        let a = base + i;
        let d = a + dist * 3;
        if d >= work.lanes() {
            dit4_decode_pairwise_16(
                forward, work, size, a, dist, log_m01, log_m23, log_m02, tables,
            );
            continue;
        }

        let b = a + dist;
        let c = a + dist * 2;

        // SAFETY: d < work.lanes() (checked above) and a<b<c<d are distinct (dist >= 1), so the four lane_mut calls via the raw *mut reborrow return four disjoint, non-aliasing lane slices.
        unsafe {
            let ptr = work as *mut FlatWork16;
            let a_ref = &mut (*ptr).lane_mut(a)[..size];
            let b_ref = &mut (*ptr).lane_mut(b)[..size];
            let c_ref = &mut (*ptr).lane_mut(c)[..size];
            let d_ref = &mut (*ptr).lane_mut(d)[..size];
            if forward {
                fft_dit4_16(
                    a_ref, b_ref, c_ref, d_ref, log_m01, log_m23, log_m02, tables,
                );
            } else {
                ifft_dit4_16(
                    a_ref, b_ref, c_ref, d_ref, log_m01, log_m23, log_m02, tables,
                );
            }
        }
        let _ = scratch;
    }
}

#[allow(clippy::too_many_arguments)]
fn dit4_decode_pairwise_16(
    forward: bool,
    work: &mut FlatWork16,
    size: usize,
    a: usize,
    dist: usize,
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
    tables: &LeopardGf16Tables,
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

    if forward {
        if has_a
            && has_c
            && let Some((r1, r2)) = get_pair_mut_flat16(work, a, c, size)
        {
            fft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_b
            && has_d
            && let Some((r1, r2)) = get_pair_mut_flat16(work, b, d, size)
        {
            fft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_a
            && has_b
            && let Some((r1, r2)) = get_pair_mut_flat16(work, a, b, size)
        {
            fft_dit2_16(r1, r2, log_m01, tables);
        }
        if has_c
            && has_d
            && let Some((r1, r2)) = get_pair_mut_flat16(work, c, d, size)
        {
            fft_dit2_16(r1, r2, log_m23, tables);
        }
    } else {
        if has_a
            && has_b
            && let Some((r1, r2)) = get_pair_mut_flat16(work, a, b, size)
        {
            ifft_dit2_16(r1, r2, log_m01, tables);
        }
        if has_c
            && has_d
            && let Some((r1, r2)) = get_pair_mut_flat16(work, c, d, size)
        {
            ifft_dit2_16(r1, r2, log_m23, tables);
        }
        if has_a
            && has_c
            && let Some((r1, r2)) = get_pair_mut_flat16(work, a, c, size)
        {
            ifft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_b
            && has_d
            && let Some((r1, r2)) = get_pair_mut_flat16(work, b, d, size)
        {
            ifft_dit2_16(r1, r2, log_m02, tables);
        }
    }
}

#[cfg(test)]
mod decode_driver_boundary_tests {
    #[test]
    fn permits_the_full_field_transform_order() {
        let driver =
            super::build_leopard_gf16_decode_driver(super::super::ORDER16 - 4, 4, 64).unwrap();
        assert_eq!(driver.n, super::super::ORDER16);
        assert_eq!(
            super::build_leopard_gf16_decode_driver(super::super::ORDER16 - 3, 4, 64),
            Err(crate::Error::TooManyShards),
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod decode_buffer_tests {
    use super::*;

    #[test]
    fn decode_workspace_retention_counts_all_buffers_and_drops_oversized_entries() {
        assert!(!cacheable_decode_size(usize::MAX, usize::MAX));
        assert!(!cacheable_decode_size(64, 32 * 1024));
        DECODE_BUFFER_CACHE.with(|cache| *cache.borrow_mut() = None);
        with_decode_buffers(16, 64, |buffers| {
            assert_eq!(buffers.retained_bytes(), (16 + 2) * 64 * 2);
        });
        DECODE_BUFFER_CACHE.with(|cache| assert!(cache.borrow().is_some()));
        // FlatWork alone fits exactly; the two scratch buffers put it over cap.
        with_decode_buffers(64, 32 * 1024, |buffers| {
            assert!(buffers.retained_bytes() > MAX_CACHED_DECODE_BYTES);
        });
        DECODE_BUFFER_CACHE.with(|cache| assert!(cache.borrow().is_none()));
    }
}
