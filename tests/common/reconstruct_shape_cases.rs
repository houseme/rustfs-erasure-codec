//! Synthetic shape catalog shared by resource probes and planning benchmarks.
//! It describes coverage, not a measured production workload distribution.

use rustfs_erasure_codec::Error;

#[derive(Clone, Debug)]
pub struct Case {
    pub name: &'static str,
    pub data: usize,
    pub parity: usize,
    pub missing: Vec<usize>,
    pub bad_length: Option<usize>,
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for data in [4, 32, 33, 64, 65, 96] {
        for (name, missing, bad_length) in [
            ("present", vec![], None),
            ("head", vec![0], None),
            ("middle", vec![data / 2], None),
            ("tail", vec![data + 3], None),
            ("sparse", vec![0, data / 2, data + 1], None),
            ("quorum", (0..5).collect(), None),
            ("short", vec![0], Some(64)),
            ("empty", vec![0], Some(0)),
            ("quorum_short", (0..5).collect(), Some(64)),
        ] {
            cases.push(Case {
                name,
                data,
                parity: 4,
                missing,
                bad_length,
            });
        }
    }
    for (name, missing, bad_length) in [
        ("wide_missing", (4..69).collect(), None),
        (
            "wide_mixed",
            std::iter::once(0).chain(4..69).collect(),
            None,
        ),
        ("wide_quorum", (0..101).collect(), None),
        ("wide_short", (4..100).collect(), Some(64)),
    ] {
        cases.push(Case {
            name,
            data: 4,
            parity: 97,
            missing,
            bad_length,
        });
    }
    cases
}
pub fn expected_error(case: &Case) -> Option<Error> {
    match case.bad_length {
        Some(0) => Some(Error::EmptyShard),
        Some(_) => Some(Error::IncorrectShardSize),
        None if case.missing.len() > case.parity => Some(Error::TooFewShardsPresent),
        None => None,
    }
}
