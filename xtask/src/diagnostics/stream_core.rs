//! Streaming owner validation: replay expected boundaries directly against events.
use super::ledger::{self, Clock, Snapshot, nullable};
use crate::{Result, sampling_contract};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Api {
    EncodeStream,
    VerifyStream,
    ReconstructStream,
    VerifyPar,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Variant {
    RetainedPrefix,
    VecViews,
    RetainedScratch,
    PerBlockVerifyPar,
    PlannedRetained,
    OptionTransfer,
    SmallvecPerBlock,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Enter,
    Reserved,
    Read,
    Ready,
    Initialized,
    Coded,
    Recycled,
    Replaced,
    Released,
    Exit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    observation: u64,
    call: u64,
    scope: u64,
    parent: u64,
    api: Api,
    variant: Variant,
    block: u64,
    phase: Phase,
    #[serde(deserialize_with = "nullable")]
    slot: Option<u64>,
    len: u64,
    capacity: u64,
    visible_len: u64,
    occupied: bool,
    #[serde(deserialize_with = "nullable")]
    offsets: Option<[u64; 3]>,
    elapsed_ns: u64,
    #[serde(deserialize_with = "nullable")]
    faults: Option<[u64; 2]>,
    snapshot: Snapshot,
    complete: bool,
    #[serde(deserialize_with = "nullable")]
    valid: Option<bool>,
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
struct Shape {
    api: Api,
    data: u64,
    parity: u64,
    size: u64,
    chunk: u64,
    blocks: u64,
    current: bool,
}
impl Shape {
    fn parse(case: &str, current: bool) -> Result<Self> {
        if !sampling_contract::known_case("stream", case) {
            return Err("invalid stream diagnostic contract".into());
        }
        let p: Vec<_> = case.split(':').collect();
        let api = match p[0] {
            "encode_stream" => Api::EncodeStream,
            "verify_stream" => Api::VerifyStream,
            "reconstruct_stream" => Api::ReconstructStream,
            _ => return Err("unknown stream API".into()),
        };
        let (data, parity, size, chunk) = (
            p[1].parse()?,
            p[2].parse()?,
            p[3].parse::<u64>()?,
            p[4].parse::<u64>()?,
        );
        Ok(Self {
            api,
            data,
            parity,
            size,
            chunk,
            blocks: size.div_ceil(chunk),
            current,
        })
    }
    fn variant(&self, api: Api) -> Variant {
        match (api, self.current) {
            (Api::EncodeStream, true) => Variant::RetainedPrefix,
            (Api::EncodeStream, false) => Variant::VecViews,
            (Api::VerifyStream, true) => Variant::RetainedScratch,
            (Api::VerifyStream, false) => Variant::PerBlockVerifyPar,
            (Api::ReconstructStream, true) => Variant::PlannedRetained,
            (Api::ReconstructStream, false) => Variant::OptionTransfer,
            (Api::VerifyPar, _) => Variant::SmallvecPerBlock,
        }
    }
    fn slots(&self) -> Vec<u64> {
        let count = if self.api == Api::EncodeStream {
            self.data
        } else {
            self.data + self.parity
        };
        let mut slots: Vec<_> = (0..count).collect();
        let second =
            if self.api == Api::EncodeStream || self.api == Api::VerifyStream && self.current {
                self.parity
            } else if self.api == Api::ReconstructStream && !self.current {
                self.data + self.parity
            } else {
                0
            };
        slots.extend(48..48 + second);
        slots
    }
    fn owner(&self, api: Api, phase: Phase, slot: u64, block: u64) -> Owner {
        let (role, index) = (slot / 48, slot % 48);
        let amount = if (1..=self.blocks).contains(&block) {
            self.chunk.min(self.size - (block - 1) * self.chunk)
        } else {
            0
        };
        let previous = if block > 1 {
            self.chunk.min(self.size - (block - 2) * self.chunk)
        } else {
            0
        };
        let owner = |len, visible, occupied, minimum| Owner {
            len,
            visible,
            occupied,
            minimum,
        };
        if api == Api::VerifyPar {
            let length = self.parity * amount;
            return owner(
                if phase == Phase::Reserved { 0 } else { length },
                if phase == Phase::Reserved { 0 } else { length },
                true,
                length,
            );
        }
        if matches!(api, Api::EncodeStream | Api::VerifyStream) {
            if role == 0 {
                return owner(
                    if phase == Phase::Reserved {
                        0
                    } else {
                        self.chunk
                    },
                    if phase == Phase::Reserved { 0 } else { amount },
                    true,
                    self.chunk,
                );
            }
            if api == Api::VerifyStream && phase == Phase::Reserved {
                return owner(0, 0, true, 0);
            }
            let length = if phase == Phase::Reserved || phase == Phase::Read && block == 1 {
                0
            } else if phase == Phase::Read {
                previous
            } else {
                amount
            };
            return owner(
                length,
                length,
                true,
                if api == Api::EncodeStream || length > 0 {
                    self.chunk
                } else {
                    0
                },
            );
        }
        let missing = [0, self.data].contains(&index);
        if phase == Phase::Reserved {
            return owner(
                0,
                0,
                role == 0,
                if role == 0 && !missing { self.chunk } else { 0 },
            );
        }
        if self.current {
            let length = if phase == Phase::Read && missing {
                previous
            } else {
                amount
            };
            return owner(
                length,
                length,
                true,
                if !missing || block > 1 || phase != Phase::Read {
                    self.chunk
                } else {
                    0
                },
            );
        }
        if role == 1 {
            if matches!(phase, Phase::Read | Phase::Recycled) || phase == Phase::Ready && missing {
                return owner(0, 0, false, 0);
            }
            return owner(
                amount,
                amount,
                true,
                if missing { amount } else { self.chunk },
            );
        }
        if matches!(phase, Phase::Ready | Phase::Coded) {
            return if missing {
                owner(previous, previous, true, previous)
            } else {
                owner(0, 0, true, 0)
            };
        }
        let length = if phase == Phase::Read && missing {
            previous
        } else {
            amount
        };
        owner(
            length,
            length,
            true,
            if !missing || block > 1 || phase != Phase::Read {
                self.chunk
            } else {
                0
            },
        )
    }
}
#[derive(Clone, Copy)]
struct Owner {
    len: u64,
    visible: u64,
    occupied: bool,
    minimum: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Range {
    len: u64,
    capacity: u64,
    visible: u64,
    occupied: bool,
    offsets: Option<[u64; 3]>,
}
struct Expected {
    observation: u64,
    call: u64,
    scope: u64,
    parent: u64,
    api: Api,
    block: u64,
    phase: Phase,
    slot: Option<u64>,
    complete: bool,
    valid: Option<bool>,
}
struct Cursor<'a> {
    events: &'a [Event],
    index: usize,
    shape: &'a Shape,
    clock: Clock,
    ranges: BTreeMap<(u64, u64), Range>,
    native: bool,
    page: u64,
    nested: u64,
}
impl Cursor<'_> {
    fn emit(&mut self, w: Expected) -> Result<()> {
        let v = self
            .events
            .get(self.index)
            .ok_or("missing source owner/block/release boundary")?;
        if (
            v.observation,
            v.call,
            v.scope,
            v.parent,
            v.api,
            v.variant,
            v.block,
            v.phase,
            v.slot,
            v.complete,
            v.valid,
        ) != (
            w.observation,
            w.call,
            w.scope,
            w.parent,
            w.api,
            self.shape.variant(w.api),
            w.block,
            w.phase,
            w.slot,
            w.complete,
            w.valid,
        ) {
            return Err("reordered or incomplete source scope".into());
        }
        if v.visible_len > v.len || v.len > v.capacity {
            return Err("invalid source capacity or initialized prefix".into());
        }
        ledger::offsets(v.offsets.as_ref(), v.capacity, self.page)?;
        let range = Range {
            len: v.len,
            capacity: v.capacity,
            visible: v.visible_len,
            occupied: v.occupied,
            offsets: v.offsets,
        };
        let former = matches!(v.phase, Phase::Released | Phase::Replaced);
        match v.slot {
            None => {
                if v.len != 0
                    || v.capacity != 0
                    || v.visible_len != 0
                    || v.occupied
                    || v.offsets.is_some()
                {
                    return Err("source scope control contains storage".into());
                }
            }
            Some(slot) => {
                let key = (v.scope, slot);
                if former {
                    if v.capacity == 0 || self.ranges.get(&key) != Some(&range) {
                        return Err("former range differs from last actual owner".into());
                    }
                } else {
                    let wanted = self.shape.owner(v.api, v.phase, slot, v.block);
                    if (v.len, v.visible_len, v.occupied)
                        != (wanted.len, wanted.visible, wanted.occupied)
                        || if wanted.minimum > 0 {
                            v.capacity < wanted.minimum
                        } else {
                            v.capacity != 0
                        }
                    {
                        return Err("source container occupancy/initialized shape differs".into());
                    }
                    self.ranges.insert(key, range);
                }
            }
        }
        v.snapshot
            .validate(
                v.capacity,
                v.offsets.map(|p| p[2]),
                self.page,
                self.native,
                former,
                false,
            )
            .map_err(|e| format!("unavailable live source range: {e}"))?;
        self.clock.observe(v.elapsed_ns, v.faults, self.native)?;
        if v.api == Api::VerifyPar && v.phase == Phase::Enter {
            self.nested += 1;
        }
        if v.phase == Phase::Exit {
            self.ranges.retain(|(scope, _), _| *scope != v.scope);
        }
        self.index += 1;
        Ok(())
    }
    fn group(
        &mut self,
        scope: (u64, u64, u64),
        block: u64,
        phase: Phase,
        slots: &[u64],
    ) -> Result<()> {
        let (observation, call, scope) = scope;
        for slot in slots {
            self.emit(Expected {
                observation,
                call,
                scope,
                parent: 0,
                api: self.shape.api,
                block,
                phase,
                slot: Some(*slot),
                complete: false,
                valid: None,
            })?;
        }
        Ok(())
    }
}
impl Ledger {
    pub(crate) fn validate(
        &self,
        case: &str,
        iterations: u64,
        page: u64,
        current: bool,
        native: bool,
    ) -> Result<Value> {
        if !(1..=2).contains(&iterations) || !sampling_contract::valid_page_size(page) {
            return Err("invalid stream diagnostic contract".into());
        }
        let shape = Shape::parse(case, current)?;
        if self.schema != "stream-core-lifecycle-v1"
            || self.page_size != page
            || self.errors != 0
            || self.calls != [iterations; 12]
            || self.release_approval
        {
            return Err("invalid or incomplete stream-core ledger".into());
        }
        if !(1..=32768).contains(&self.events.len()) {
            return Err("missing bounded source lifecycle".into());
        }
        let slots = shape.slots();
        let mut scope = 0;
        let mut cursor = Cursor {
            events: &self.events,
            index: 0,
            shape: &shape,
            clock: Clock::default(),
            ranges: BTreeMap::new(),
            native,
            page,
            nested: 0,
        };
        for observation in 0..12 {
            for call in 1..=iterations {
                scope += 1;
                let outer = scope;
                cursor.emit(Expected {
                    observation,
                    call,
                    scope: outer,
                    parent: 0,
                    api: shape.api,
                    block: 0,
                    phase: Phase::Enter,
                    slot: None,
                    complete: false,
                    valid: None,
                })?;
                cursor.group((observation, call, outer), 0, Phase::Reserved, &slots)?;
                for block in 1..=shape.blocks + 1 {
                    cursor.group((observation, call, outer), block, Phase::Read, &slots)?;
                    if block > shape.blocks {
                        break;
                    }
                    cursor.group((observation, call, outer), block, Phase::Ready, &slots)?;
                    if shape.api == Api::VerifyStream && !current {
                        scope += 1;
                        let child = scope;
                        for (phase, slot, complete, valid) in [
                            (Phase::Enter, None, false, None),
                            (Phase::Reserved, Some(0), false, None),
                            (Phase::Initialized, Some(0), false, None),
                            (Phase::Coded, Some(0), false, None),
                            (Phase::Released, Some(0), true, Some(true)),
                            (Phase::Exit, None, true, Some(true)),
                        ] {
                            cursor.emit(Expected {
                                observation,
                                call,
                                scope: child,
                                parent: outer,
                                api: Api::VerifyPar,
                                block,
                                phase,
                                slot,
                                complete,
                                valid,
                            })?;
                        }
                    }
                    cursor.group((observation, call, outer), block, Phase::Coded, &slots)?;
                    if shape.api == Api::ReconstructStream && !current {
                        if block > 1 {
                            for slot in [0, shape.data] {
                                cursor.emit(Expected {
                                    observation,
                                    call,
                                    scope: outer,
                                    parent: 0,
                                    api: shape.api,
                                    block,
                                    phase: Phase::Replaced,
                                    slot: Some(slot),
                                    complete: false,
                                    valid: None,
                                })?;
                            }
                        }
                        cursor.group((observation, call, outer), block, Phase::Recycled, &slots)?;
                    }
                }
                let valid = if shape.api == Api::VerifyStream {
                    Some(true)
                } else {
                    None
                };
                for slot in &slots {
                    if cursor
                        .ranges
                        .get(&(outer, *slot))
                        .is_some_and(|r| r.capacity > 0)
                    {
                        cursor.emit(Expected {
                            observation,
                            call,
                            scope: outer,
                            parent: 0,
                            api: shape.api,
                            block: shape.blocks + 1,
                            phase: Phase::Released,
                            slot: Some(*slot),
                            complete: true,
                            valid,
                        })?;
                    }
                }
                cursor.emit(Expected {
                    observation,
                    call,
                    scope: outer,
                    parent: 0,
                    api: shape.api,
                    block: shape.blocks + 1,
                    phase: Phase::Exit,
                    slot: None,
                    complete: true,
                    valid,
                })?;
            }
        }
        if cursor.index != self.events.len() {
            return Err("unexpected trailing source boundaries".into());
        }
        Ok(
            json!({"calls":12*iterations,"events":self.events.len(),"nested_verify_calls":cursor.nested}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_and_retained_recovery_have_distinct_owner_transitions() {
        // A two-block recovery with one missing data and one missing parity shard.
        // Old owners move to Option slots; retained owners stay in their slots.
        for (current, phase, slot, block, expected) in [
            (false, Phase::Reserved, 0, 0, (0, 0, true, 0)),
            (false, Phase::Reserved, 1, 0, (0, 0, true, 262144)),
            (false, Phase::Reserved, 48, 0, (0, 0, false, 0)),
            (false, Phase::Ready, 1, 1, (0, 0, true, 0)),
            (false, Phase::Ready, 49, 1, (262144, 262144, true, 262144)),
            (false, Phase::Ready, 48, 1, (0, 0, false, 0)),
            (false, Phase::Coded, 48, 1, (262144, 262144, true, 262144)),
            (false, Phase::Recycled, 48, 1, (0, 0, false, 0)),
            (false, Phase::Ready, 0, 2, (262144, 262144, true, 262144)),
            (true, Phase::Read, 0, 1, (0, 0, true, 0)),
            (true, Phase::Ready, 0, 1, (262144, 262144, true, 262144)),
            (true, Phase::Read, 0, 2, (262144, 262144, true, 262144)),
            (true, Phase::Ready, 1, 2, (262144, 262144, true, 262144)),
        ] {
            let shape = Shape::parse("reconstruct_stream:4:2:1048576:262144", current).unwrap();
            let owner = shape.owner(shape.api, phase, slot, block);
            assert_eq!(
                (owner.len, owner.visible, owner.occupied, owner.minimum),
                expected
            );
        }
    }

    #[test]
    fn original_nested_verify_scratch_keeps_full_capacity_before_initialization() {
        let shape = Shape::parse("verify_stream:4:2:1048576:262144", false).unwrap();
        assert_eq!(shape.variant(shape.api), Variant::PerBlockVerifyPar);
        assert_eq!(shape.slots(), vec![0, 1, 2, 3, 4, 5]);
        for phase in [Phase::Reserved, Phase::Initialized, Phase::Coded] {
            let owner = shape.owner(Api::VerifyPar, phase, 0, 1);
            assert_eq!(owner.minimum, 524288);
            assert_eq!(owner.len, if phase == Phase::Reserved { 0 } else { 524288 });
            assert_eq!(owner.visible, owner.len);
            assert!(owner.occupied);
        }
    }
}
