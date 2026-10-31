use std::{
    hint::black_box,
    time::{Duration, Instant},
};
fn measure(mut operation: impl FnMut()) -> ([f64; 3], [u64; 3]) {
    for _ in 0..20 {
        operation();
    }
    let mut samples = [0.0; 3];
    let mut counts = [0u64; 3];
    for (value, count) in samples.iter_mut().zip(&mut counts) {
        let begin = Instant::now();
        let mut iterations = 0u64;
        loop {
            for _ in 0..8 {
                operation();
            }
            iterations += 8;
            if begin.elapsed() >= Duration::from_millis(150) {
                break;
            }
        }
        *value = begin.elapsed().as_secs_f64() * 1e9 / iterations as f64;
        *count = iterations;
    }
    (samples, counts)
}
fn main() {
    assert!(
        std::env::var_os("RSE_BACKEND_OVERRIDE").is_none(),
        "unset RSE_BACKEND_OVERRIDE for generated-kernel comparisons"
    );
    assert_eq!(base_codec::galois_8::active_backend_name(), "rust-avx2");
    assert_eq!(next_codec::galois_8::active_backend_name(), "rust-avx2");
    for (data, parity) in [(6, 2), (10, 4)] {
        for len in [256, 1024, 4095, 4096, 8192, 32768] {
            let before = base_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
            let after = next_codec::galois_8::ReedSolomon::new(data, parity).unwrap();
            let original: Vec<Vec<u8>> = (0..data + parity)
                .map(|row| {
                    (0..len)
                        .map(|offset| (row * 73 + offset * 19 + offset / 17) as u8)
                        .collect()
                })
                .collect();
            let mut a = original.clone();
            let mut b = original;
            before.encode(&mut a).unwrap();
            after.encode(&mut b).unwrap();
            assert_eq!(a, b);
            assert!(before.verify(&a).unwrap());
            assert!(after.verify(&b).unwrap());
            for shard in &mut a[data..] {
                for byte in shard {
                    *byte ^= 0xff
                }
            }
            for shard in &mut b[data..] {
                for byte in shard {
                    *byte ^= 0xff
                }
            }
            before.encode(&mut a).unwrap();
            after.encode(&mut b).unwrap();
            assert_eq!(a, b);
            drop(b);
            let a1 = measure(|| before.encode(black_box(&mut a)).unwrap());
            let b1 = measure(|| after.encode(black_box(&mut a)).unwrap());
            let b2 = measure(|| after.encode(black_box(&mut a)).unwrap());
            let a2 = measure(|| before.encode(black_box(&mut a)).unwrap());
            assert!(before.verify(&a).unwrap());
            assert!(after.verify(&a).unwrap());
            println!(
                "{{\"case_id\":\"encode.serial/{data}x{parity}/{len}\",\"A1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B1\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"B2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}},\"A2\":{{\"samples_ns\":{:?},\"iterations\":{:?}}}}}",
                a1.0, a1.1, b1.0, b1.1, b2.0, b2.1, a2.0, a2.1
            );
        }
    }
}
