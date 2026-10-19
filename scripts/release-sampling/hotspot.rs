#[derive(Clone, Copy)]
struct HotspotCase {
    data: usize,
    parity: usize,
    name: &'static str,
    missing: &'static [usize],
    required: Option<&'static [usize]>,
}
const HOTSPOT_CASES: &[HotspotCase] = &[__HOTSPOT_CASES__];

pub(super) fn release_profile() {
    for &case in HOTSPOT_CASES {
        super::sampling::run_prepared(
            case.name,
            "reconstruction-hotspot-results.json",
            |_| {
                let mut prepared = PreparedHotspot::new(case, 1048576);
                move |iterations| prepared.run(iterations)
            },
            |r| r.useful_shards as f64 * r.shard_size as f64 * 1e9 / (1048576.0 * r.candidate_mb_s),
            |r| write_reconstruction_hotspot_bench_results(std::slice::from_ref(r)),
        );
    }
}

struct PreparedHotspot {
    case: HotspotCase,
    shard_size: usize,
    seed: u64,
    codec: ReedSolomon,
    missing_data: Vec<usize>,
    required: Vec<bool>,
    rng: super::workload::InputRng,
}
impl PreparedHotspot {
    fn new(case: HotspotCase, shard_size: usize) -> Self {
        let seed = super::workload::hotspot_seed(case.name, case.data, case.parity, shard_size);
        let missing_data = case
            .missing
            .iter()
            .copied()
            .filter(|&i| i < case.data)
            .collect();
        let mut required = vec![false; case.data + case.parity];
        if let Some(indices) = case.required {
            for &i in indices {
                required[i] = true;
            }
        }
        Self {
            case,
            shard_size,
            seed,
            codec: ReedSolomon::new(case.data, case.parity).unwrap(),
            missing_data,
            required,
            rng: super::workload::InputRng::new(seed),
        }
    }
    fn run(
        &mut self,
        iterations: usize,
    ) -> super::sampling::Output<ReconstructionHotspotBenchResult> {
        let before = self.rng.point();
        let result = if self.case.required.is_some() {
            bench_reconstruct_some_hotspot(self, iterations)
        } else {
            bench_reconstruct_data_hotspot(self, iterations)
        };
        let after = self.rng.point();
        let trace = super::workload::RngTrace::new(
            before,
            after,
            (iterations as u64)
                .checked_mul(self.shard_size as u64)
                .unwrap()
                .checked_mul((self.case.data + self.case.parity) as u64)
                .unwrap(),
        );
        let missing = self
            .case
            .missing
            .iter()
            .fold(0u64, |mask, &i| mask | (1 << i));
        let required = self
            .case
            .required
            .map_or((1u64 << self.case.data) - 1, |indices| {
                indices.iter().fold(0u64, |mask, &i| mask | (1 << i))
            });
        let contract = (
            "Vec<Option<Vec<u8>>>",
            "iteration-random-source-cloned-baseline-and-candidate-missing-outputs-untracked",
            missing,
            required,
        );
        super::sampling::Output {
            result,
            fixture: super::layout::OptionalFixture(None),
            workload: super::workload::OptionalWorkload(Some(super::workload::Workload::hotspot(
                self.seed, contract, trace,
            ))),
        }
    }
}

#[test]
fn all_hotspot_masks_replay_inputs_without_resetting_codec_or_rng() {
    for &case in HOTSPOT_CASES {
        let mut left = PreparedHotspot::new(case, 1061);
        let mut right = PreparedHotspot::new(case, 1061);
        let identity = &left.codec as *const ReedSolomon;
        for _ in 0..12 {
            let before = left.rng.point();
            let a = left.run(2);
            let b = right.run(2);
            assert_eq!(&left.codec as *const ReedSolomon, identity);
            assert_eq!(left.rng.point(), right.rng.point());
            assert_eq!(
                left.rng.point().bytes - before.bytes,
                (2 * 1061 * (case.data + case.parity)) as u64
            );
            assert_eq!(
                a.result.missing_pattern.to_string(),
                b.result.missing_pattern.to_string()
            );
            assert_eq!(a.workload.to_string(), b.workload.to_string());
        }
    }
}

#[test]
fn hotspot_input_replay_witnesses() {
    for &case in HOTSPOT_CASES {
        let seed = super::workload::hotspot_seed(case.name, case.data, case.parity, 1048576);
        let mut rng = super::workload::InputRng::new(seed);
        let initial = rng.point();
        let mut data = [0u8; 4099];
        rng.fill(&mut data);
        let hash = data.into_iter().fold(0xcbf29ce484222325u64, |state, byte| {
            (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        let after = rng.point();
        println!(
            "\nRSE_INPUT_REPLAY {{\"case\":\"{}\",\"seed\":{seed},\"bytes\":4099,\"hash\":{hash},\"initial\":{:?},\"next\":{:?}}}",
            case.name, initial.next, after.next
        );
    }
}
