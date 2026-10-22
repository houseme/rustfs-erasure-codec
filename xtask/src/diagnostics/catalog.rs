pub const HOTSPOT_BLOCKS: &[&str] = &[
    "struct ReconstructionHotspotBenchResult",
    "fn benchmark_test_iterations",
    "macro_rules! make_random_shards",
    "pub fn fill_random",
    "fn shards_to_option_shards",
    "fn option_shards_with_missing",
    "fn shard_index_pattern",
    "fn assert_required_reconstruction_matches",
    "fn bench_reconstruct_data_hotspot",
    "fn bench_reconstruct_some_hotspot",
    "fn write_reconstruction_hotspot_bench_results",
];
pub const SMALL_KERNELS: &[(&str, &str)] = &[
    ("SmallFileOp::Standard(Operation::Encode)", "encode"),
    ("SmallFileOp::Standard(Operation::Verify)", "verify"),
    ("SmallFileOp::VerifyWithBuffer", "verify_with_buffer"),
    (
        "SmallFileOp::Standard(Operation::Reconstruct)",
        "reconstruct",
    ),
    ("SmallFileOp::ReconstructOpt", "reconstruct_opt"),
    (
        "SmallFileOp::ReconstructShardSlot",
        "reconstruct_shard_slot",
    ),
    (
        "SmallFileOp::ReconstructSomeDataOnly",
        "reconstruct_some_data_only",
    ),
    (
        "SmallFileOp::Standard(Operation::ReconstructData)",
        "reconstruct_data",
    ),
];
pub const STREAM_KERNELS: &[(&str, &str)] = &[
    ("StreamOperation::Encode", "encode_stream"),
    ("StreamOperation::Verify", "verify_stream"),
    ("StreamOperation::Reconstruct", "reconstruct_stream"),
];
pub struct HotspotCase {
    pub data: usize,
    pub parity: usize,
    pub name: &'static str,
    pub missing: &'static [usize],
    pub required: Option<&'static [usize]>,
}
pub const HOTSPOT_CASES: &[HotspotCase] = &[
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_data_missing_1_data",
        missing: &[0],
        required: None,
    },
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_data_missing_2_data",
        missing: &[0, 2],
        required: None,
    },
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_data_missing_data_plus_parity",
        missing: &[1, 10],
        required: None,
    },
    HotspotCase {
        data: 32,
        parity: 16,
        name: "reconstruct_data_32x16_missing_2_data",
        missing: &[0, 2],
        required: None,
    },
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_some_required_1_of_2_missing_data",
        missing: &[0, 2],
        required: Some(&[2]),
    },
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_some_required_2_of_3_missing_data",
        missing: &[0, 2, 4],
        required: Some(&[0, 4]),
    },
    HotspotCase {
        data: 10,
        parity: 4,
        name: "reconstruct_some_required_data_and_skip_parity",
        missing: &[1, 10],
        required: Some(&[1]),
    },
    HotspotCase {
        data: 32,
        parity: 16,
        name: "reconstruct_some_32x16_required_2_of_4_missing_data",
        missing: &[0, 2, 4, 6],
        required: Some(&[2, 6]),
    },
];

/// One mask model supplies both emitted Rust expressions and replayed values.
#[derive(Clone, Copy)]
pub enum Mask {
    None,
    First,
    FirstTwo,
    FirstDataAndParity,
    Data,
    All,
}
impl Mask {
    pub fn expression(self) -> &'static str {
        match self {
            Self::None => "0",
            Self::First => "1",
            Self::FirstTwo => "3",
            Self::FirstDataAndParity => "1 | (1 << case.data_shards)",
            Self::Data => "(1 << case.data_shards)-1",
            Self::All => "(1 << (case.data_shards+case.parity_shards))-1",
        }
    }
    pub fn value(self, data: u32, parity: u32) -> u64 {
        match self {
            Self::None => 0,
            Self::First => 1,
            Self::FirstTwo => 3,
            Self::FirstDataAndParity => 1 | (1 << data),
            Self::Data => (1 << data) - 1,
            Self::All => (1 << (data + parity)) - 1,
        }
    }
}

pub struct FixtureContract {
    pub entry: &'static str,
    pub policy: &'static str,
    pub missing: Mask,
    pub required: Mask,
}

pub fn fixture(operation: &str) -> crate::Result<FixtureContract> {
    use Mask::*;
    let (entry, policy, missing, required) = match operation {
        "encode" => ("Vec<Vec<u8>>", "caller-parity-in-place", None, None),
        "verify" => ("Vec<Vec<u8>>", "internal-scratch-untracked", None, None),
        "verify_with_buffer" => ("Vec<Vec<u8>>", "retained-caller-scratch", None, None),
        "reconstruct" | "reconstruct_opt" => (
            "Vec<Option<Vec<u8>>>",
            "iteration-clones-drop-missing-new-output-untracked",
            FirstDataAndParity,
            All,
        ),
        "reconstruct_shard_slot" => (
            "Vec<ShardSlot<Vec<u8>>>",
            "iteration-clones-retain-missing-storage-untracked",
            FirstDataAndParity,
            All,
        ),
        "reconstruct_some_data_only" => (
            "Vec<Option<Vec<u8>>>",
            "iteration-clones-drop-missing-new-output-untracked",
            FirstDataAndParity,
            First,
        ),
        "reconstruct_data" => (
            "Vec<Option<Vec<u8>>>",
            "iteration-clones-drop-missing-new-output-untracked",
            FirstTwo,
            Data,
        ),
        "encode_stream" => (
            "Vec<&[u8]> -> Vec<Vec<u8>>",
            "iteration-reserved-writers-untracked",
            None,
            None,
        ),
        "verify_stream" => (
            "Vec<&[u8]>",
            "iteration-readers-internal-scratch-untracked",
            None,
            None,
        ),
        "reconstruct_stream" => (
            "Vec<Cursor<Vec<u8>>>",
            "iteration-cloned-cursors-empty-missing-untracked",
            FirstDataAndParity,
            All,
        ),
        _ => return Err("unexpected caller-fixture operation".into()),
    };
    Ok(FixtureContract {
        entry,
        policy,
        missing,
        required,
    })
}
