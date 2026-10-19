//! Diagnostic-copy hooks only. Never linked into the production/formal sampler.
//! Numeric former ranges are queried without references or content access.
use std::fmt;
use std::mem::size_of;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
#[cfg(target_os = "linux")]
use std::{
    fs,
    io::{self, Read},
};
mod page_residency;
const LIMIT: usize = 512;
const PHASES: [&str; 4] = ["reserved", "initialized", "verified", "released"];
static ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Event {
    observation: usize,
    call: usize,
    phase: usize,
    len: usize,
    capacity: usize,
    offsets: [usize; 3],
    inline: bool,
    elapsed_ns: u128,
    faults: Option<[u64; 2]>,
    snapshot: page_residency::Snapshot,
    valid: Option<bool>,
}
impl Event {
    const EMPTY: Self = Self {
        observation: 0,
        call: 0,
        phase: 0,
        len: 0,
        capacity: 0,
        offsets: [0; 3],
        inline: false,
        elapsed_ns: 0,
        faults: None,
        snapshot: page_residency::Snapshot::EMPTY,
        valid: None,
    };
}
struct State {
    events: [Event; LIMIT],
    used: usize,
    errors: usize,
    page: usize,
    epoch: Option<Instant>,
    owner: Option<std::thread::ThreadId>,
    observation: usize,
    calls: [usize; 12],
}
impl State {
    const fn new() -> Self {
        Self {
            events: [Event::EMPTY; LIMIT],
            used: 0,
            errors: 0,
            page: 0,
            epoch: None,
            owner: None,
            observation: 0,
            calls: [0; 12],
        }
    }
}
static STATE: Mutex<State> = Mutex::new(State::new());
fn faults() -> Option<[u64; 2]> {
    #[cfg(target_os = "linux")]
    {
        let mut buffer = [0u8; 4096];
        let (minor, major) =
            process_faults(read_bounded("/proc/self/stat", &mut buffer).ok()?).ok()?;
        Some([minor, major])
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
/// Prepare outside the original measurement schedule.
pub fn start(page: usize) {
    assert!(!ACTIVE.load(Ordering::Relaxed));
    assert!(page.is_power_of_two());
    let mut s = STATE.lock().unwrap();
    *s = State::new();
    s.page = page;
    s.epoch = Some(Instant::now());
    s.owner = Some(std::thread::current().id());
    ACTIVE.store(true, Ordering::Relaxed);
}
/// Select the original observation, without changing its iteration count.
pub fn observation(index: usize) {
    assert!(index < 12);
    STATE.lock().unwrap().observation = index;
}
/// A guard declared before scratch, so its destructor runs after scratch drops.
pub struct Guard {
    active: bool,
    observation: usize,
    call: usize,
    requested: usize,
    address: usize,
    capacity: usize,
    len: usize,
    inline: bool,
    phase: usize,
    valid: Option<bool>,
}
impl Guard {
    pub fn new(requested: usize) -> Self {
        let mut result = Self {
            active: false,
            observation: 0,
            call: 0,
            requested,
            address: 0,
            capacity: 0,
            len: 0,
            inline: false,
            phase: 0,
            valid: None,
        };
        if !ACTIVE.load(Ordering::Relaxed) {
            return result;
        }
        let mut s = STATE.lock().unwrap();
        if s.owner != Some(std::thread::current().id()) {
            s.errors += 1;
            return result;
        }
        let index = s.observation;
        s.calls[index] += 1;
        result.active = true;
        result.observation = index;
        result.call = s.calls[index];
        result
    }
    pub fn reserved<T>(&mut self, pointer: *const T, capacity: usize, inline: bool) {
        if !self.active {
            return;
        }
        self.address = pointer as usize;
        self.capacity = capacity.checked_mul(size_of::<T>()).unwrap();
        self.requested = self.requested.checked_mul(size_of::<T>()).unwrap();
        self.inline = inline;
        self.record(0);
    }
    pub fn initialized<T>(&mut self, pointer: *const T, length: usize, capacity: usize) {
        if !self.active {
            return;
        }
        self.len = length.checked_mul(size_of::<T>()).unwrap();
        if self.address != pointer as usize
            || self.capacity != capacity * size_of::<T>()
            || self.len != self.requested
        {
            STATE.lock().unwrap().errors += 1;
        }
        self.record(1);
    }
    pub fn verified(&mut self, valid: bool) {
        if !self.active {
            return;
        }
        self.valid = Some(valid);
        self.record(2);
    }
    fn record(&mut self, phase: usize) {
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if s.owner != Some(std::thread::current().id()) {
            s.errors += 1;
            return;
        }
        if self.phase != phase || self.capacity == 0 || self.capacity < self.requested {
            s.errors += 1;
        }
        self.phase = phase + 1;
        if s.used == LIMIT {
            s.errors += 1;
            return;
        }
        let snapshot = page_residency::query(self.address, self.capacity, s.page);
        let event = Event {
            observation: self.observation,
            call: self.call,
            phase,
            len: self.len,
            capacity: self.capacity,
            offsets: [
                self.address % 64,
                self.address % 4096,
                self.address % s.page,
            ],
            inline: self.inline,
            elapsed_ns: s.epoch.unwrap().elapsed().as_nanos(),
            faults: faults(),
            snapshot,
            valid: self.valid,
        };
        let used = s.used;
        s.events[used] = event;
        s.used += 1;
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.active {
            self.record(3);
        }
    }
}
struct Json<'a>(&'a State);
impl fmt::Display for Json<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0;
        write!(
            f,
            "{{\"schema\":\"verify-scratch-lifecycle-v1\",\"page_size\":{},\"errors\":{},\"calls\":{:?},\"release_approval\":false,\"events\":[",
            s.page, s.errors, s.calls
        )?;
        for (index, e) in s.events[..s.used].iter().enumerate() {
            if index != 0 {
                f.write_str(",")?;
            }
            write!(
                f,
                "{{\"observation\":{},\"call\":{},\"phase\":\"{}\",\"len\":{},\"capacity\":{},\"offsets\":{:?},\"inline\":{},\"elapsed_ns\":{},\"faults\":",
                e.observation,
                e.call,
                PHASES[e.phase],
                e.len,
                e.capacity,
                e.offsets,
                e.inline,
                e.elapsed_ns
            )?;
            match e.faults {
                Some(v) => write!(f, "{v:?}")?,
                None => f.write_str("null")?,
            }
            write!(f, ",\"snapshot\":{},\"valid\":", e.snapshot)?;
            match e.valid {
                Some(v) => write!(f, "{v}")?,
                None => f.write_str("null")?,
            }
            f.write_str("}")?;
        }
        f.write_str("]}")
    }
}
/// Export after every original warmup/sample; retain raw failures before refusal.
pub fn export() -> bool {
    let valid = stop();
    let s = STATE.lock().unwrap();
    println!("\nRSE_VERIFY_SCRATCH {}", Json(&s));
    valid
}
/// Stop without exporting raw data, for the independent allocator test.
pub fn stop() -> bool {
    ACTIVE.store(false, Ordering::Relaxed);
    STATE.lock().unwrap().errors == 0
}

