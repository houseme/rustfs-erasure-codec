//! Ordinary verify scratch: retain all twelve observations and every lifetime boundary.
use super::ledger::{self, Clock, Snapshot, nullable};
use crate::{Result, sampling_contract};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Reserved,
    Initialized,
    Verified,
    Released,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    observation: u64,
    call: u64,
    phase: Phase,
    len: u64,
    capacity: u64,
    offsets: [u64; 3],
    inline: bool,
    elapsed_ns: u64,
    #[serde(deserialize_with = "nullable")]
    faults: Option<[u64; 2]>,
    snapshot: Snapshot,
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
impl Ledger {
    pub(super) fn bind(&self, bindings: &super::worker::Bindings<'_>) -> Result<()> {
        for event in &self.events {
            bindings.faults(event.observation, event.faults)?;
        }
        Ok(())
    }
    pub(crate) fn validate(
        &self,
        case: &str,
        iterations: u64,
        page: u64,
        native: bool,
    ) -> Result<Value> {
        if !case.starts_with("verify:")
            || !(sampling_contract::known_case("smoke", case)
                || sampling_contract::known_case("small_file", case))
            || !sampling_contract::valid_page_size(page)
        {
            return Err("unknown verify case or page size".into());
        }
        if self.schema != "verify-scratch-lifecycle-v1"
            || self.page_size != page
            || self.errors != 0
            || self.release_approval
        {
            return Err("invalid verify scratch observation".into());
        }
        if !(1..=2).contains(&iterations) || self.calls != [iterations; 12] {
            return Err("incomplete verify scratch calls".into());
        }
        if self.events.len() != 12 * iterations as usize * 4 {
            return Err("incomplete scratch lifecycle".into());
        }
        let parts: Vec<_> = case.split(':').collect();
        let length = parts[2]
            .parse::<u64>()?
            .checked_mul(parts[3].parse()?)
            .ok_or("scratch length overflow")?;
        let mut clock = Clock::default();
        let mut inline_calls = 0;
        for (index, event) in self.events.iter().enumerate() {
            let phase = index % 4;
            let observation = index / (iterations as usize * 4);
            let call = (index / 4) % iterations as usize + 1;
            if event.observation != observation as u64
                || event.call != call as u64
                || event.phase
                    != [
                        Phase::Reserved,
                        Phase::Initialized,
                        Phase::Verified,
                        Phase::Released,
                    ][phase]
            {
                return Err("reordered scratch boundary".into());
            }
            if event.len != if phase == 0 { 0 } else { length } || event.capacity < length {
                return Err("wrong scratch storage shape".into());
            }
            ledger::offsets(Some(&event.offsets), event.capacity, page)?;
            clock.observe(event.elapsed_ns, event.faults, native)?;
            if event.valid != if phase >= 2 { Some(true) } else { None } {
                return Err("original verify result was not retained".into());
            }
            event
                .snapshot
                .validate(
                    event.capacity,
                    Some(event.offsets[2]),
                    page,
                    native,
                    phase == 3,
                    false,
                )
                .map_err(|e| format!("unavailable live scratch residency: {e}"))?;
            if phase > 0 {
                let previous = &self.events[index - 1];
                if event.capacity != previous.capacity
                    || event.offsets != previous.offsets
                    || event.inline != previous.inline
                {
                    return Err("scratch storage changed within call".into());
                }
            } else if event.inline {
                inline_calls += 1;
            }
        }
        Ok(json!({"calls":12*iterations,"events":self.events.len(),"inline_calls":inline_calls}))
    }
}
