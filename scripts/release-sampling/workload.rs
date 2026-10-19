//! Fixed-storage execution contracts. Full addresses and allocator hooks stay private.
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::fmt;

type Contract = (&'static str, &'static str, u64, u64);
const BASE_SEED: u64 = 0x00EC_5EED_2026_0524;

pub(super) fn hotspot_seed(name: &str, data: usize, parity: usize, size: usize) -> u64 {
    let hash = name.bytes().fold(0xcbf29ce484222325u64, |state, byte| {
        (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    BASE_SEED ^ 0x44 ^ ((data as u64) << 48) ^ ((parity as u64) << 32) ^ size as u64 ^ hash
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RngPoint {
    pub(super) bytes: u64,
    pub(super) next: [u64; 2],
}

pub(super) struct InputRng {
    rng: SmallRng,
    bytes: u64,
}
impl InputRng {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            rng: SmallRng::seed_from_u64(seed),
            bytes: 0,
        }
    }
    pub(super) fn fill(&mut self, output: &mut [u8]) {
        for byte in output.iter_mut() {
            *byte = self.rng.next_u32() as u8;
        }
        self.bytes = self.bytes.checked_add(output.len() as u64).unwrap();
    }
    pub(super) fn point(&self) -> RngPoint {
        let mut copy = self.rng.clone();
        RngPoint {
            bytes: self.bytes,
            next: [copy.next_u64(), copy.next_u64()],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RngTrace {
    before: RngPoint,
    after: RngPoint,
}
impl RngTrace {
    pub(super) fn new(before: RngPoint, after: RngPoint, expected_bytes: u64) -> Self {
        assert_eq!(after.bytes - before.bytes, expected_bytes);
        Self { before, after }
    }
}
impl fmt::Display for RngTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"bytes_before\":{},\"bytes_after\":{},\"next_before\":{:?},\"next_after\":{:?}}}",
            self.before.bytes, self.after.bytes, self.before.next, self.after.next
        )
    }
}

#[derive(Clone, Copy)]
pub(super) struct Workload {
    contract: Contract,
    seed: u64,
    source_lifetime: &'static str,
    construction: &'static str,
    generator: &'static str,
    changed_mask: u64,
    rng: Option<RngTrace>,
}
impl Workload {
    pub(super) fn retained(seed: u64, contract: Contract) -> Self {
        Self {
            contract,
            seed,
            source_lifetime: "worker",
            construction: "source-before-codec",
            generator: "worker-seeded-SmallRng-random-u8",
            changed_mask: 0,
            rng: None,
        }
    }
    pub(super) fn generated(seed: u64, contract: Contract, changed_mask: u64) -> Self {
        Self {
            contract,
            seed,
            source_lifetime: "iteration",
            construction: "codec-before-iteration-source",
            generator: "per-iteration-seeded-SmallRng-random-u8",
            changed_mask,
            rng: None,
        }
    }
    pub(super) fn hotspot(seed: u64, contract: Contract, trace: RngTrace) -> Self {
        Self {
            contract,
            seed,
            source_lifetime: "iteration",
            construction: "codec-before-iteration-source",
            generator: "worker-seeded-SmallRng-u32-low-byte",
            changed_mask: 0,
            rng: Some(trace),
        }
    }
}
impl fmt::Display for Workload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"codec_lifetime\":\"worker\",\"source_lifetime\":\"{}\",\"construction\":\"{}\",\"generator\":\"{}\",\"seed\":{},\"call_entry\":\"{}\",\"output_policy\":\"{}\",\"missing_mask\":{},\"required_mask\":{},\"changed_mask\":{},\"rng\":",
            self.source_lifetime,
            self.construction,
            self.generator,
            self.seed,
            self.contract.0,
            self.contract.1,
            self.contract.2,
            self.contract.3,
            self.changed_mask
        )?;
        match self.rng {
            Some(trace) => trace.fmt(f)?,
            None => f.write_str("null")?,
        }
        f.write_str("}")
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct OptionalWorkload(pub(super) Option<Workload>);
impl fmt::Display for OptionalWorkload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(workload) => workload.fmt(f),
            None => f.write_str("null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngExt;
    #[test]
    fn input_preview_does_not_consume_the_stream_and_replays_exact_bytes() {
        for seed in [0, 1, u64::MAX, hotspot_seed("case", 10, 4, 1061)] {
            let mut input = InputRng::new(seed);
            let first = input.point();
            assert_eq!(first, input.point());
            let mut reference = SmallRng::seed_from_u64(seed);
            for length in [1, 3, 1061, 4096] {
                let mut bytes = vec![0u8; length];
                input.fill(&mut bytes);
                for byte in bytes {
                    assert_eq!(byte, reference.random::<u8>());
                }
                let next = input.point();
                let mut copy = reference.clone();
                assert_eq!(next.next, [copy.next_u64(), copy.next_u64()]);
            }
            assert_eq!(input.point().bytes, 5161);
        }
    }
}
