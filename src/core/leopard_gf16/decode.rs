extern crate alloc;

use alloc::vec::Vec;
use smallvec::SmallVec;

use crate::errors::Error;

use super::ops::{
    fft_dit2_16, fft_dit4_16, fwht16_mtrunc, fwht16_variable, ifft_dit2_16, ifft_dit4_16,
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
        .saturating_add(1)
        .saturating_mul(chunk_len)
        .saturating_mul(core::mem::size_of::<u16>())
        <= MAX_CACHED_DECODE_BYTES
}

struct DecodeBuffers {
    work: FlatWork16,
    // The std generic conversion fallback needs scratch. no_std conversion is
    // fused, and the FFT routines do not use the scratch argument.
    scratch: Vec<u16>,
}

impl DecodeBuffers {
    fn new(lanes: usize, chunk_len: usize) -> Self {
        Self {
            work: FlatWork16::new(lanes, chunk_len),
            scratch: {
                #[cfg(feature = "std")]
                {
                    alloc::vec![0; chunk_len]
                }
                #[cfg(not(feature = "std"))]
                {
                    Vec::new()
                }
            },
        }
    }

    #[cfg(feature = "std")]
    fn retained_bytes(&self) -> usize {
        self.work.allocated_bytes().saturating_add(
            self.scratch
                .capacity()
                .saturating_mul(core::mem::size_of::<u16>()),
        )
    }
}

// no_std conversion is fused, so its workspace is deliberately empty.
#[inline(always)]
fn conversion_scratch(scratch: &mut [u16], size: usize) -> &mut [u16] {
    #[cfg(feature = "std")]
    {
        &mut scratch[..size]
    }
    #[cfg(not(feature = "std"))]
    {
        let _ = size;
        scratch
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
    let mut buffers = if cacheable {
        DECODE_BUFFER_CACHE.with(|cache| {
            cache
                .take()
                .filter(|buffers| buffers.work.can_reuse(lanes, chunk_len))
                .unwrap_or_else(|| DecodeBuffers::new(lanes, chunk_len))
        })
    } else {
        // An oversized operation must not evict a smaller warm workspace.
        DecodeBuffers::new(lanes, chunk_len)
    };
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

#[cfg(all(target_arch = "aarch64", not(feature = "std")))]
#[inline(always)]
fn mul_locator_mod16(a: u16, b: u16) -> u16 {
    let product = u32::from(a) * u32::from(b);
    // 2^16 is 1 modulo 65535; the folded sum is at most 131069.
    let folded = (product & 0xffff) + (product >> 16);
    if folded >= 0xffff {
        (folded - 0xffff) as u16
    } else {
        folded as u16
    }
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

    #[cfg(all(target_arch = "aarch64", not(feature = "std")))]
    for i in 0..super::ORDER16 {
        err_locs[i] = mul_locator_mod16(err_locs[i], tables.log_walsh[i]);
    }
    #[cfg(any(not(target_arch = "aarch64"), feature = "std"))]
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

#[cfg(feature = "std")]
type ErrorLocations = [u16];
#[cfg(not(feature = "std"))]
type ErrorLocations = [u16; ORDER16];

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
    #[cfg(feature = "std")]
    err_locs: SmallVec<[u16; SPARSE_LOCATOR_MAX_WORK_SIZE]>,
    #[cfg(not(feature = "std"))]
    err_locs: [u16; super::ORDER16],
    input_count: usize,
    ifft_plan: IfftDit16Plan,
    fft_plan: FftDit16Plan,
}

#[cfg(feature = "std")]
impl LeopardGf16DecodePlan {
    fn retained_bytes(&self) -> usize {
        let locator_bytes = if self.err_locs.spilled() {
            self.err_locs.capacity() * core::mem::size_of::<u16>()
        } else {
            0
        };
        core::mem::size_of::<Self>()
            + locator_bytes
            + (self.ifft_plan.initial_blocks.capacity()
                + self.ifft_plan.later_blocks.capacity()
                + self.fft_plan.stage4_blocks.capacity())
                * core::mem::size_of::<super::Stage4Block>()
            + self.fft_plan.final_stage.capacity() * core::mem::size_of::<super::Stage2Block>()
    }
}

#[cfg(feature = "std")]
type DecodePlanKey = (usize, usize, SmallVec<[bool; 256]>);

/// Charge keys and plan-owned storage while keeping independent callers' Arc
/// lifetimes outside the cache budget. LRU bookkeeping and allocator overhead
/// are excluded, as with the Classic inverse-cache accounting.
#[cfg(feature = "std")]
struct DecodePlanCache {
    entries: hashlink::LruCache<DecodePlanKey, (alloc::sync::Arc<LeopardGf16DecodePlan>, usize)>,
    retained_bytes: usize,
    max_bytes: usize,
}

#[cfg(feature = "std")]
impl DecodePlanCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: hashlink::LruCache::new(16),
            retained_bytes: 0,
            max_bytes,
        }
    }

    fn get(&mut self, key: &DecodePlanKey) -> Option<alloc::sync::Arc<LeopardGf16DecodePlan>> {
        self.entries.get(key).map(|(plan, _)| plan.clone())
    }

    fn insert(
        &mut self,
        key: DecodePlanKey,
        plan: alloc::sync::Arc<LeopardGf16DecodePlan>,
    ) -> alloc::sync::Arc<LeopardGf16DecodePlan> {
        // Another caller may have completed the same plan while this one built
        // outside the lock. Reuse its entry and account for it only once.
        if let Some(existing) = self.get(&key) {
            return existing;
        }
        let key_bytes = core::mem::size_of::<DecodePlanKey>()
            + if key.2.spilled() { key.2.capacity() } else { 0 };
        let Some(bytes) = plan.retained_bytes().checked_add(key_bytes) else {
            return plan;
        };
        if bytes > self.max_bytes {
            return plan;
        }
        while self.entries.len() >= self.entries.capacity()
            || self.retained_bytes > self.max_bytes - bytes
        {
            let (_, (_, removed_bytes)) = self.entries.remove_lru().expect("cache contains a plan");
            self.retained_bytes -= removed_bytes;
        }
        self.entries.insert(key, (plan.clone(), bytes));
        self.retained_bytes += bytes;
        plan
    }
}

