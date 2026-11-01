
#[cfg(test)]
mod small_cost_probe {
    extern crate std;
    use super::*;
    use std::{hint::black_box, time::Instant};
    use alloc::{vec, vec::Vec};

    #[test]
    #[ignore]
    fn report_small_encode_costs() {
        let tables = init_leopard_gf16_tables();
        for (data_count, parity_count) in [(10usize, 4usize), (16, 8)] {
            for size in [64usize, 4096] {
                let rounds = if size == 64 { 12000 } else { 1200 };
                let data: Vec<Vec<u8>> = (0..data_count)
                    .map(|i| (0..size).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                    .collect();
                let mut parity = vec![vec![0u8; size]; parity_count];
                let driver = build_leopard_gf16_encode_driver(data_count, parity_count, size).unwrap();
                let plans = EncodePlans::new(data_count, parity_count, &driver, tables);
                let len = size / 2;
                let lanes = driver.work_slices;
                let nwork = core::cmp::min(driver.m * 2, lanes);
                let ns = |start: Instant| start.elapsed().as_nanos() as f64 / rounds as f64;
                let t = Instant::now();
                for _ in 0..rounds {
                    black_box(EncodePlans::new(black_box(data_count), black_box(parity_count), black_box(&driver), black_box(tables)));
                }
                let plans_ns = ns(t);
                let t = Instant::now();
                for _ in 0..rounds { black_box(FlatWork16::new(black_box(lanes), black_box(len))); }
                let alloc_ns = ns(t);
                let mut work = FlatWork16::new(lanes, len);
                work.with_lane_views(nwork, len, |views| {
                    let t = Instant::now();
                    for _ in 0..rounds {
                        ifft_dit_encoder16_with_plan(black_box(&data), &plans.first_ifft_plan, &mut views[..driver.m], None, 0, len, tables);
                        let mut offset = driver.m;
                        for plan in &plans.later_ifft_plans {
                            let (dst, tmp) = views.split_at_mut(driver.m);
                            ifft_dit_encoder16_with_plan(&data[offset..], plan, tmp, Some(dst), 0, len, tables);
                            offset += driver.m;
                        }
                        if let Some(plan) = plans.remainder_ifft_plan.as_ref() {
                            let (dst, tmp) = views.split_at_mut(driver.m);
                            ifft_dit_encoder16_with_plan(&data[offset..], plan, tmp, Some(dst), 0, len, tables);
                        }
                        black_box(&*views);
                    }
                    let ifft_ns = ns(t);
                    let t = Instant::now();
                    for _ in 0..rounds { fft_dit16_with_plan(black_box(&mut views[..driver.m]), &plans.fft_plan, tables); }
                    let fft_ns = ns(t);
                    let t = Instant::now();
                    for _ in 0..rounds {
                        for (i, out) in parity.iter_mut().enumerate() {
                            super::super::ops::work_u16_to_user_bytes(black_box(&views[i]), black_box(out));
                        }
                    }
                    let output_ns = ns(t);
                    let t = Instant::now();
                    for _ in 0..rounds { encode_with_tables16(data_count, parity_count, black_box(&data), black_box(&mut parity)).unwrap(); }
                    let full_ns = ns(t);
                    std::println!("{data_count}+{parity_count} size={size} rounds={rounds} plans_ns={plans_ns:.1} work_ns={alloc_ns:.1} ifft_ns={ifft_ns:.1} fft_ns={fft_ns:.1} output_ns={output_ns:.1} full_ns={full_ns:.1}");
                });
            }
        }
    }
}
