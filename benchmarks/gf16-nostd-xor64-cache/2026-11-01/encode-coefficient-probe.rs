
#[cfg(test)]
mod encode_coefficient_probe {
    extern crate std;
    use super::*;
    use super::super::MODULUS16;
    fn count(blocks: &[super::super::Stage4Block]) -> [usize; 8] {
        let mut counts = [0usize; 8];
        for b in blocks {
            let mask = usize::from(b.log_m01 == MODULUS16 as u16)
                | (usize::from(b.log_m23 == MODULUS16 as u16) << 1)
                | (usize::from(b.log_m02 == MODULUS16 as u16) << 2);
            counts[mask] += 1;
        }
        counts
    }
    #[test]
    fn observe_encoding_masks() {
        let tables = init_leopard_gf16_tables();
        for (data, parity) in [(4, 2), (10, 4), (16, 8), (32, 16)] {
            let driver = build_leopard_gf16_encode_driver(data, parity, 64).unwrap();
            let plans = EncodePlans::new(data, parity, &driver, tables);
            std::println!("{data}+{parity} m={} first={:?} first_later={:?} fft={:?}",
                driver.m, count(&plans.first_ifft_plan.initial_blocks),
                count(&plans.first_ifft_plan.later_blocks), count(&plans.fft_plan.stage4_blocks));
            for p in &plans.later_ifft_plans {
                std::println!("  next={:?} next_later={:?}", count(&p.initial_blocks), count(&p.later_blocks));
            }
            if let Some(p) = &plans.remainder_ifft_plan {
                std::println!("  remainder={:?}", count(&p.initial_blocks));
            }
        }
    }
}
