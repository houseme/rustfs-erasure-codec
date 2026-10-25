extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
use crate::core::leopard::leopard_work_cache_fits;
use crate::errors::Error;

use super::ops::{
    fft_dit2_16, fft_dit4_16, get_pair_mut_16, ifft_dit2_16, ifft_dit4_16, slice_xor_u16,
};
use super::work::FlatWork16;

use super::{
    FftDit16Plan, IfftDit16Plan, LeopardGf16EncodeDriver, LeopardGf16Tables, build_fft_dit16_plan,
    build_ifft_dit16_plan, build_leopard_gf16_encode_driver, init_leopard_gf16_tables,
};

#[cfg(feature = "std")]
thread_local! {
    static FLAT_WORK16_CACHE: std::cell::RefCell<Option<FlatWork16>> =
        const { std::cell::RefCell::new(None) };
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
    first_ifft_plan: IfftDit16Plan,
    fft_plan: FftDit16Plan,
    later_ifft_plans: Vec<IfftDit16Plan>,
    remainder_ifft_plan: Option<IfftDit16Plan>,
}

impl EncodePlans {
    fn new(
        data_shards: usize,
        parity_shards: usize,
        driver: &LeopardGf16EncodeDriver,
        tables: &LeopardGf16Tables,
    ) -> Self {
        let skew = &tables.fft_skew[driver.skew_offset..];
        let first_ifft_plan = build_ifft_dit16_plan(driver.mtrunc, driver.m, skew);
        let fft_plan = build_fft_dit16_plan(parity_shards, driver.m, &tables.fft_skew);
        let mut later_ifft_plans = Vec::new();
        let mut remainder_ifft_plan = None;
        if driver.m < data_shards {
            let mut group_offset = driver.m;
            let mut skew_offset = driver.m;
            while group_offset + driver.m <= data_shards {
                later_ifft_plans.push(build_ifft_dit16_plan(
                    driver.m,
                    driver.m,
                    &skew[skew_offset..],
                ));
                group_offset += driver.m;
                skew_offset += driver.m;
            }
            if driver.last_count != 0 {
                remainder_ifft_plan = Some(build_ifft_dit16_plan(
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
        fn ifft_bytes(plan: &IfftDit16Plan) -> usize {
            (plan.initial_blocks.capacity() + plan.later_blocks.capacity())
                * core::mem::size_of::<super::Stage4Block>()
        }
        self.fft_plan.stage4_blocks.capacity() * core::mem::size_of::<super::Stage4Block>()
            + self.fft_plan.final_stage.capacity() * core::mem::size_of::<super::Stage2Block>()
            + ifft_bytes(&self.first_ifft_plan)
            + self.later_ifft_plans.capacity() * core::mem::size_of::<IfftDit16Plan>()
            + self.later_ifft_plans.iter().map(ifft_bytes).sum::<usize>()
            + self.remainder_ifft_plan.as_ref().map_or(0, ifft_bytes)
    }
}

#[cfg(feature = "std")]
thread_local! {
    static ENCODE_PLAN_CACHE: std::cell::RefCell<Option<EncodePlans>> =
        const { std::cell::RefCell::new(None) };
}

// The u16 butterflies operate directly on work lanes and need no scratch.
// Check the actual work allocation before returning it to the thread cache.
#[cfg(feature = "std")]
fn cache_encode_work(work: FlatWork16) {
    if leopard_work_cache_fits(work.allocated_bytes(), 0) {
        FLAT_WORK16_CACHE.with(|cache| *cache.borrow_mut() = Some(work));
    }
}

pub(crate) fn encode_with_tables16<T: AsRef<[u8]>, U: AsRef<[u8]> + AsMut<[u8]>>(
    data_shards: usize,
    parity_shards: usize,
    data: &[T],
    parity: &mut [U],
) -> Result<LeopardGf16EncodeDriver, Error> {
    let tables = init_leopard_gf16_tables();
    if data.len() != data_shards || parity.len() != parity_shards {
        return Err(Error::TooFewShards);
    }
    let shard_size = data
        .first()
        .map(|shard| shard.as_ref().len())
        .ok_or(Error::TooFewShards)?;
    let driver = build_leopard_gf16_encode_driver(data_shards, parity_shards, shard_size)?;

    let shard_u16_len = shard_size / 2;

    let chunk_cap = core::cmp::min(shard_u16_len, driver.chunk_size);
    let needed_lanes = driver.work_slices;
    let needed_lane_len = chunk_cap;

    // An oversized call uses only local resources, preserving the normal warm
    // configuration (including its immutable plan) in this thread's caches.
    #[cfg(feature = "std")]
    let cacheable = leopard_work_cache_fits(
        needed_lanes
            .saturating_mul(needed_lane_len)
            .saturating_mul(core::mem::size_of::<u16>()),
        0,
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

    #[cfg(feature = "std")]
    let cached_work = if cacheable {
        FLAT_WORK16_CACHE.with(|cache| cache.take())
    } else {
        None
    };
    #[cfg(feature = "std")]
    let mut flat_work = cached_work
        .filter(|work| work.can_reuse(needed_lanes, needed_lane_len))
        .unwrap_or_else(|| FlatWork16::new(needed_lanes, needed_lane_len));
    #[cfg(not(feature = "std"))]
    let mut flat_work = FlatWork16::new(needed_lanes, needed_lane_len);

    // Each IFFT group initializes every active lane: input lanes are converted
    // directly from bytes, partial radix-4 groups and trailing lanes are zeroed.
    // All chunk boundaries are multiples of 32 elements (64 user bytes), so the
    // per-chunk conversion preserves the split layout without full-shard copies.

    let work_size = core::cmp::min(driver.m * 2, flat_work.lanes());
    // The callback owns these disjoint views for the whole operation. Only the
    // final short chunk narrows them; the FlatWork metadata keeps its capacity.
    flat_work.with_lane_views(work_size, chunk_cap, |work_views| {
        let mut offset = 0usize;
        while offset < shard_u16_len {
            let end = core::cmp::min(offset + driver.chunk_size, shard_u16_len);
            let size = end - offset;
            if size < chunk_cap {
                debug_assert_eq!(end, shard_u16_len);
                super::super::leopard::truncate_lane_views(work_views, size);
            }

            ifft_dit_encoder16_with_plan(
                data,
                &plans.first_ifft_plan,
                &mut work_views[..driver.m],
                None,
                offset,
                end,
                tables,
            );

            let mut group_offset = driver.m;
            for plan in &plans.later_ifft_plans {
                let (xor_dst, temp_work) = work_views[..work_size].split_at_mut(driver.m);
                ifft_dit_encoder16_with_plan(
                    &data[group_offset..],
                    plan,
                    temp_work,
                    Some(xor_dst),
                    offset,
                    end,
                    tables,
                );
                group_offset += driver.m;
            }

            if let Some(plan) = plans.remainder_ifft_plan.as_ref() {
                let (xor_dst, temp_work) = work_views[..work_size].split_at_mut(driver.m);
                ifft_dit_encoder16_with_plan(
                    &data[group_offset..],
                    plan,
                    temp_work,
                    Some(xor_dst),
                    offset,
                    end,
                    tables,
                );
            }

            fft_dit16_with_plan(&mut work_views[..driver.m], &plans.fft_plan, tables);

            // Write parity shards straight into user byte layout: the fused
            // de-interleave skips the intermediate split-byte buffer and the
            // second whole-shard pass the split->user conversion used to need.
            for (idx, output) in parity.iter_mut().enumerate() {
                let out_bytes = output.as_mut();
                super::ops::work_u16_to_user_bytes(
                    &work_views[idx][..size],
                    &mut out_bytes[offset * 2..end * 2],
                );
            }
            offset = end;
        }
    });

    #[cfg(feature = "std")]
    if cacheable {
        cache_encode_work(flat_work);
    }

    #[cfg(feature = "std")]
    if cacheable && plans.retained_bytes() <= MAX_CACHED_PLAN_BYTES {
        ENCODE_PLAN_CACHE.with(|cache| {
            *cache.borrow_mut() = Some(plans);
        });
    }

    Ok(driver)
}

fn zero_trailing_lanes_16(work: &mut [&mut [u16]], start_lane: usize, count: usize) {
    for i in start_lane..start_lane + count {
        if i < work.len() {
            work[i].fill(0);
        }
    }
}

fn fft_dit16_with_plan(work: &mut [&mut [u16]], plan: &FftDit16Plan, tables: &LeopardGf16Tables) {
    for block in &plan.stage4_blocks {
        dit4_at_16(
            true,
            work,
            block.r,
            block.dist,
            block.log_m01,
            block.log_m23,
            block.log_m02,
            tables,
        );
    }

    for stage in &plan.final_stage {
        let r = stage.r;
        if r + stage.dist < work.len() {
            let (left, right) = work[r..r + stage.dist + 1].split_at_mut(stage.dist);
            fft_dit2_16(left[0], right[0], stage.log_m, tables);
        }
    }
}

fn ifft_dit_encoder16_with_plan<T: AsRef<[u8]>>(
    data: &[T],
    plan: &IfftDit16Plan,
    work: &mut [&mut [u16]],
    mut xor_dst: Option<&mut [&mut [u16]]>,
    start: usize,
    end: usize,
    tables: &LeopardGf16Tables,
) {
    let size = end - start;

    if plan.initial_blocks.is_empty() {
        for (idx, slot) in work.iter_mut().take(plan.mtrunc).enumerate() {
            super::ops::user_bytes_to_work_u16_into(
                &data[idx].as_ref()[start * 2..end * 2],
                &mut slot[..size],
            );
        }
        zero_trailing_lanes_16(work, plan.mtrunc, plan.m - plan.mtrunc);
    } else {
        for block in &plan.initial_blocks {
            let available = core::cmp::min(plan.mtrunc.saturating_sub(block.r), 4);
            for i in 0..available {
                super::ops::user_bytes_to_work_u16_into(
                    &data[block.r + i].as_ref()[start * 2..end * 2],
                    &mut work[block.r + i][..size],
                );
            }
            for slot in work
                .iter_mut()
                .skip(block.r + available)
                .take(4usize.saturating_sub(available))
            {
                slot[..size].fill(0);
            }

            dit4_at_16(
                false,
                work,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
            );
        }

        zero_trailing_lanes_16(
            work,
            plan.clear_start,
            plan.m.saturating_sub(plan.clear_start),
        );

        for block in &plan.later_blocks {
            dit4_at_16(
                false,
                work,
                block.r,
                block.dist,
                block.log_m01,
                block.log_m23,
                block.log_m02,
                tables,
            );
        }
    }

    if let Some(stage) = plan.final_stage {
        for i in 0..stage.dist {
            if i + stage.dist < work.len() {
                let (left, right) = work[i..i + stage.dist + 1].split_at_mut(stage.dist);
                ifft_dit2_16(left[0], right[0], stage.log_m, tables);
            }
        }
    }

    if let Some(xor_dst) = xor_dst.as_mut() {
        for idx in 0..plan.m {
            if idx < work.len() && idx < xor_dst.len() {
                slice_xor_u16(xor_dst[idx], work[idx]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dit4_at_16(
    forward: bool,
    work: &mut [&mut [u16]],
    base: usize,
    dist: usize,
    log_m01: u16,
    log_m23: u16,
    log_m02: u16,
    tables: &LeopardGf16Tables,
) {
    for i in 0..dist {
        let a = base + i;
        let b = a + dist;
        let c = a + dist * 2;
        let d = a + dist * 3;

        if d < work.len() {
            // SAFETY: d < work.len() (checked above) and a<b<c<d are distinct (dist >= 1), so reading the four &mut [u16] elements through the raw *mut and reborrowing yields four disjoint, non-aliasing lanes.
            let (a_ref, b_ref, c_ref, d_ref) = unsafe {
                let ptr = work.as_mut_ptr();
                let a_ref = &mut *(*ptr.add(a));
                let b_ref = &mut *(*ptr.add(b));
                let c_ref = &mut *(*ptr.add(c));
                let d_ref = &mut *(*ptr.add(d));
                (a_ref, b_ref, c_ref, d_ref)
            };
            if forward {
                fft_dit4_16(
                    a_ref, b_ref, c_ref, d_ref, log_m01, log_m23, log_m02, tables,
                );
            } else {
                ifft_dit4_16(
                    a_ref, b_ref, c_ref, d_ref, log_m01, log_m23, log_m02, tables,
                );
            }
        } else {
            dit4_pairwise_16(forward, work, a, dist, log_m01, log_m23, log_m02, tables);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dit4_pairwise_16(
    forward: bool,
    work: &mut [&mut [u16]],
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
    let has_a = a < work.len();
    let has_b = b < work.len();
    let has_c = c < work.len();
    let has_d = d < work.len();

    if forward {
        if has_a
            && has_c
            && let Some((r1, r2)) = get_pair_mut_16(work, a, c)
        {
            fft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_b
            && has_d
            && let Some((r1, r2)) = get_pair_mut_16(work, b, d)
        {
            fft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_a
            && has_b
            && let Some((r1, r2)) = get_pair_mut_16(work, a, b)
        {
            fft_dit2_16(r1, r2, log_m01, tables);
        }
        if has_c
            && has_d
            && let Some((r1, r2)) = get_pair_mut_16(work, c, d)
        {
            fft_dit2_16(r1, r2, log_m23, tables);
        }
    } else {
        if has_a
            && has_b
            && let Some((r1, r2)) = get_pair_mut_16(work, a, b)
        {
            ifft_dit2_16(r1, r2, log_m01, tables);
        }
        if has_c
            && has_d
            && let Some((r1, r2)) = get_pair_mut_16(work, c, d)
        {
            ifft_dit2_16(r1, r2, log_m23, tables);
        }
        if has_a
            && has_c
            && let Some((r1, r2)) = get_pair_mut_16(work, a, c)
        {
            ifft_dit2_16(r1, r2, log_m02, tables);
        }
        if has_b
            && has_d
            && let Some((r1, r2)) = get_pair_mut_16(work, b, d)
        {
            ifft_dit2_16(r1, r2, log_m02, tables);
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod encode_cache_budget_tests {
    use super::*;
    use crate::core::leopard::LEOPARD_MAX_CACHED_WORK_BYTES;

    fn clear_caches() {
        FLAT_WORK16_CACHE.with(|cache| *cache.borrow_mut() = None);
        ENCODE_PLAN_CACHE.with(|cache| *cache.borrow_mut() = None);
    }

    fn small_cache_pointers() -> (*const u8, *const u8) {
        let work = FLAT_WORK16_CACHE.with(|cache| {
            let cache = cache.borrow();
            let work = cache.as_ref().unwrap();
            assert!(work.can_reuse(8, 32));
            work.lane(0).as_ptr().cast::<u8>()
        });
        let plan = ENCODE_PLAN_CACHE.with(|cache| {
            let cache = cache.borrow();
            let plan = cache.as_ref().unwrap();
            assert_eq!((plan.data_shards, plan.parity_shards), (10, 4));
            plan.first_ifft_plan.initial_blocks.as_ptr().cast::<u8>()
        });
        (work, plan)
    }

    #[test]
    fn oversized_encode_preserves_small_warm_buffers_and_plan() {
        clear_caches();
        let data: Vec<_> = (0..10).map(|index| vec![index as u8; 64]).collect();
        let mut parity = vec![vec![0; 64]; 4];
        encode_with_tables16(10, 4, &data, &mut parity).unwrap();
        let expected = parity.clone();
        let warmed = small_cache_pointers();
        encode_with_tables16(10, 4, &data, &mut parity).unwrap();
        assert_eq!(small_cache_pointers(), warmed);

        // The active work allocation exceeds the budget, so none of the
        // oversized call's resources may replace the small warm cache.
        let large_data = vec![vec![0x5a; 65536]; 4];
        let mut large_parity = vec![vec![0; 65536]; 64];
        let driver = encode_with_tables16(4, 64, &large_data, &mut large_parity).unwrap();
        let lane_len = (driver.shard_size / core::mem::size_of::<u16>()).min(driver.chunk_size);
        let work_bytes = driver.work_slices * lane_len * core::mem::size_of::<u16>();
        assert!(work_bytes > LEOPARD_MAX_CACHED_WORK_BYTES);
        assert!(!leopard_work_cache_fits(work_bytes, 0));
        assert_eq!(small_cache_pointers(), warmed);

        parity.iter_mut().for_each(|shard| shard.fill(0xa5));
        encode_with_tables16(10, 4, &data, &mut parity).unwrap();
        assert_eq!(parity, expected);
        assert_eq!(small_cache_pointers(), warmed);
    }

    #[test]
    fn cache_return_accounts_for_the_actual_work_allocation() {
        clear_caches();
        let exact = FlatWork16::new(64, 32 * 1024);
        assert_eq!(exact.allocated_bytes(), LEOPARD_MAX_CACHED_WORK_BYTES);
        cache_encode_work(exact);
        FLAT_WORK16_CACHE.with(|cache| {
            assert_eq!(
                cache.borrow().as_ref().unwrap().allocated_bytes(),
                LEOPARD_MAX_CACHED_WORK_BYTES
            );
        });

        clear_caches();
        let oversized = FlatWork16::new(64, 32 * 1024 + 32);
        assert!(oversized.allocated_bytes() > LEOPARD_MAX_CACHED_WORK_BYTES);
        cache_encode_work(oversized);
        FLAT_WORK16_CACHE.with(|cache| assert!(cache.borrow().is_none()));
    }
}