const SPARSE_LOCATOR_MAX_WORK_SIZE: usize = 256;
const SPARSE_LOCATOR_MAX_TERMS: usize = 16;

// The locator is an XOR convolution of the erasure indicator with the log
// table. The Walsh transform is its own inverse up to a factor of 65,536,
// which is 1 modulo 65,535. For small erasure sets, sum the relevant log-table
// entries directly instead of performing two full transforms.
//
// Only indices below work_size are used by recovery. The table value at zero
// and the transform may represent modular zero as 65,535; normalizing it to 0
// is equivalent for the GF16 scaling operations that consume these values.
// In std this is used only while building a cache-miss plan. Keep it out of
// the cache lookup body to avoid expanding frequently executed dispatch code.
#[cfg_attr(feature = "std", inline(never))]
fn fill_sparse_error_locs16(
    locs: &mut [u16],
    missing_parity: &[usize],
    missing_data: &[usize],
    driver: &LeopardGf16DecodeDriver,
    tables: &LeopardGf16Tables,
) {
    debug_assert_eq!(locs.len(), driver.work_size);
    debug_assert!(
        missing_parity.len() + missing_data.len() + (driver.m - driver.parity_shards)
            <= SPARSE_LOCATOR_MAX_TERMS
    );
    for (i, out) in locs.iter_mut().enumerate() {
        let mut sum = 0u32;
        for &shard_idx in missing_parity {
            sum += u32::from(tables.log_lut[i ^ (shard_idx - driver.data_shards)]);
        }
        for pos in driver.parity_shards..driver.m {
            sum += u32::from(tables.log_lut[i ^ pos]);
        }
        for &shard_idx in missing_data {
            sum += u32::from(tables.log_lut[i ^ (driver.m + shard_idx)]);
        }
        *out = (sum % MODULUS16 as u32) as u16;
    }
}

