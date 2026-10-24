// Baseline-only placement diagnosis; the measuring thread pins itself after construction.
#[derive(Clone, Copy)]
pub(super) struct PlacementFaults {
    minor: u64,
    major: u64,
    cpu: u64,
}

fn parse_placement_faults(stat: &str) -> PlacementFaults {
    let mut fields = stat.rsplit_once(") ").unwrap().1.split_whitespace();
    let minor = fields.nth(7).unwrap().parse().unwrap();
    let major = fields.nth(1).unwrap().parse().unwrap();
    let cpu = fields.nth(26).unwrap().parse().unwrap();
    PlacementFaults { minor, major, cpu }
}

pub(super) fn placement_faults() -> PlacementFaults {
    let mut buffer = [0u8; 4096];
    parse_placement_faults(
        super::sampling::read_bounded("/proc/thread-self/stat", &mut buffer).unwrap(),
    )
}

impl std::fmt::Display for PlacementFaults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"minor\":{},\"major\":{},\"cpu\":{}}}",
            self.minor, self.major, self.cpu
        )
    }
}

fn pin_measuring_thread(cpu: usize) {
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn sched_setaffinity(pid: i32, bytes: usize, mask: *const usize) -> i32;
        }
        let mut mask = [0usize; 1024 / usize::BITS as usize];
        assert!(cpu < 1024);
        mask[cpu / usize::BITS as usize] |= 1usize << (cpu % usize::BITS as usize);
        // SAFETY: PID zero changes only this calling thread. The initialized,
        // aligned mask remains live for the exact byte length passed to libc.
        assert_eq!(
            unsafe { sched_setaffinity(0, std::mem::size_of_val(&mask), mask.as_ptr()) },
            0
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cpu;
        panic!("placement capture requires Linux");
    }
}

pub(super) fn cpu_placement_diagnostic() {
    use std::io::{Read, Write};
    let count = std::env::var("RSE_CPU_ITERATIONS")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!((1..=1_000_000).contains(&count));
    let page_size = std::env::var("RSE_SAMPLER_PAGE_SIZE")
        .unwrap()
        .parse()
        .unwrap();
    let cpus: Vec<_> = std::env::var("RSE_CPU_SEQUENCE")
        .unwrap()
        .split(',')
        .map(|c| c.parse::<usize>().unwrap())
        .collect();
    assert!(!cpus.is_empty() && cpus.len() <= 128);
    let tid = std::fs::read_link("/proc/thread-self")
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let budget_before = std::thread::available_parallelism().unwrap().get();
    let case = BenchCase {
        data_shards: 10,
        parity_shards: 4,
        shard_size: 524288,
        label: "10x4_512k",
    };
    let mut prepared =
        PreparedSmallFile::new(case, SmallFileOp::Standard(Operation::Encode), page_size);
    let budget_after = std::thread::available_parallelism().unwrap().get();
    assert_eq!(budget_before, budget_after);
    assert!(
        budget_before > 1,
        "codec must be constructed with the complete reservation"
    );
    // This is the cached decision for an explicit parallel API, not evidence that
    // ordinary encode uses Rayon. The selected ordinary Classic path is serial.
    let policy = prepared
        .codec
        .parallel_policy(case.shard_size, case.parity_shards);
    let backend = super::bench_common::backend();
    for (epoch, cpu) in cpus.into_iter().enumerate() {
        println!(
            "\nRSE_CPU_READY {{\"epoch\":{epoch},\"cpu\":{cpu},\"tid\":{tid},\"budget_before\":{budget_before},\"budget_after\":{budget_after},\"hypothetical_parallel_policy\":{{\"use_parallel\":{},\"jobs\":{},\"chunk_len\":{}}}}}",
            policy.use_parallel, policy.jobs, policy.chunk_len
        );
        std::io::stdout().flush().unwrap();
        let mut command = [0u8; 1];
        std::io::stdin().read_exact(&mut command).unwrap();
        assert_eq!(command, [b'G']);
        pin_measuring_thread(cpu);
        assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
        assert_eq!(
            prepared
                .codec
                .parallel_policy(case.shard_size, case.parity_shards),
            policy
        );
        for index in 0..12 {
            let before = super::sampling::telemetry();
            let faults_before = placement_faults();
            assert_eq!(faults_before.cpu, cpu as u64);
            let start = Instant::now();
            let output = prepared.run(count);
            let wall_ns = start.elapsed().as_nanos();
            let faults_after = placement_faults();
            let after = super::sampling::telemetry();
            assert_eq!(faults_after.cpu, cpu as u64);
            let ns_per_iter = output.result.ns_per_iter;
            let kind = if index < 3 { "warmup" } else { "sample" };
            let fixture = output.fixture;
            println!(
                "\nRSE_CPU_SAMPLE {{\"epoch\":{epoch},\"cpu\":{cpu},\"index\":{index},\"kind\":\"{kind}\",\"iterations\":{count},\"case\":\"encode:10:4:524288\",\"call_entry\":\"Vec<Vec<u8>>\",\"backend\":\"{backend}\",\"ns_per_iter\":{ns_per_iter},\"wall_ns\":{wall_ns},\"timed_ns\":{},\"fixture\":{fixture},\"before\":{before},\"after\":{after},\"faults_before\":{faults_before},\"faults_after\":{faults_after}}}",
                ns_per_iter * count as f64
            );
        }
        assert_eq!(
            prepared
                .codec
                .parallel_policy(case.shard_size, case.parity_shards),
            policy
        );
        println!(
            "\nRSE_CPU_END {{\"epoch\":{epoch},\"cpu\":{cpu},\"cached_policy_unchanged\":true,\"available_parallelism_while_pinned\":1}}"
        );
        // Keep this exact thread alive until the parent verifies its END affinity.
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_exact(&mut command).unwrap();
        assert_eq!(command, [b'E']);
    }
    assert!(prepared.codec.verify(&prepared.source).unwrap());
}

#[test]
fn placement_stat_fields_follow_linux_field_numbers() {
    let fields = (3..=52)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let snapshot = parse_placement_faults(&format!("1 (name with ) parenthesis) {fields}"));
    assert_eq!((snapshot.minor, snapshot.major, snapshot.cpu), (10, 12, 39));
}

#[test]
#[cfg(target_os = "linux")]
fn placement_real_counters_are_monotonic() {
    let first = placement_faults();
    let second = placement_faults();
    assert!(second.minor >= first.minor && second.major >= first.major);
}

#[test]
#[cfg(target_os = "linux")]
fn placement_self_pin_keeps_the_parent_affinity_unchanged() {
    let before = std::fs::read_to_string("/proc/thread-self/status").unwrap();
    let affinity = |text: &str| {
        text.lines()
            .find(|line| line.starts_with("Cpus_allowed_list:"))
            .unwrap()
            .to_owned()
    };
    let original = affinity(&before);
    let cpu = original
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(['-', ','])
        .next()
        .unwrap()
        .parse()
        .unwrap();
    std::thread::spawn(move || {
        pin_measuring_thread(cpu);
        assert_eq!(std::thread::available_parallelism().unwrap().get(), 1);
    })
    .join()
    .unwrap();
    assert_eq!(
        affinity(&std::fs::read_to_string("/proc/thread-self/status").unwrap()),
        original
    );
}
