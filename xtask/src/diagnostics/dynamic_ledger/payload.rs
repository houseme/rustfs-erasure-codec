//! One prepared payload schedule and its cross-stage ownership relationships.
use super::{Event, Kind, Region};
use crate::{Result, sampling_contract};
use std::collections::BTreeMap;

pub(super) struct Supplement {
    pub case: &'static str,
    pub family: &'static str,
    pub width: u64,
    pub cache: bool,
    lane_width: u64,
}
pub(super) const SUPPLEMENTS: [Supplement; 6] = [
    Supplement {
        case: "classic_uncached:10:4:16384",
        family: "Classic",
        width: 1,
        cache: false,
        lane_width: 1,
    },
    Supplement {
        case: "tower_gf16:10:4:65",
        family: "Classic",
        width: 2,
        cache: false,
        lane_width: 2,
    },
    Supplement {
        case: "leopard_gf8:10:4:32768",
        family: "LeopardGF8",
        width: 1,
        cache: true,
        lane_width: 1,
    },
    Supplement {
        case: "leopard_gf8:10:4:65600",
        family: "LeopardGF8",
        width: 1,
        cache: true,
        lane_width: 1,
    },
    Supplement {
        case: "leopard_gf16:10:4:32768",
        family: "LeopardGF16",
        width: 1,
        cache: true,
        lane_width: 2,
    },
    Supplement {
        case: "leopard_gf16:10:4:65600",
        family: "LeopardGF16",
        width: 1,
        cache: true,
        lane_width: 2,
    },
];
pub(super) fn supplement(case: &str) -> Option<&'static Supplement> {
    SUPPLEMENTS.iter().find(|s| s.case == case)
}
struct Group {
    role: &'static str,
    stage: &'static str,
    count: usize,
    start: usize,
}
pub(super) struct Contract {
    data: u64,
    parity: u64,
    size: u64,
    width: u64,
    lane_width: u64,
    missing: u64,
    wanted: u64,
    baseline_data_only: bool,
    operation: String,
    profile: String,
    legacy: bool,
    leopard: bool,
    groups: Vec<Group>,
}
impl Contract {
    pub(super) fn parse(profile: &str, case: &str, legacy: bool) -> Result<Self> {
        let decoder = profile == "decoder";
        if decoder && supplement(case).is_none()
            || !decoder
                && (!matches!(profile, "smoke" | "small_file" | "hotspot" | "stream")
                    || !sampling_contract::known_case(profile, case))
        {
            return Err("unknown dynamic inventory contract".into());
        }
        let parts: Vec<_> = case.split(':').collect();
        let (data, parity, size, data_only) = if profile == "hotspot" {
            let c = super::super::catalog::HOTSPOT_CASES
                .iter()
                .find(|c| c.name == case)
                .ok_or("unknown hotspot case")?;
            (
                c.data as u64,
                c.parity as u64,
                1048576,
                c.required.is_some(),
            )
        } else {
            (
                parts[1].parse()?,
                parts[2].parse()?,
                parts[3].parse()?,
                false,
            )
        };
        let (missing, wanted) = if decoder {
            (1 | (1 << data), (1 << (data + parity)) - 1)
        } else {
            let workload = sampling_contract::workload_contract(profile, case)?;
            (
                workload["missing_mask"]
                    .as_u64()
                    .ok_or("missing workload mask")?,
                workload["required_mask"]
                    .as_u64()
                    .ok_or("missing required mask")?,
            )
        };
        let operation = parts[0].to_owned();
        let recovery =
            operation.starts_with("reconstruct") || matches!(profile, "hotspot" | "decoder");
        let mut groups = Vec::new();
        let mut start = 0;
        let mut group = |role, stage, count: u64| {
            groups.push(Group {
                role,
                stage,
                count: count as usize,
                start,
            });
            start += count as usize;
        };
        let n = data + parity;
        if matches!(profile, "small_file" | "stream" | "decoder") {
            group(
                "source",
                "retained",
                if profile == "stream" && operation == "encode_stream" {
                    data
                } else {
                    n
                },
            );
            if matches!(profile, "small_file" | "decoder") {
                if operation == "verify_with_buffer" {
                    group("scratch", "retained", parity);
                }
                if recovery {
                    for stage in ["cloned", "missing", "recovered"] {
                        group("dynamic", stage, n);
                    }
                }
            }
        } else {
            group("source", "created", n);
            group("source", "encoded", n);
            if profile == "smoke" && operation == "update" {
                group("old_data", "cloned", data);
                group("updated", "changed", data);
                group("parity", "before", parity);
                group("parity", "updated", parity);
            } else if profile == "smoke" && recovery {
                for stage in ["moved", "missing", "recovered"] {
                    group("dynamic", stage, n);
                }
            } else if profile == "hotspot" {
                for role in ["baseline", "candidate"] {
                    group("clone", "cloned", n);
                    group(role, "missing", n);
                    group(role, "recovered", n);
                }
            }
        }
        Ok(Self {
            data,
            parity,
            size,
            width: supplement(case).map_or(1, |s| s.width),
            lane_width: supplement(case).map_or(1, |s| s.lane_width),
            missing,
            wanted,
            baseline_data_only: data_only,
            operation,
            profile: profile.to_owned(),
            legacy,
            leopard: decoder && case.starts_with("leopard_"),
            groups,
        })
    }
    fn recovery(&self) -> bool {
        self.operation.starts_with("reconstruct")
            || matches!(self.profile.as_str(), "hotspot" | "decoder")
    }
    fn group<'a, 'b>(
        &self,
        views: &'a [&'b Event],
        role: &str,
        stage: &str,
    ) -> Result<&'a [&'b Event]> {
        // A repeated clone group has the same logical key in the old contract;
        // its final occurrence is the one compared by cross-stage rules.
        let g = self
            .groups
            .iter()
            .rfind(|g| g.role == role && g.stage == stage)
            .ok_or("missing payload group")?;
        views
            .get(g.start..g.start + g.count)
            .ok_or_else(|| "incomplete payload group".into())
    }
    pub(super) fn internal(&self, v: &Event) -> Result<()> {
        let role = v.role.as_str();
        if role == "leopard_work" {
            let shape = v.shape.ok_or("missing Leopard lane shape")?;
            let capacity = u128::from(shape[0]) * u128::from(shape[2]);
            if !self.leopard
                || shape[0] != 16
                || shape[2] == 0
                || shape[1] > shape[2]
                || v.capacity.map(u128::from) != Some(capacity)
                || u128::from(v.len) != capacity
                || v.element_size != self.lane_width
                || !matches!(
                    v.stage.as_str(),
                    "leased" | "returned" | "chunk_ready" | "chunk_coded"
                )
                || v.stage.starts_with("chunk_") && shape[1] == 0
                || v.index != 0
                || v.present.is_some()
                || !v.occupied
                || v.storage != "heap"
            {
                return Err("invalid actual Leopard work/lane shape".into());
            }
        } else {
            if v.shape.is_some() {
                return Err("non-work view contains lane metadata".into());
            }
            if role.starts_with("leopard_") {
                if !self.leopard
                    || v.present.is_some()
                    || !v.occupied
                    || v.capacity.is_none() && !(role == "leopard_locator" && self.lane_width == 2)
                {
                    return Err("invalid internal decoder owner".into());
                }
                match role {
                    "leopard_locator" => {
                        // GF16 chunk observers receive a borrowed locator slice;
                        // the current std plan holds only the active prefix.
                        let (length, capacity, storage) = if self.lane_width == 2 {
                            let length = if self.legacy {
                                65536
                            } else {
                                self.data + self.parity.next_power_of_two()
                            };
                            (length, None, "borrowed")
                        } else {
                            (256, Some(256), "inline")
                        };
                        if v.len != length
                            || v.capacity != capacity
                            || v.element_size != self.lane_width
                            || v.storage != storage
                            || !matches!(
                                v.stage.as_str(),
                                "leased" | "returned" | "chunk_ready" | "chunk_coded"
                            )
                            || v.index != 0
                        {
                            return Err("invalid error-locator storage".into());
                        }
                    }
                    "leopard_scratch" | "leopard_out_scratch" => {
                        if v.element_size != self.lane_width
                            || v.storage != "heap"
                            || !matches!(
                                v.stage.as_str(),
                                "leased" | "returned" | "chunk_ready" | "chunk_coded"
                            )
                            || v.index != 0
                        {
                            return Err("invalid FFT conversion/scaling scratch".into());
                        }
                    }
                    "leopard_decoded" => {
                        let count = self.groups[0].count as u64;
                        let length = if self.legacy || [0, self.data].contains(&v.index) {
                            self.size
                        } else {
                            0
                        };
                        if v.index >= count
                            || v.len != length
                            || v.capacity.is_none_or(|cap| cap < length)
                            || v.element_size != 1
                            || v.storage != "heap"
                            || !matches!(v.stage.as_str(), "prepared" | "decoded")
                        {
                            return Err("invalid decoded-before-publication owner".into());
                        }
                    }
                    _ => return Err("unknown Leopard owner".into()),
                }
            }
        }
        if role == "matrix"
            && (!matches!(
                v.stage.as_str(),
                "constructed"
                    | "invert_ready"
                    | "invert_coded"
                    | "decode_submatrix"
                    | "decode_ready"
                    | "decode_cached"
            ) || v.index != 0
                || v.capacity.is_none()
                || !v.occupied
                || v.present.is_some()
                || !matches!(v.storage.as_str(), "heap" | "inline")
                || v.element_size != self.width
                || matches!(
                    v.stage.as_str(),
                    "decode_submatrix" | "decode_ready" | "decode_cached"
                ) && v.len != self.data * self.data
                || matches!(v.stage.as_str(), "invert_ready" | "invert_coded")
                    && v.len != 2 * self.data * self.data)
        {
            return Err("wrong decoder matrix storage/dimensions".into());
        }
        if matches!(role, "decode_coefficients" | "decode_missing") {
            if self.legacy
                || self.leopard
                || self.data < 64
                || !matches!(v.stage.as_str(), "decode_ready" | "decode_cached")
                || v.index != 0
                || !v.occupied
                || v.present.is_some()
            {
                return Err("invalid compact decoder metadata".into());
            }
            let valid = if role == "decode_coefficients" {
                v.len.is_multiple_of(self.data)
                    && (1..=4).contains(&(v.len / self.data))
                    && v.capacity.is_some_and(|cap| cap >= v.len)
                    && v.element_size == self.width
                    && v.storage == "heap"
            } else {
                (1..=4).contains(&v.len)
                    && v.capacity == Some(4)
                    && matches!(v.element_size, 4 | 8)
                    && v.storage == "inline"
            };
            if !valid {
                return Err("wrong compact decoder storage/dimensions".into());
            }
        }
        Ok(())
    }

    fn decoder_witness(&self, events: &[Event]) -> Result<bool> {
        let mut pending: BTreeMap<(u64, u64, &str), &Event> = BTreeMap::new();
        let mut witnessed = false;
        for view in events.iter().filter(|v| v.kind == Kind::View) {
            if view.role == "matrix" {
                witnessed |= view.call > 0
                    && matches!(view.stage.as_str(), "decode_ready" | "decode_cached")
                    && view.region == Region::Decode;
            } else if matches!(view.role.as_str(), "decode_coefficients" | "decode_missing") {
                self.internal(view)?;
                let key = (view.call, view.thread, view.stage.as_str());
                if view.role == "decode_coefficients" {
                    if pending.insert(key, view).is_some() {
                        return Err("duplicate compact decoder coefficients before indices".into());
                    }
                } else {
                    let coefficients = pending
                        .remove(&key)
                        .ok_or("compact decoder indices lack matching coefficients")?;
                    if coefficients.len / self.data != view.len
                        || coefficients.observation != view.observation
                        || coefficients.region != view.region
                    {
                        return Err("compact decoder coefficients/indices disagree".into());
                    }
                    witnessed |= view.call > 0
                        && view.observation.is_some()
                        && view.region == Region::Decode;
                }
            }
        }
        if !pending.is_empty() {
            return Err("compact decoder coefficients lack matching indices".into());
        }
        Ok(witnessed)
    }

    pub(super) fn iteration<'a>(
        &self,
        events: &'a [Event],
        live: &BTreeMap<u64, &Event>,
        retained: &mut BTreeMap<(&'a str, u64), &'a Event>,
    ) -> Result<()> {
        // The borrowed slice also spans unscoped events. Only events already
        // bound to this call by Ledger::validate may witness decoder work.
        if self.leopard {
            if !events.iter().any(|v| {
                v.kind == Kind::View
                    && v.call > 0
                    && v.role == "leopard_work"
                    && v.stage == "chunk_coded"
                    && v.region == Region::Decode
            }) {
                return Err("iteration lacks actual Leopard decode work".into());
            }
        } else if self.recovery() && !self.decoder_witness(events)? {
            return Err("iteration lacks actual decoder matrix storage".into());
        }
        let views: Vec<_> = events.iter().filter(|v| v.payload()).collect();
        let expected = self.groups.iter().map(|g| g.count).sum::<usize>();
        if views.len() != expected {
            return Err("incomplete or reordered dynamic payload stages".into());
        }
        for group in &self.groups {
            for (index, v) in views[group.start..group.start + group.count]
                .iter()
                .enumerate()
            {
                if v.role != group.role || v.stage != group.stage || v.index != index as u64 {
                    return Err("incomplete or reordered dynamic payload stages".into());
                }
                let absent = self.missing & (1 << v.index) != 0;
                let present = if matches!(v.role.as_str(), "dynamic" | "baseline" | "candidate") {
                    match v.stage.as_str() {
                        "missing" => !absent,
                        "recovered" => {
                            !absent
                                || if v.role == "baseline" {
                                    !self.baseline_data_only || v.index < self.data
                                } else {
                                    self.wanted & (1 << v.index) != 0
                                }
                        }
                        _ => true,
                    }
                } else {
                    true
                };
                let owner =
                    self.operation == "reconstruct_shard_slot" && v.role == "dynamic" || present;
                if v.present != Some(present) || v.occupied != owner {
                    return Err(
                        "dynamic presence differs from actual missing/required policy".into(),
                    );
                }
                let length = if owner { self.size } else { 0 };
                if v.len != length
                    || v.capacity.is_none_or(|cap| cap < length)
                    || v.element_size != self.width
                    || !owner && v.capacity != Some(0)
                {
                    return Err("dynamic payload initialized length/capacity differs".into());
                }
                if length > 0
                    && (v.storage != "heap"
                        || v.allocation.is_none()
                        || v.allocation_offset != Some(0))
                {
                    return Err("dynamic payload lacks an observed allocation generation".into());
                }
            }
        }
        if matches!(self.profile.as_str(), "smoke" | "hotspot")
            && !self
                .group(&views, "source", "created")?
                .iter()
                .map(|v| v.geometry())
                .eq(self
                    .group(&views, "source", "encoded")?
                    .iter()
                    .map(|v| v.geometry()))
        {
            return Err("encoding replaced generated source storage".into());
        }
        if self.profile == "smoke"
            && self.operation == "update"
            && !self
                .group(&views, "parity", "before")?
                .iter()
                .map(|v| (v.allocation, v.capacity))
                .eq(self
                    .group(&views, "parity", "updated")?
                    .iter()
                    .map(|v| (v.allocation, v.capacity)))
        {
            return Err("update replaced parity backing storage".into());
        }
        if self.profile == "smoke"
            && self.recovery()
            && !self
                .group(&views, "source", "encoded")?
                .iter()
                .map(|v| v.allocation)
                .eq(self
                    .group(&views, "dynamic", "moved")?
                    .iter()
                    .map(|v| v.allocation))
        {
            return Err("smoke source ownership move became a clone".into());
        }
        if self.operation == "reconstruct_shard_slot" {
            for stage in ["missing", "recovered"] {
                if !self
                    .group(&views, "dynamic", "cloned")?
                    .iter()
                    .map(|v| (v.allocation, v.allocation_offset))
                    .eq(self
                        .group(&views, "dynamic", stage)?
                        .iter()
                        .map(|v| (v.allocation, v.allocation_offset)))
                {
                    return Err("ShardSlot storage was replaced while changing presence".into());
                }
            }
        }
        for view in views {
            if let Some(ident) = view.allocation {
                if matches!(self.profile.as_str(), "small_file" | "stream" | "decoder")
                    && matches!(view.role.as_str(), "source" | "scratch")
                {
                    let original = retained
                        .entry((view.role.as_str(), view.index))
                        .or_insert(view);
                    if (
                        original.allocation,
                        original.len,
                        original.capacity,
                        original.offsets,
                    ) != (view.allocation, view.len, view.capacity, view.offsets)
                        || !live.contains_key(&ident)
                    {
                        return Err("retained caller storage changed between iterations".into());
                    }
                } else if live.contains_key(&ident) {
                    return Err("iteration-owned payload outlives its original scope".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;
    use serde_json::{Value, json};

    // Exercise the representation contract independently of the frozen case
    // catalog, whose current layouts all select dense decoding.
    fn contract(width: u64) -> Contract {
        Contract {
            data: 64,
            parity: 4,
            size: 65,
            width,
            lane_width: width,
            missing: 3,
            wanted: u64::MAX,
            baseline_data_only: false,
            operation: "reconstruct".to_owned(),
            profile: "compact-contract-test".to_owned(),
            legacy: false,
            leopard: false,
            groups: Vec::new(),
        }
    }

    fn view(role: &str, rows: u64, width: u64, stage: &str) -> Event {
        let coefficients = role == "decode_coefficients";
        let (len, capacity, element_size, storage) = if coefficients {
            (64 * rows, 64 * rows, width, "heap")
        } else {
            (rows, 4, width, "inline")
        };
        serde_json::from_value(json!({
            "kind":"view", "observation":0, "call":1, "thread":0, "region":"decode",
            "allocation":if coefficients {Some(1)} else {None},
            "allocation_offset":if coefficients {Some(0)} else {None},
            "previous":null, "role":role, "stage":stage, "index":0,
            "len":len, "capacity":capacity, "element_size":element_size,
            "shape":null, "storage":storage, "occupied":true, "present":null,
            "bytes":capacity * element_size, "align":0, "offsets":[0,0,0],
            "elapsed_ns":0, "faults":null,
            "snapshot":{"status":"unsupported", "pages":null, "resident_pages":null, "errno":null}
        }))
        .unwrap()
    }

    fn pair(rows: u64, width: u64, index_width: u64, stage: &str) -> Vec<Event> {
        vec![
            view("decode_coefficients", rows, width, stage),
            view("decode_missing", rows, index_width, stage),
        ]
    }

    fn validate(contract: &Contract, events: &[Event]) -> Result<()> {
        for event in events {
            event.view(contract, &mut None, false)?;
        }
        contract.iteration(events, &BTreeMap::new(), &mut BTreeMap::new())
    }

    #[test]
    fn compact_coefficients_and_indices_witness_complete_typed_decode_storage() {
        for width in [1, 2] {
            for index_width in [4, 8] {
                for rows in 1..=4 {
                    for stage in ["decode_ready", "decode_cached"] {
                        validate(&contract(width), &pair(rows, width, index_width, stage)).unwrap();
                    }
                }
            }
        }
        let mut events = pair(2, 1, 8, "decode_ready");
        events[0].capacity = Some(192);
        events[0].bytes = 192;
        validate(&contract(1), &events).unwrap();

        let mut interleaved = pair(1, 1, 8, "decode_ready");
        let mut other = pair(4, 1, 8, "decode_ready");
        for event in &mut other {
            event.thread = 1;
        }
        interleaved.insert(1, other.remove(0));
        interleaved.push(other.remove(0));
        validate(&contract(1), &interleaved).unwrap();
        let mut repeated = pair(1, 1, 8, "decode_cached");
        repeated.extend(pair(4, 1, 8, "decode_cached"));
        validate(&contract(1), &repeated).unwrap();
    }

    #[test]
    fn compact_roles_reject_bad_lengths_widths_stages_and_owner_metadata() {
        for role in ["decode_coefficients", "decode_missing"] {
            let width = if role == "decode_coefficients" { 1 } else { 8 };
            let original = serde_json::to_value(view(role, 2, width, "decode_ready")).unwrap();
            for (key, value) in [
                ("len", json!(0)),
                ("capacity", Value::Null),
                ("element_size", json!(3)),
                ("stage", json!("constructed")),
                ("stage", json!("decode_submatrix")),
                ("storage", json!("borrowed")),
                ("index", json!(1)),
                ("occupied", json!(false)),
                ("present", json!(true)),
                ("shape", json!([1, 2, 3])),
            ] {
                let mut invalid = original.clone();
                invalid[key] = value;
                let invalid: Event = serde_json::from_value(invalid).unwrap();
                assert!(contract(1).internal(&invalid).is_err(), "{role}/{key}");
            }
        }
        for (len, capacity) in [(63, 64), (65, 128), (320, 320), (128, 127)] {
            let mut invalid = view("decode_coefficients", 2, 1, "decode_ready");
            invalid.len = len;
            invalid.capacity = Some(capacity);
            assert!(contract(1).internal(&invalid).is_err());
        }
        for (len, capacity) in [(5, 8), (2, 2), (2, 8)] {
            let mut invalid = view("decode_missing", 2, 8, "decode_ready");
            invalid.len = len;
            invalid.capacity = Some(capacity);
            assert!(contract(1).internal(&invalid).is_err());
        }
        let mut wrong_width = view("decode_coefficients", 2, 2, "decode_ready");
        assert!(contract(1).internal(&wrong_width).is_err());
        wrong_width.element_size = 1;
        wrong_width.storage = "inline".to_owned();
        assert!(contract(1).internal(&wrong_width).is_err());
        let mut indices = view("decode_missing", 2, 8, "decode_ready");
        indices.storage = "heap".to_owned();
        assert!(contract(1).internal(&indices).is_err());
    }

    #[test]
    fn compact_witness_requires_matching_indices_for_every_coefficient_span() {
        let contract = contract(1);
        for omitted in 0..2 {
            let mut events = pair(2, 1, 8, "decode_ready");
            events.remove(omitted);
            assert!(validate(&contract, &events).is_err());
        }
        for (key, value) in [
            ("len", json!(1)),
            ("call", json!(2)),
            ("thread", json!(1)),
            ("stage", json!("decode_cached")),
            ("observation", json!(1)),
            ("region", json!("caller")),
        ] {
            let mut events = pair(2, 1, 8, "decode_ready");
            let mut invalid = serde_json::to_value(&events[1]).unwrap();
            invalid[key] = value;
            events[1] = serde_json::from_value(invalid).unwrap();
            assert!(validate(&contract, &events).is_err(), "{key}");
        }
        let mut reversed = pair(2, 1, 8, "decode_ready");
        reversed.reverse();
        assert!(validate(&contract, &reversed).is_err());
        let mut duplicate = pair(2, 1, 8, "decode_ready");
        duplicate.insert(1, view("decode_coefficients", 2, 1, "decode_ready"));
        assert!(validate(&contract, &duplicate).is_err());

        for unscoped in ["call", "observation", "region"] {
            let mut events = pair(2, 1, 8, "decode_ready");
            for event in &mut events {
                match unscoped {
                    "call" => event.call = 0,
                    "observation" => event.observation = None,
                    _ => event.region = Region::Caller,
                }
            }
            assert!(validate(&contract, &events).is_err(), "{unscoped}");
        }
    }

    #[test]
    fn compact_representation_keeps_dense_dimensions_and_frozen_catalog_rules() {
        let mut dense = view("decode_coefficients", 1, 1, "decode_ready");
        dense.role = "matrix".to_owned();
        assert!(contract(1).internal(&dense).is_err());
        dense.len = 64 * 64;
        dense.capacity = Some(dense.len);
        dense.bytes = dense.len;
        let mut dense_events = vec![dense];
        validate(&contract(1), &dense_events).unwrap();
        dense_events.push(view("decode_coefficients", 1, 1, "decode_ready"));
        assert!(validate(&contract(1), &dense_events).is_err());

        let events = pair(1, 1, 8, "decode_ready");
        let mut narrow = contract(1);
        narrow.data = 32;
        assert!(validate(&narrow, &events).is_err());
        let mut legacy = contract(1);
        legacy.legacy = true;
        assert!(validate(&legacy, &events).is_err());
        let mut leopard = contract(1);
        leopard.leopard = true;
        assert!(validate(&leopard, &events).is_err());
        assert!(Contract::parse("smoke", "reconstruct:64:4:65", false).is_err());
        assert!(Contract::parse("decoder", "classic_uncached:64:4:65", false).is_err());
    }
}