/// Native base page size; unavailable platforms remain explicit.
pub fn native_page_size() -> Option<usize> {
    page_residency::page_size()
}
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guard_records_the_original_false_result_and_releases_after_the_owner() {
        let _serial = TEST_LOCK.lock().unwrap();
        start(native_page_size().unwrap_or(4096));
        observation(0);
        {
            let mut guard = Guard::new(1061);
            let mut bytes = Vec::<u8>::with_capacity(1061);
            guard.reserved(bytes.as_ptr(), bytes.capacity(), false);
            bytes.resize(1061, 0);
            guard.initialized(bytes.as_ptr(), bytes.len(), bytes.capacity());
            guard.verified(false);
        }
        ACTIVE.store(false, Ordering::Relaxed);
        let s = STATE.lock().unwrap();
        assert_eq!(s.errors, 0);
        assert_eq!(s.used, 4);
        assert_eq!(s.events[3].phase, 3);
        assert_eq!(s.events[3].valid, Some(false));
    }
    #[test]
    fn interrupted_lifecycle_is_retained_as_invalid_without_panicking_in_drop() {
        let _serial = TEST_LOCK.lock().unwrap();
        start(native_page_size().unwrap_or(4096));
        observation(0);
        drop(Guard::new(1061));
        ACTIVE.store(false, Ordering::Relaxed);
        let s = STATE.lock().unwrap();
        assert!(s.errors > 0);
        assert_eq!(s.used, 1);
    }
}
