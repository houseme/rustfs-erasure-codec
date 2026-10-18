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
#[inline(never)]
fn work(iterations: u64, mut value: u64) -> u64 {
    for _ in 0..iterations {
        value ^= value.rotate_left(23);
        value = value
            .wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add(0xda942042e4dd58b5);
    }
    value
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
            "{{\"schema_version\":\"independent-cpu-execution-control-v1\",\"kind\":\"{kind}\",\"index\":{index},\"iterations\":{iterations},\"wall_ns\":{wall_ns},\"cpu_before\":{},\"cpu_after\":{},\"run_ns\":{},\"wait_ns\":{},\"minor_faults\":{},\"major_faults\":{},\"result\":{value}}}",
            before.cpu,
            after.cpu,
            after.run - before.run,
            after.wait - before.wait,
            after.minor - before.minor,
            after.major - before.major
        );
    }
}
