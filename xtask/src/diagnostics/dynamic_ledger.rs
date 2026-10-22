//! Allocation generations and typed owner spans from the disposable source observer.
mod payload;
use super::ledger::{self, Snapshot, nullable};
use crate::{Result, sampling_contract};
use payload::Contract;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn catalog() -> Value {
    let supplements: BTreeMap<_, _> = payload::SUPPLEMENTS
        .iter()
        .map(|s| (s.case, (s.family, s.width, s.cache)))
        .collect();
    json!({"profiles":["smoke","small_file","hotspot","stream"],"modes":["calibrate","sample"],"supplements":supplements})
}

pub(crate) fn supplement(
    header: &Value,
    data: &Ledger,
    case: &str,
    page: u64,
    cpus: Option<&[u64]>,
    legacy: bool,
) -> Result<Value> {
    supplement_header(header, case, cpus)?;
    data.validate("decoder", case, page, cpus.is_some(), legacy)
}

fn supplement_header(header: &Value, case: &str, cpus: Option<&[u64]>) -> Result<()> {
    let fixed = payload::supplement(case).ok_or("unknown fixed decoder supplement")?;
    let budget = header["constructor_budget"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("invalid decoder constructor budget")?;
    if cpus.is_some_and(|cpus| budget as u128 > cpus.len() as u128) {
        return Err("decoder constructor budget exceeds reservation".into());
    }
    let parts: Vec<_> = case.split(':').collect();
    let size: u64 = parts[3].parse()?;
    let expected = json!({"schema":"decoder-owner-supplement-v1","observer":"dynamic-source-copy-v1","case":case,
        "data_shards":10,"parity_shards":4,"shard_size_elements":size,"element_size":fixed.width,"family":fixed.family,
        "inversion_cache":fixed.cache,"fast_one_parity":false,"source_lifetime":"worker","construction":"codec-then-source-then-explicit-pool-preparation",
        "output_policy":"iteration-cloned-options-drop-missing-reconstruct-all","missing_mask":1025,"required_mask":16383,"pool_threads":4,
        "native_placement_checked":cpus.is_some(),"normal_outputs_match_source":true,"release_approval":false,"constructor_budget":budget});
    if *header != expected {
        return Err("invalid source-bound decoder supplement".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Alloc,
    AllocZeroed,
    Realloc,
    Free,
    View,
    Enter,
    Leave,
    Begin,
    End,
    Finish,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Region {
    Setup,
    Caller,
    Between,
    Decode,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    kind: Kind,
    #[serde(deserialize_with = "nullable")]
    observation: Option<u64>,
    call: u64,
    thread: u64,
    region: Region,
    #[serde(deserialize_with = "nullable")]
    allocation: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    allocation_offset: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    previous: Option<u64>,
    role: String,
    stage: String,
    index: u64,
    len: u64,
    #[serde(deserialize_with = "nullable")]
    capacity: Option<u64>,
    element_size: u64,
    #[serde(deserialize_with = "nullable")]
    shape: Option<[u64; 3]>,
    storage: String,
    occupied: bool,
    #[serde(deserialize_with = "nullable")]
    present: Option<bool>,
    bytes: u64,
    align: u64,
    #[serde(deserialize_with = "nullable")]
    offsets: Option<[u64; 3]>,
    elapsed_ns: u64,
    #[serde(deserialize_with = "nullable")]
    faults: Option<[u64; 2]>,
    snapshot: Snapshot,
}
type Geometry = (Option<u64>, Option<u64>, u64, Option<u64>, Option<[u64; 3]>);
impl Event {
    fn payload(&self) -> bool {
        self.kind == Kind::View
            && matches!(
                self.role.as_str(),
                "source"
                    | "scratch"
                    | "dynamic"
                    | "baseline"
                    | "candidate"
                    | "clone"
                    | "old_data"
                    | "updated"
                    | "parity"
            )
    }
    fn geometry(&self) -> Geometry {
        (
            self.allocation,
            self.allocation_offset,
            self.len,
            self.capacity,
            self.offsets,
        )
    }
    fn scalars(&self, page: u64) -> Result<()> {
        if self.thread >= 64
            || self.element_size == 0
            || self.observation.is_some_and(|n| n >= 12)
            || self.allocation == Some(0)
            || self.previous == Some(0)
        {
            return Err("invalid dynamic scalar or allocation reference".into());
        }
        ledger::offsets(self.offsets.as_ref(), self.bytes, page)?;
        if self.kind != Kind::View
            && (!self.role.is_empty()
                || !self.stage.is_empty()
                || self.index != 0
                || self.len != 0
                || self.capacity.is_some()
                || self.element_size != 1
                || !self.storage.is_empty()
                || self.occupied
                || self.present.is_some()
                || self.shape.is_some()
                || self.faults.is_some())
        {
            return Err("non-view event contains forged view metadata".into());
        }
        if matches!(
            self.kind,
            Kind::Enter | Kind::Leave | Kind::Begin | Kind::End | Kind::Finish
        ) && (self.bytes != 0
            || self.align != 0
            || self.allocation.is_some()
            || self.allocation_offset.is_some()
            || self.previous.is_some())
        {
            return Err("scope marker contains allocation data".into());
        }
        if self.kind == Kind::View
            && (self.previous.is_some() || self.align != 0 || self.payload() && self.call == 0)
        {
            return Err("view escapes its declared scope".into());
        }
        Ok(())
    }
    fn view(
        &self,
        contract: &Contract,
        last_faults: &mut Option<[u64; 2]>,
        native: bool,
    ) -> Result<()> {
        if !matches!(
            self.role.as_str(),
            "matrix"
                | "source"
                | "scratch"
                | "dynamic"
                | "baseline"
                | "candidate"
                | "clone"
                | "old_data"
                | "updated"
                | "parity"
                | "leopard_work"
                | "leopard_locator"
                | "leopard_scratch"
                | "leopard_out_scratch"
                | "leopard_decoded"
        ) || !matches!(
            self.storage.as_str(),
            "heap" | "inline" | "empty" | "borrowed"
        ) {
            return Err("unknown dynamic view role/storage".into());
        }
        if self.capacity.is_some_and(|cap| self.len > cap)
            || u128::from(self.bytes)
                != u128::from(self.element_size) * u128::from(self.capacity.unwrap_or(self.len))
        {
            return Err("dynamic element/byte extent mismatch".into());
        }
        if native {
            let faults = self.faults.ok_or("missing native dynamic fault counters")?;
            if last_faults.is_some_and(|before| before.iter().zip(faults).any(|(a, b)| *a > b)) {
                return Err("invalid native dynamic fault counters".into());
            }
            *last_faults = Some(faults);
        } else if self.faults.is_some() {
            return Err("unrecorded native dynamic counters".into());
        }
        contract.internal(self)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ledger {
    schema: String,
    page_size: u64,
    errors: u64,
    calls: [u64; 12],
    release_approval: bool,
    events: Vec<Event>,
}
impl Ledger {
    pub(crate) fn validate(
        &self,
        profile: &str,
        case: &str,
        page: u64,
        native: bool,
        legacy: bool,
    ) -> Result<Value> {
        if !sampling_contract::valid_page_size(page) {
            return Err("invalid dynamic page size".into());
        }
        let contract = Contract::parse(profile, case, legacy)?;
        if self.schema != "dynamic-buffer-lifecycle-v1"
            || self.page_size != page
            || self.errors != 0
            || self.calls != [2; 12]
            || self.release_approval
        {
            return Err("invalid or incomplete dynamic lifecycle ledger".into());
        }
        if !(1..=65536).contains(&self.events.len())
            || self.events.last().is_none_or(|e| e.kind != Kind::Finish)
        {
            return Err("bounded complete dynamic ledger required".into());
        }
        let mut live: BTreeMap<u64, &Event> = BTreeMap::new();
        let mut retained = BTreeMap::new();
        let mut regions: [Vec<Region>; 64] = std::array::from_fn(|_| Vec::new());
        let mut faults = [None; 64];
        let mut last_time = 0;
        let mut generation = 0;
        let mut epoch = 0;
        let mut completed = 0;
        let mut opened = None;
        let mut begin = 0;
        let mut payload_views = 0;
        let mut matrix_views = 0;
        let mut bounded = 0;
        for (ordinal, v) in self.events.iter().enumerate() {
            v.scalars(page)?;
            if v.elapsed_ns < last_time {
                return Err("nonmonotonic dynamic event".into());
            }
            last_time = v.elapsed_ns;
            v.snapshot.validate_dynamic(
                v.bytes,
                v.offsets.map(|p| p[2]),
                page,
                native,
                v.kind == Kind::Free,
            )?;
            bounded += u64::from(v.snapshot.status == "over_budget");
            let thread = v.thread as usize;
            if v.kind == Kind::Enter {
                regions[thread].push(v.region);
            }
            if v.kind == Kind::Leave && regions[thread].pop() != Some(v.region) {
                return Err("unbalanced decoder region".into());
            }
            if v.kind == Kind::Finish && ordinal + 1 != self.events.len() {
                return Err("duplicate dynamic finish marker".into());
            }
            if v.kind == Kind::Begin {
                epoch += 1;
                if opened.is_some()
                    || thread != 0
                    || v.call != epoch
                    || v.observation != Some((epoch - 1) / 2)
                {
                    return Err("overlapping or reordered dynamic iteration".into());
                }
                opened = Some((v.call, v.observation));
                begin = ordinal;
            }
            if v.call > 0 && opened != Some((v.call, v.observation)) {
                return Err("event escapes its dynamic iteration".into());
            }
            match v.kind {
                Kind::Alloc | Kind::AllocZeroed | Kind::Realloc => {
                    if !v.align.is_power_of_two()
                        || v.bytes == 0
                        || v.allocation != Some(generation + 1)
                        || v.allocation_offset != Some(0)
                    {
                        return Err("invalid allocation request generation".into());
                    }
                    if v.offsets.ok_or("missing allocation offsets")?[2] % v.align.min(page) != 0 {
                        return Err("allocator alignment differs from request".into());
                    }
                    if let Some(previous) = v.previous {
                        if v.kind != Kind::Realloc {
                            return Err("unexpected previous allocation".into());
                        }
                        if live.get(&previous).is_none_or(|old| old.align != v.align) {
                            return Err("realloc references a dead/foreign generation".into());
                        }
                        live.remove(&previous);
                    }
                    generation += 1;
                    live.insert(generation, v);
                }
                Kind::Free => {
                    if let Some(ident) = v.allocation {
                        let old = live.get(&ident).ok_or("free generation/layout mismatch")?;
                        if (v.bytes, v.align, v.offsets) != (old.bytes, old.align, old.offsets)
                            || v.allocation_offset != Some(0)
                        {
                            return Err("free generation/layout mismatch".into());
                        }
                        live.remove(&ident);
                    } else if v.allocation_offset.is_some() {
                        return Err("unattributed free has an allocation offset".into());
                    }
                }
                Kind::View => {
                    v.view(&contract, &mut faults[thread], native)?;
                    payload_views += u64::from(v.role != "matrix");
                    matrix_views += u64::from(v.role == "matrix" && v.region == Region::Decode);
                    if let Some(ident) = v.allocation {
                        let old = live
                            .get(&ident)
                            .ok_or("view is not inside its live allocation generation")?;
                        let offset = v
                            .allocation_offset
                            .ok_or("view has no live allocation offset")?;
                        if u128::from(offset) + u128::from(v.bytes) > u128::from(old.bytes) {
                            return Err("view escapes its live allocation generation".into());
                        }
                        let original = old.offsets.ok_or("allocation has no offsets")?;
                        let expected = std::array::from_fn(|i| {
                            ((u128::from(original[i]) + u128::from(offset))
                                % u128::from([64, 4096, page][i]))
                                as u64
                        });
                        if v.offsets != Some(expected) {
                            return Err(
                                "view offsets differ from live allocation generation".into()
                            );
                        }
                    } else if v.allocation_offset.is_some() || v.storage == "heap" && v.bytes > 0 {
                        return Err("heap view has no live allocation".into());
                    }
                }
                _ => {}
            }
            if v.kind == Kind::End {
                if thread != 0
                    || opened != Some((v.call, v.observation))
                    || regions.iter().any(|r| !r.is_empty())
                {
                    return Err("dynamic iteration ended inside a decoder region".into());
                }
                let current = &self.events[begin..=ordinal];
                contract.iteration(current, &live, &mut retained)?;
                completed += 1;
                opened = None;
            }
            if v.kind == Kind::Finish
                && (opened.is_some() || v.call != 0 || regions.iter().any(|r| !r.is_empty()))
            {
                return Err("incomplete dynamic owner teardown".into());
            }
        }
        if epoch != 24 || completed != 24 {
            return Err("incomplete dynamic iteration catalog".into());
        }
        if retained
            .values()
            .any(|v: &&Event| v.allocation.is_some_and(|id| live.contains_key(&id)))
        {
            return Err("prepared caller owner was not released before final ledger".into());
        }
        Ok(
            json!({"iterations":24,"events":self.events.len(),"allocation_generations":generation,"retained_requests":live.len(),"payload_views":payload_views,"decoder_matrix_views":matrix_views,"bounded_residency_queries":bounded}),
        )
    }
}

#[cfg(test)]
mod tests;