#[cfg(any(test, not(feature = "std")))]
fn sparse_error_locs16(
    missing_parity: &[usize],
    missing_data: &[usize],
    driver: &LeopardGf16DecodeDriver,
    tables: &LeopardGf16Tables,
) -> [u16; ORDER16] {
    let mut locs = [0u16; ORDER16];
    fill_sparse_error_locs16(
        &mut locs[..driver.work_size],
        missing_parity,
        missing_data,
        driver,
        tables,
    );
    locs
}

fn build_decode_plan(
    driver: &LeopardGf16DecodeDriver,
    present: &[bool],
    missing_parity: &[usize],
    missing_data: &[usize],
    tables: &LeopardGf16Tables,
) -> LeopardGf16DecodePlan {
    // Cap direct work at 256 * 16 table lookups, including padded parity.
    let terms = missing_parity.len() + missing_data.len() + (driver.m - driver.parity_shards);
    let err_locs =
        if driver.work_size <= SPARSE_LOCATOR_MAX_WORK_SIZE && terms <= SPARSE_LOCATOR_MAX_TERMS {
            #[cfg(feature = "std")]
            {
                // Both zero encodings mean identity. Use the sentinel for
                // present inputs to select direct conversion, and retain zero
                // for missing outputs so inverse scaling selects it as well.
                let mut locs = SmallVec::<[u16; SPARSE_LOCATOR_MAX_WORK_SIZE]>::new();
                locs.resize(driver.work_size, 0);
                fill_sparse_error_locs16(
                    &mut locs[..driver.work_size],
                    missing_parity,
                    missing_data,
                    driver,
                    tables,
                );
                for (i, &is_present) in present.iter().take(driver.data_shards).enumerate() {
                    if is_present && locs[driver.m + i] == 0 {
                        locs[driver.m + i] = MODULUS16 as u16;
                    }
                }
                for (i, &is_present) in present
                    .iter()
                    .skip(driver.data_shards)
                    .take(driver.parity_shards)
                    .enumerate()
                {
                    if is_present && locs[i] == 0 {
                        locs[i] = MODULUS16 as u16;
                    }
                }
                locs
            }
            #[cfg(not(feature = "std"))]
            {
                sparse_error_locs16(missing_parity, missing_data, driver, tables)
            }
        } else {
            #[cfg(feature = "std")]
            {
                let full = compute_error_locs16(missing_parity, missing_data, driver, tables);
                SmallVec::from_slice(&full[..driver.work_size])
            }
            #[cfg(not(feature = "std"))]
            {
                compute_error_locs16(missing_parity, missing_data, driver, tables)
            }
        };

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
    use parking_lot::Mutex;
    use std::sync::OnceLock;

    const MAX_CACHED_PLAN_BYTES: usize = 8 * 1024 * 1024;
    static CACHE: OnceLock<Mutex<DecodePlanCache>> = OnceLock::new();

    let key = (
        driver.data_shards,
        driver.parity_shards,
        present.iter().copied().collect::<SmallVec<[bool; 256]>>(),
    );
    let cache = CACHE.get_or_init(|| Mutex::new(DecodePlanCache::new(MAX_CACHED_PLAN_BYTES)));

    if let Some(plan) = cache.lock().get(&key) {
        return plan;
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
    cache.lock().insert(key, plan)
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
    #[cfg(feature = "std")]
    let err_locs = plan.err_locs.as_slice();
    #[cfg(not(feature = "std"))]
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
        let DecodeBuffers { work, scratch } = buffers;
        let mut offset = 0usize;
        while offset < shard_u16_len {
            let end = core::cmp::min(offset + driver.chunk_size, shard_u16_len);
            let size = end - offset;

            compute_reconstruct_chunk(
                work, scratch, offset, end, size, present, input_data, err_locs, ifft_plan,
                fft_plan, tables, &driver,
            )?;

            // Step 5: scale each missing shard's transform by the inverse error locator
            // and write it straight into user byte layout at this chunk's byte range.
            for &(shard_idx, work_idx, inv_err) in &missing {
                super::fused::work_mul_to_user(
                    &work.lane(work_idx)[..size],
                    &mut outputs[shard_idx][offset * 2..end * 2],
                    conversion_scratch(scratch, size),
                    inv_err,
                    tables,
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
    #[allow(clippy::ptr_arg)] scratch: &mut Vec<u16>,
    offset: usize,
    end: usize,
    size: usize,
    present: &[bool],
    input_data: &[Option<&[u8]>],
    err_locs: &ErrorLocations,
    ifft_plan: &IfftDit16Plan,
    fft_plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    driver: &LeopardGf16DecodeDriver,
) -> Result<(), Error> {
    #[cfg(feature = "std")]
    if scratch.len() < size {
        scratch.resize(size, 0);
    }

    // Step 1: multiply received data by error locator values.
    // Work layout: lanes 0..m = parity, lanes m..m+data_shards = data.
    for i in 0..driver.data_shards {
        let work_idx = driver.m + i;
        if present[i] {
            let input = input_data[i].ok_or(Error::TooFewShardsPresent)?;
            super::fused::user_to_work_mul(
                &input[offset * 2..end * 2],
                &mut work.lane_mut(work_idx)[..size],
                conversion_scratch(scratch, size),
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
            super::fused::user_to_work_mul(
                &input[offset * 2..end * 2],
                &mut work.lane_mut(i)[..size],
                conversion_scratch(scratch, size),
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

/// Reconstruct 64 KiB chunks in the existing contiguous Rayon groups. Split
/// each output once per group, so the staged reference tables are bounded by
/// the job budget rather than shard length. Each group owns disjoint output
/// ranges and leases the same per-thread work buffers as before.
#[cfg(feature = "std")]
#[allow(clippy::too_many_arguments)]
fn reconstruct_chunks_parallel(
    driver: &LeopardGf16DecodeDriver,
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    err_locs: &ErrorLocations,
    ifft_plan: &IfftDit16Plan,
    fft_plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    missing: &[(usize, usize, u16)],
    shard_u16_len: usize,
    chunk_cap: usize,
    max_parallel_jobs: usize,
) -> Result<(), Error> {
    use rayon::prelude::*;

    let chunk_count = shard_u16_len.div_ceil(driver.chunk_size);
    let available_jobs = rayon::current_num_threads().max(1);
    let max_jobs = if max_parallel_jobs == 0 {
        available_jobs
    } else {
        max_parallel_jobs.min(available_jobs).max(1)
    };
    // These are the same contiguous chunk groups as par_chunks_mut previously
    // formed from the per-chunk job table. Only their view preparation changes.
    let chunks_per_job = chunk_count.div_ceil(max_jobs).max(1);
    let chunk_bytes = driver.chunk_size * 2;
    let group_bytes = chunks_per_job.saturating_mul(chunk_bytes);
    let mut per_shard_groups: Vec<_> = outputs
        .iter_mut()
        .enumerate()
        .filter(|(i, out)| !present[*i] && !out.is_empty())
        .map(|(_, out)| out.chunks_mut(group_bytes))
        .collect();

    // Each item stores its absolute u16 range and relative output byte slices.
    let mut jobs = Vec::with_capacity(chunk_count.div_ceil(chunks_per_job));
    let mut offset = 0usize;
    while offset < driver.shard_size {
        let end = offset.saturating_add(group_bytes).min(driver.shard_size);
        let slices: Vec<&mut [u8]> = per_shard_groups
            .iter_mut()
            .map(|it| it.next().expect("group count matches shard length"))
            .collect();
        jobs.push((offset / 2, end / 2, slices));
        offset = end;
    }
    drop(per_shard_groups);

    jobs.par_iter_mut()
        .try_for_each(|(start, group_end, group_out)| {
            let start = *start;
            with_decode_buffers(driver.n, chunk_cap, |buffers| {
                let DecodeBuffers { work, scratch } = buffers;
                let mut offset = start;
                while offset < *group_end {
                    let end = (offset + driver.chunk_size).min(*group_end);
                    let size = end - offset;
                    compute_reconstruct_chunk(
                        work, scratch, offset, end, size, present, input_data, err_locs, ifft_plan,
                        fft_plan, tables, driver,
                    )?;
                    for (k, &(_, work_idx, inv_err)) in missing.iter().enumerate() {
                        super::fused::work_mul_to_user(
                            &work.lane(work_idx)[..size],
                            &mut group_out[k][(offset - start) * 2..(end - start) * 2],
                            &mut scratch[..size],
                            inv_err,
                            tables,
                        );
                    }
                    offset = end;
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
    #[cfg(not(feature = "std"))]
    #[test]
    fn no_std_decode_workspace_does_not_allocate_conversion_scratch() {
        let buffers = super::DecodeBuffers::new(16, 32 * 1024);
        assert_eq!(buffers.scratch.capacity(), 0);
    }

    #[cfg(all(target_arch = "aarch64", not(feature = "std")))]
    #[test]
    fn locator_reduction_matches_division_for_all_inputs_at_edge_factors() {
        for factor in [0u16, 1, 2, 3, 255, 256, 257, 32767, 32768, 65534, 65535] {
            for value in 0..=u16::MAX {
                let expected = ((u64::from(value) * u64::from(factor)) % 65535) as u16;
                assert_eq!(super::mul_locator_mod16(value, factor), expected);
            }
        }
    }

    #[test]
    fn error_locator_matches_wide_product_reference() {
        use super::{MODULUS16, ORDER16};
        let tables = super::init_leopard_gf16_tables();
        for (data_count, parity_count, missing_data, missing_parity) in [
            (10, 4, &[0, 3][..], &[10][..]),
            (16, 8, &[1, 15][..], &[19, 23][..]),
            (100, 27, &[0, 49, 99][..], &[100, 126][..]),
            (65532, 4, &[0, 32767, 65531][..], &[65535][..]),
        ] {
            let driver =
                super::build_leopard_gf16_decode_driver(data_count, parity_count, 64).unwrap();
            let actual = super::compute_error_locs16(missing_parity, missing_data, &driver, tables);
            let mut expected = super::alloc::vec![0u16; ORDER16];
            for &i in missing_parity {
                expected[i - data_count] = 1;
            }
            for value in &mut expected[parity_count..driver.m] {
                *value = 1;
            }
            for &i in missing_data {
                expected[driver.m + i] = 1;
            }
            super::fwht16_mtrunc(&mut expected, ORDER16, driver.work_size);
            for (value, &factor) in expected.iter_mut().zip(tables.log_walsh.iter()) {
                *value = ((u64::from(*value) * u64::from(factor)) % MODULUS16 as u64) as u16;
            }
            super::fwht16_variable(&mut expected);
            assert_eq!(
                actual.as_slice(),
                expected.as_slice(),
                "data={data_count}, parity={parity_count}"
            );
        }
    }

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
    fn decode_workspace_retention_preserves_small_buffers_across_oversized_calls() {
        assert!(!cacheable_decode_size(usize::MAX, usize::MAX));
        assert!(!cacheable_decode_size(64, 32 * 1024));
        DECODE_BUFFER_CACHE.with(|cache| *cache.borrow_mut() = None);
        with_decode_buffers(16, 64, |buffers| {
            assert_eq!(buffers.retained_bytes(), (16 + 1) * 64 * 2);
        });
        let warmed = DECODE_BUFFER_CACHE
            .with(|cache| cache.borrow().as_ref().unwrap().work.lane(0).as_ptr());
        // FlatWork alone fits exactly; conversion/scaling scratch puts it over cap.
        with_decode_buffers(64, 32 * 1024, |buffers| {
            assert!(buffers.retained_bytes() > MAX_CACHED_DECODE_BYTES);
        });
        DECODE_BUFFER_CACHE.with(|cache| {
            let cache = cache.borrow();
            let buffers = cache.as_ref().unwrap();
            assert_eq!(buffers.work.lane(0).as_ptr(), warmed);
            assert_eq!(buffers.retained_bytes(), (16 + 1) * 64 * 2);
        });
        with_decode_buffers(16, 64, |buffers| {
            assert_eq!(buffers.work.lane(0).as_ptr(), warmed);
        });
    }

    #[test]
    fn decode_retention_rechecks_actual_scratch_capacity() {
        DECODE_BUFFER_CACHE.with(|cache| *cache.borrow_mut() = None);
        // Equality is permitted, but any capacity growth must release the pair.
        assert!(cacheable_decode_size(31, 64 * 1024));
        with_decode_buffers(31, 64 * 1024, |buffers| {
            assert_eq!(buffers.retained_bytes(), MAX_CACHED_DECODE_BYTES);
        });
        DECODE_BUFFER_CACHE.with(|cache| assert!(cache.borrow().is_some()));
        with_decode_buffers(31, 64 * 1024, |buffers| {
            buffers.scratch.reserve_exact(1);
            assert!(buffers.retained_bytes() > MAX_CACHED_DECODE_BYTES);
        });
        DECODE_BUFFER_CACHE.with(|cache| assert!(cache.borrow().is_none()));
    }
}

#[cfg(all(test, feature = "std"))]
mod decode_plan_budget_tests {
    use super::*;
    use alloc::sync::Arc;

    fn plan(data: usize, missing: usize) -> (DecodePlanKey, Arc<LeopardGf16DecodePlan>) {
        let driver = build_leopard_gf16_decode_driver(data, 4, 64).unwrap();
        let mut present = SmallVec::from_elem(true, data + 4);
        present[missing] = false;
        let plan = build_decode_plan(
            &driver,
            &present,
            &[],
            &[missing],
            init_leopard_gf16_tables(),
        );
        ((data, 4, present), Arc::new(plan))
    }

    #[test]
    fn plan_budget_evicts_lru_and_charges_duplicate_inserts_once() {
        let (first_key, first) = plan(10, 0);
        let (second_key, second) = plan(10, 1);
        let (third_key, third) = plan(10, 2);
        let entry_bytes = first.retained_bytes() + core::mem::size_of::<DecodePlanKey>();
        assert_eq!(first.retained_bytes(), second.retained_bytes());
        assert_eq!(first.retained_bytes(), third.retained_bytes());
        let mut cache = DecodePlanCache::new(entry_bytes * 2);
        cache.insert(first_key.clone(), first.clone());
        cache.insert(second_key.clone(), second);
        assert!(Arc::ptr_eq(&cache.get(&first_key).unwrap(), &first));
        cache.insert(third_key.clone(), third);
        assert!(cache.get(&second_key).is_none());
        let (_, duplicate) = plan(10, 0);
        assert!(Arc::ptr_eq(
            &cache.insert(first_key.clone(), duplicate),
            &first
        ));
        assert_eq!(cache.retained_bytes, entry_bytes * 2);
        assert_eq!(cache.entries.len(), 2);
        assert!(cache.get(&third_key).is_some());
    }

    #[test]
    fn oversized_plan_preserves_hot_entries_and_spilled_keys_are_charged() {
        let (small_key, small) = plan(10, 0);
        let small_bytes = small.retained_bytes() + core::mem::size_of::<DecodePlanKey>();
        let mut cache = DecodePlanCache::new(small_bytes);
        cache.insert(small_key.clone(), small.clone());
        let (large_key, large) = plan(300, 0);
        assert!(large_key.2.spilled());
        assert!(large.retained_bytes() > small_bytes);
        assert!(Arc::ptr_eq(
            &cache.insert(large_key.clone(), large.clone()),
            &large
        ));
        assert!(cache.get(&large_key).is_none());
        assert!(Arc::ptr_eq(&cache.get(&small_key).unwrap(), &small));
        assert_eq!(cache.retained_bytes, small_bytes);

        let exact_key = large_key.clone();
        let large_bytes =
            large.retained_bytes() + core::mem::size_of::<DecodePlanKey>() + exact_key.2.capacity();
        let mut exact = DecodePlanCache::new(large_bytes);
        exact.insert(exact_key, large.clone());
        assert_eq!(exact.retained_bytes, large_bytes);
        let mut short = DecodePlanCache::new(large_bytes - 1);
        short.insert(large_key.clone(), large);
        assert_eq!(short.retained_bytes, 0);
    }
}

#[cfg(test)]
mod sparse_locator_tests {
    use super::*;
    use crate::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
    use alloc::{vec, vec::Vec};

    #[test]
    fn sparse_locator_matches_full_transform_for_varied_erasures() {
        let tables = init_leopard_gf16_tables();
        for (data, parity) in [
            (1usize, 1usize),
            (4, 2),
            (10, 4),
            (16, 8),
            (20, 3),
            (63, 5),
            (150, 3),
            (250, 3),
        ] {
            let driver = build_leopard_gf16_decode_driver(data, parity, 64).unwrap();
            for seed in 0..12usize {
                let missing = 1 + seed % parity.min(4);
                let mut positions: Vec<usize> = (0..data + parity).collect();
                for j in 0..missing {
                    let pick =
                        j + (seed.wrapping_mul(7919).wrapping_add(j * 101)) % (positions.len() - j);
                    positions.swap(j, pick);
                }
                let mut missing_data = vec![];
                let mut missing_parity = vec![];
                for &idx in &positions[..missing] {
                    if idx < data {
                        missing_data.push(idx);
                    } else {
                        missing_parity.push(idx);
                    }
                }
                let got = sparse_error_locs16(&missing_parity, &missing_data, &driver, tables);
                let expected =
                    compute_error_locs16(&missing_parity, &missing_data, &driver, tables);
                for (i, (&actual, &reference)) in got[..driver.work_size]
                    .iter()
                    .zip(&expected[..driver.work_size])
                    .enumerate()
                {
                    assert_eq!(
                        u32::from(actual) % MODULUS16 as u32,
                        u32::from(reference) % MODULUS16 as u32,
                        "data={data}, parity={parity}, seed={seed}, i={i}"
                    );
                }
            }
        }
    }

    #[test]
    fn plan_selector_preserves_values_at_thresholds() {
        let tables = init_leopard_gf16_tables();
        for (data, parity, missing) in [
            (224usize, 17usize, 1usize),
            (224, 17, 2),
            (225, 17, 1),
            (32, 16, 16),
            (248, 8, 8),
        ] {
            let driver = build_leopard_gf16_decode_driver(data, parity, 64).unwrap();
            let missing_data: Vec<usize> = (0..missing).map(|i| i * 3 % data).collect();
            let missing_parity: Vec<usize> = vec![];
            let mut present = vec![true; data + parity];
            for &i in &missing_data {
                present[i] = false;
            }
            let plan = build_decode_plan(&driver, &present, &missing_parity, &missing_data, tables);
            let expected = compute_error_locs16(&missing_parity, &missing_data, &driver, tables);
            for (i, (&actual, &reference)) in plan.err_locs[..driver.work_size]
                .iter()
                .zip(&expected[..driver.work_size])
                .enumerate()
            {
                assert_eq!(
                    u32::from(actual) % MODULUS16 as u32,
                    u32::from(reference) % MODULUS16 as u32,
                    "data={data}, parity={parity}, missing={missing}, i={i}"
                );
            }
            #[cfg(feature = "std")]
            {
                assert_eq!(plan.err_locs.len(), driver.work_size);
                assert_eq!(
                    plan.err_locs.spilled(),
                    driver.work_size > SPARSE_LOCATOR_MAX_WORK_SIZE
                );
                let terms = missing + driver.m - parity;
                if driver.work_size > SPARSE_LOCATOR_MAX_WORK_SIZE
                    || terms > SPARSE_LOCATOR_MAX_TERMS
                {
                    assert_eq!(plan.err_locs.as_slice(), &expected[..driver.work_size]);
                }
            }
            #[cfg(not(feature = "std"))]
            {
                let terms = missing + driver.m - parity;
                if driver.work_size <= SPARSE_LOCATOR_MAX_WORK_SIZE
                    && terms <= SPARSE_LOCATOR_MAX_TERMS
                {
                    assert!(plan.err_locs[driver.work_size..].iter().all(|&v| v == 0));
                } else {
                    assert_eq!(plan.err_locs.as_slice(), expected.as_slice());
                }
            }
        }
    }

    #[test]
    fn canonical_zero_preserves_recovered_parity() {
        let tables = init_leopard_gf16_tables();
        let driver = build_leopard_gf16_decode_driver(224, 32, 64).unwrap();
        let codec = ReedSolomon::with_options(
            224,
            32,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let mut shards: Vec<Vec<u8>> = (0..256)
            .map(|i| (0..64).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
            .collect();
        codec.encode(&mut shards).unwrap();
        // Index zero is respectively missing and present in these cases, so
        // output inverse scaling and present-input scaling both exercise the
        // alternative representation of zero.
        for missing_parity in [[224usize, 225], [226, 227]] {
            let full = compute_error_locs16(&missing_parity, &[], &driver, tables);
            let sparse = sparse_error_locs16(&missing_parity, &[], &driver, tables);
            assert_eq!(full[0], MODULUS16 as u16);
            assert_eq!(sparse[0], 0);
            #[cfg(feature = "std")]
            {
                let mut present = vec![true; 256];
                for &idx in &missing_parity {
                    present[idx] = false;
                }
                let plan = build_decode_plan(&driver, &present, &missing_parity, &[], tables);
                let expected = if present[224] { MODULUS16 as u16 } else { 0 };
                assert_eq!(plan.err_locs[0], expected);
            }
            let mut missing: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
            for idx in missing_parity {
                missing[idx] = None;
            }
            codec.reconstruct(&mut missing).unwrap();
            assert_eq!(
                missing,
                shards.iter().cloned().map(Some).collect::<Vec<_>>()
            );
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod sparse_plan_concurrency_tests {
    use crate::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
    use std::sync::{Arc, Barrier};

    #[test]
    fn parallel_chunks_recover_exact_bytes_with_reused_sparse_plan() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        pool.install(|| {
            let codec = ReedSolomon::with_options(
                10,
                4,
                CodecOptions::builder()
                    .codec_family(CodecFamily::LeopardGF16)
                    .max_parallel_jobs(4)
                    .build(),
            )
            .unwrap();
            let size = 65536 + 64;
            let mut source: Vec<Vec<u8>> = (0..14)
                .map(|i| {
                    (0..size)
                        .map(|j| (i * 31 + j * 17 + j / 11) as u8)
                        .collect()
                })
                .collect();
            codec.encode(&mut source).unwrap();
            let mut slots: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
            for _ in 0..2 {
                for slot in &mut slots[..4] {
                    slot.mark_missing();
                }
                codec.reconstruct(&mut slots).unwrap();
                assert!(
                    slots
                        .iter()
                        .zip(&source)
                        .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
                );
            }
        });
    }

    #[test]
    fn concurrent_cached_and_distinct_patterns_recover_exact_bytes() {
        let codec = Arc::new(
            ReedSolomon::with_options(
                10,
                4,
                CodecOptions::builder()
                    .codec_family(CodecFamily::LeopardGF16)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap(),
        );
        let mut source: Vec<Vec<u8>> = (0..14)
            .map(|i| (0..64).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
            .collect();
        codec.encode(&mut source).unwrap();
        let source = Arc::new(source);
        let barrier = Arc::new(Barrier::new(24));
        std::thread::scope(|scope| {
            for worker in 0..24usize {
                let codec = Arc::clone(&codec);
                let source = Arc::clone(&source);
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let mut slots: Vec<_> =
                        source.iter().cloned().map(ShardSlot::new_present).collect();
                    barrier.wait();
                    for iteration in 0..4usize {
                        if worker < 8 {
                            slots[0].mark_missing();
                        } else {
                            let pattern = (worker - 8 + iteration * 7) % 40;
                            slots[pattern % 10].mark_missing();
                            slots[10 + pattern / 10].mark_missing();
                        }
                        codec.reconstruct(&mut slots).unwrap();
                        assert!(slots.iter().zip(source.iter()).all(|(slot, expected)| {
                            slot.is_present() && slot.as_inner() == expected
                        }));
                    }
                });
            }
        });
    }
}
