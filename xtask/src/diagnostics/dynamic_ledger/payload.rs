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
                if !self.leopard || v.present.is_some() || !v.occupied || v.capacity.is_none() {
                    return Err("invalid internal decoder owner".into());
                }
                match role {
                    "leopard_locator" => {
                        let length = if self.lane_width == 2 { 65536 } else { 256 };
                        if v.len != length
                            || v.capacity != Some(length)
                            || v.element_size != self.lane_width
                            || v.storage != "inline"
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
        Ok(())
    }
    pub(super) fn iteration<'a>(
        &self,
        events: &'a [Event],
        live: &BTreeMap<u64, &Event>,
        retained: &mut BTreeMap<(&'a str, u64), &'a Event>,
    ) -> Result<()> {
        if self.leopard {
            if !events.iter().any(|v| {
                v.kind == Kind::View
                    && v.role == "leopard_work"
                    && v.stage == "chunk_coded"
                    && v.region == Region::Decode
            }) {
                return Err("iteration lacks actual Leopard decode work".into());
            }
        } else if self.recovery()
            && !events.iter().any(|v| {
                v.kind == Kind::View
                    && v.role == "matrix"
                    && matches!(v.stage.as_str(), "decode_ready" | "decode_cached")
                    && v.region == Region::Decode
            })
        {
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
