use std::{env, fs, hint::black_box, time::Instant};

#[derive(Clone, Copy)]
struct Counters {
    cpu: u64,
    run: u64,
    wait: u64,
    minor: u64,
    major: u64,
}
fn counters() -> Counters {
    let stat = fs::read_to_string("/proc/thread-self/stat").unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    let sched = fs::read_to_string("/proc/thread-self/schedstat").unwrap();
    let values: Vec<u64> = sched
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    Counters {
        cpu: fields[36].parse().unwrap(),
        run: values[0],
        wait: values[1],
        minor: fields[7].parse().unwrap(),
        major: fields[9].parse().unwrap(),
    }
}
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "gfni,avx512f,avx512bw")]
unsafe fn vector_work(iterations: u64, value: u64) -> u64 {
    use std::arch::x86_64::*;
    let base = _mm512_set_epi64(
        value as i64,
        value.rotate_left(7) as i64,
        value.rotate_left(13) as i64,
        value.rotate_left(19) as i64,
        value.rotate_left(29) as i64,
        value.rotate_left(37) as i64,
        value.rotate_left(43) as i64,
        value.rotate_left(53) as i64,
    );
    let k0 = _mm512_set1_epi8(3);
    let k1 = _mm512_set1_epi8(5);
    let k2 = _mm512_set1_epi8(7);
    let k3 = _mm512_set1_epi8(11);
    let k4 = _mm512_set1_epi8(13);
    let k5 = _mm512_set1_epi8(17);
    let k6 = _mm512_set1_epi8(19);
    let k7 = _mm512_set1_epi8(23);
    let mut s0 = _mm512_xor_si512(base, k0);
    let mut s1 = _mm512_xor_si512(base, k1);
    let mut s2 = _mm512_xor_si512(base, k2);
    let mut s3 = _mm512_xor_si512(base, k3);
    let mut s4 = _mm512_xor_si512(base, k4);
    let mut s5 = _mm512_xor_si512(base, k5);
    let mut s6 = _mm512_xor_si512(base, k6);
    let mut s7 = _mm512_xor_si512(base, k7);
    for _ in 0..iterations {
        s0 = _mm512_gf2p8mul_epi8(s0, k0);
        s1 = _mm512_gf2p8mul_epi8(s1, k1);
        s2 = _mm512_gf2p8mul_epi8(s2, k2);
        s3 = _mm512_gf2p8mul_epi8(s3, k3);
        s4 = _mm512_gf2p8mul_epi8(s4, k4);
        s5 = _mm512_gf2p8mul_epi8(s5, k5);
        s6 = _mm512_gf2p8mul_epi8(s6, k6);
        s7 = _mm512_gf2p8mul_epi8(s7, k7);
    }
    let lo = _mm512_xor_si512(_mm512_xor_si512(s0, s1), _mm512_xor_si512(s2, s3));
    let hi = _mm512_xor_si512(_mm512_xor_si512(s4, s5), _mm512_xor_si512(s6, s7));
    let mut lanes = [0u64; 8];
    // SAFETY: The array owns 64 writable bytes; the unaligned store stays in it.
    unsafe {
        _mm512_storeu_si512(lanes.as_mut_ptr().cast(), _mm512_xor_si512(lo, hi));
    }
    lanes.into_iter().fold(0, |value, lane| value ^ lane)
}
fn work(iterations: u64, value: u64) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        assert!(
            std::is_x86_feature_detected!("gfni")
                && std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
        );
        // SAFETY: Every target feature required by vector_work is checked above;
        // the only pointer operation writes its own 64-byte local array.
        return unsafe { vector_work(iterations, value) };
    }
    #[cfg(not(target_arch = "x86_64"))]
    panic!("GFNI control requires x86_64");
}
fn main() {
    let iterations = env::args().nth(1).unwrap().parse::<u64>().unwrap();
    assert!((1..=1_000_000_000).contains(&iterations));
    let mut expected = None;
    for index in 0..12 {
        let before = counters();
        let start = Instant::now();
        let value = black_box(work(black_box(iterations), black_box(0x123456789abcdef0)));
        let wall_ns = start.elapsed().as_nanos();
        let after = counters();
        if let Some(expected) = expected {
            assert_eq!(value, expected);
        } else {
            expected = Some(value);
        }
        let kind = if index < 3 { "warmup" } else { "sample" };
        println!(
            "{{\"schema_version\":\"independent-gfni-execution-control-v1\",\"kind\":\"{kind}\",\"index\":{index},\"iterations\":{iterations},\"wall_ns\":{wall_ns},\"cpu_before\":{},\"cpu_after\":{},\"run_ns\":{},\"wait_ns\":{},\"minor_faults\":{},\"major_faults\":{},\"result\":{value}}}",
            before.cpu,
            after.cpu,
            after.run - before.run,
            after.wait - before.wait,
            after.minor - before.minor,
            after.major - before.major
        );
    }
}
