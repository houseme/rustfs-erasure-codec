extern crate alloc;

use alloc::vec::Vec;

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

#[cfg(feature = "std")]
thread_local! {
    static SCRATCH16_CACHE: std::cell::RefCell<Option<Vec<u16>>> =
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

    #[cfg(feature = "std")]
    let plans = ENCODE_PLAN_CACHE.with(|cache| {
        cache
            .take()
            .filter(|plan| plan.data_shards == data_shards && plan.parity_shards == parity_shards)
            .unwrap_or_else(|| EncodePlans::new(data_shards, parity_shards, &driver, tables))
    });
    #[cfg(not(feature = "std"))]
    let plans = EncodePlans::new(data_shards, parity_shards, &driver, tables);

    let chunk_cap = core::cmp::min(shard_u16_len, driver.chunk_size);
    let needed_lanes = driver.work_slices;
    let needed_lane_len = chunk_cap;

    #[cfg(feature = "std")]
    let mut flat_work = FLAT_WORK16_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(fw) = cache.take() {
            if fw.can_reuse(needed_lanes, needed_lane_len) {
                return fw;
            }
            drop(fw);
        }
        FlatWork16::new(needed_lanes, needed_lane_len)
    });
    #[cfg(not(feature = "std"))]
    let mut flat_work = FlatWork16::new(needed_lanes, needed_lane_len);

    #[cfg(feature = "std")]
    let mut scratch = SCRATCH16_CACHE.with(|cache| cache.take().unwrap_or_default());
    #[cfg(not(feature = "std"))]
    let mut scratch: Vec<u16> = Vec::new();

    // Each IFFT group initializes every active lane: input lanes are converted
    // directly from bytes, partial radix-4 groups and trailing lanes are zeroed.
    // All chunk boundaries are multiples of 32 elements (64 user bytes), so the
    // per-chunk conversion preserves the split layout without full-shard copies.

    let mut offset = 0usize;
    while offset < shard_u16_len {
        let end = core::cmp::min(offset + driver.chunk_size, shard_u16_len);
        let size = end - offset;
        let work_size = core::cmp::min(driver.m * 2, flat_work.lanes());

        if scratch.len() < size {
            scratch.resize(size, 0);
        }

        // Build mutable views into flat work using with_lane_views.
        flat_work.with_lane_views(work_size, size, |work_views| {
            ifft_dit_encoder16_with_plan(
                data,
                &plans.first_ifft_plan,
                &mut work_views[..driver.m],
                None,
                offset,
                end,
                tables,
                &mut scratch,
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
                    &mut scratch,
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
                    &mut scratch,
                );
            }

            fft_dit16_with_plan(
                &mut work_views[..driver.m],
                &plans.fft_plan,
                tables,
                &mut scratch,
            );

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
        });
        offset = end;
    }

    #[cfg(feature = "std")]
    SCRATCH16_CACHE.with(|cache| {
        *cache.borrow_mut() = Some(scratch);
    });
    #[cfg(feature = "std")]
    FLAT_WORK16_CACHE.with(|cache| {
        *cache.borrow_mut() = Some(flat_work);
    });

    #[cfg(feature = "std")]
    if plans.retained_bytes() <= MAX_CACHED_PLAN_BYTES {
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

fn fft_dit16_with_plan(
    work: &mut [&mut [u16]],
    plan: &FftDit16Plan,
    tables: &LeopardGf16Tables,
    scratch: &mut [u16],
) {
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
            scratch,
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

#[allow(clippy::too_many_arguments)]
fn ifft_dit_encoder16_with_plan<T: AsRef<[u8]>>(
    data: &[T],
    plan: &IfftDit16Plan,
    work: &mut [&mut [u16]],
    mut xor_dst: Option<&mut [&mut [u16]]>,
    start: usize,
    end: usize,
    tables: &LeopardGf16Tables,
    scratch: &mut [u16],
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
                scratch,
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
                scratch,
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
    scratch: &mut [u16],
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
            let _ = scratch;
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
