//! Verification request-volume catalog; no timing or release acceptance.
use crate::allocation::Counts;
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
    Gf8,
    TowerGf16,
}
impl FieldKind {
    const ALL: [Self; 2] = [Self::Gf8, Self::TowerGf16];
    fn name(self) -> &'static str {
        match self {
            Self::Gf8 => "gf8",
            Self::TowerGf16 => "tower-gf16",
        }
    }
    fn bytes(self) -> usize {
        match self {
            Self::Gf8 => 1,
            Self::TowerGf16 => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub data: usize,
    pub parity: usize,
    pub elements: usize,
}
const CATALOG: [Config; 6] = [
    Config {
        data: 4,
        parity: 2,
        elements: 1024,
    },
    Config {
        data: 4,
        parity: 2,
        elements: 16384,
    },
    Config {
        data: 10,
        parity: 4,
        elements: 32768,
    },
    Config {
        data: 4,
        parity: 2,
        elements: 1024 * 1024,
    },
    Config {
        data: 10,
        parity: 4,
        elements: 1024 * 1024,
    },
    Config {
        data: 2,
        parity: 33,
        elements: 65,
    },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Api {
    Ordinary,
    Buffer,
    Workspace,
}
impl Api {
    pub const ALL: [Self; 3] = [Self::Ordinary, Self::Buffer, Self::Workspace];
    fn name(self) -> &'static str {
        match self {
            Self::Ordinary => "verify",
            Self::Buffer => "verify_with_buffer",
            Self::Workspace => "verify_with_workspace",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Damage {
    Valid,
    Head,
    Tail,
}
impl Damage {
    pub const ALL: [Self; 3] = [Self::Valid, Self::Head, Self::Tail];
    fn name(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Head => "parity_head",
            Self::Tail => "parity_tail",
        }
    }
    pub fn valid(self) -> bool {
        self == Self::Valid
    }
    pub fn position(self, c: Config) -> Option<(usize, usize)> {
        match self {
            Self::Valid => None,
            Self::Head => Some((c.data, 0)),
            Self::Tail => Some((c.data + c.parity - 1, c.elements - 1)),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Sample {
    pub valid: bool,
    pub counts: Counts,
}
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub api: Api,
    pub damage: Damage,
    pub corruption: Option<(usize, usize)>,
    pub samples: Vec<Sample>,
}
#[derive(Clone, Debug)]
pub(crate) struct Fixture {
    pub field: FieldKind,
    pub config: Config,
    pub element_bytes: usize,
    pub parity_bytes: Vec<Vec<u8>>,
    pub observations: Vec<Observation>,
}

fn validate(f: &Fixture, field: FieldKind, c: Config) -> Result<(), &'static str> {
    if c.data == 0
        || c.parity == 0
        || c.elements == 0
        || f.field != field
        || f.config != c
        || f.element_bytes != field.bytes()
    {
        return Err("fixture identity or element units differ");
    }
    if f.parity_bytes.len() != c.parity
        || f.parity_bytes
            .iter()
            .any(|row| row.len() != c.elements * field.bytes())
    {
        return Err("incomplete parity witness");
    }
    if f.observations.len() != 9 {
        return Err("incomplete verification catalog");
    }
    for (index, row) in f.observations.iter().enumerate() {
        let damage = Damage::ALL[index / 3];
        if row.api != Api::ALL[index % 3]
            || row.damage != damage
            || row.corruption != damage.position(c)
            || row.samples.len() != 3
        {
            return Err("verification case order, damage or sample count differs");
        }
        if row
            .samples
            .iter()
            .any(|sample| sample.valid != damage.valid())
        {
            return Err("verification result differs from corruption oracle");
        }
    }
    Ok(())
}
fn pair(
    before: &Fixture,
    after: &Fixture,
    field: FieldKind,
    c: Config,
) -> Result<(), &'static str> {
    validate(before, field, c)?;
    validate(after, field, c)?;
    if before.parity_bytes != after.parity_bytes {
        return Err("published/current parity differs");
    }
    Ok(())
}
fn samples(rows: &[Sample]) -> String {
    rows.iter()
        .map(|sample| {
            format!(
                "{{\"valid\":{},\"requests\":{}}}",
                sample.valid, sample.counts
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn fixture_json(before: &Fixture, after: &Fixture) -> String {
    let c = before.config;
    let rows = before.observations.iter().zip(&after.observations).map(|(a, b)| {
        let position = match a.corruption {
            Some((row, offset)) => format!("{{\"shard_index\":{row},\"element_offset\":{offset}}}"),
            None => "null".to_owned(),
        };
        format!("{{\"api\":\"{}\",\"condition\":\"{}\",\"corruption\":{},\"expected_valid\":{},\"baseline\":[{}],\"candidate\":[{}]}}", a.api.name(), a.damage.name(), position, a.damage.valid(), samples(&a.samples), samples(&b.samples))
    }).collect::<Vec<_>>().join(",");
    // Identifiers are fixed enum labels; JSON values are numeric/boolean. No
    // user path or arbitrary string is interpolated into this report.
    format!(
        "{{\"field\":\"{}\",\"data\":{},\"parity\":{},\"elements_per_shard\":{},\"element_bytes\":{},\"shard_bytes\":{},\"parity_equal\":true,\"observations\":[{}]}}",
        before.field.name(),
        c.data,
        c.parity,
        c.elements,
        before.element_bytes,
        c.elements * before.element_bytes,
        rows
    )
}
pub(crate) fn write(path: &Path) {
    // Refuse an existing destination before the optional heavy catalog runs.
    let mut output = std::fs::File::create_new(path).expect("preserve an existing verify report");
    let mut fixtures = Vec::with_capacity(CATALOG.len() * 2);
    for field in FieldKind::ALL {
        for c in CATALOG {
            let before = crate::old::verify_fixture(field, c);
            let after = crate::new::verify_fixture(field, c);
            pair(&before, &after, field, c)
                .expect("complete verification fixture and parity witnesses");
            fixtures.push(fixture_json(&before, &after));
            // Full parity witnesses are compared and dropped per fixture,
            // rather than retaining nine full copies for the API/damage rows.
        }
    }
    let fixture_count = fixtures.len();
    let observation_cases = fixture_count * Api::ALL.len() * Damage::ALL.len();
    let counted_windows = observation_cases * 3 * 2;
    let architecture = std::env::consts::ARCH;
    let baseline_backend = codec_old::galois_8::active_backend_name();
    let candidate_backend = codec_new::galois_8::active_backend_name();
    let debug_assertions = cfg!(debug_assertions);
    let fixtures_json = fixtures.join(",");
    let report = format!(
        r#"{{"schema":"verify-workload-resources-v1","baseline_version":"9.0.0","candidate_version":"9.1.0","architecture":"{architecture}","baseline_backend":"{baseline_backend}","candidate_backend":"{candidate_backend}","build_debug_assertions":{debug_assertions},"fixture_count":{fixture_count},"observation_cases":{observation_cases},"counted_windows":{counted_windows},"warmups_per_case_per_version":2,"samples_per_case_per_version":3,"family":"Classic","matrix_mode":"Vandermonde","fast_one_parity":false,"max_parallel_jobs":1,"scope":"serial APIs; caller buffer and workspace prepared outside counting; global allocator includes all threads","bytes_policy":"sum of allocation sizes and full realloc targets; not RSS or peak live bytes","latency_claim":null,"release_approval":false,"fixtures":[{fixtures_json}]}}"#
    );

    output.write_all(report.as_bytes()).unwrap();
    println!(
        "{observation_cases} verification cases / {counted_windows} counted windows: valid, parity head/tail corruption and published/current parity passed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_keeps_downstream_small_large_and_wide_cases_distinct() {
        assert!(CATALOG.contains(&Config {
            data: 4,
            parity: 2,
            elements: 16384
        }));
        assert!(CATALOG.contains(&Config {
            data: 10,
            parity: 4,
            elements: 32768
        }));
        assert!(CATALOG.iter().any(|c| c.elements >= 1024 * 1024));
        assert!(CATALOG.iter().any(|c| c.parity > 32));
        for (index, c) in CATALOG.iter().enumerate() {
            assert!(!CATALOG[index + 1..].contains(c));
        }
    }
    #[test]
    fn real_fixture_gate_rejects_forged_results_order_units_damage_and_witnesses() {
        let c = Config {
            data: 4,
            parity: 2,
            elements: 65,
        };
        for field in FieldKind::ALL {
            let before = crate::old::verify_fixture(field, c);
            let after = crate::new::verify_fixture(field, c);
            assert!(pair(&before, &after, field, c).is_ok());
            assert_eq!(after.observations[6].corruption, Some((5, 64)));
            let mut bad = after.clone();
            bad.observations.pop();
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.observations[3].samples[0].valid = true;
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.observations.swap(0, 1);
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.observations[6].corruption = Some((4, 0));
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.observations[0].samples.pop();
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.element_bytes += 1;
            assert!(pair(&before, &bad, field, c).is_err());
            let mut bad = after.clone();
            bad.parity_bytes[0][0] ^= 1;
            assert!(pair(&before, &bad, field, c).is_err());
        }
    }
}
