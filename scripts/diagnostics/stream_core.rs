//! Diagnostic source-copy observer. Never linked into the production codec.
//! Capacity ranges are only queried numerically; no uninitialized/former bytes
//! are accessed. All event/counter storage is bounded and heap-free.
use std::{
    fmt,
    mem::size_of,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
#[cfg(target_os = "linux")]
use std::{
    fs,
    io::{self, Read},
};
mod page_residency;
const LIMIT: usize = 32768;
const SLOTS: usize = 96;
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy, Default)]
struct Range {
    address: usize,
    len: usize,
    capacity: usize,
    visible: usize,
    occupied: bool,
}
#[derive(Clone, Copy)]
struct Event {
    observation: usize,
    call: usize,
    scope: usize,
    parent: usize,
    api: &'static str,
    variant: &'static str,
    block: usize,
    phase: &'static str,
    slot: Option<usize>,
    range: Range,
    offsets: Option<[usize; 3]>,
    elapsed_ns: u128,
    faults: Option<[u64; 2]>,
    snapshot: page_residency::Snapshot,
    complete: bool,
    valid: Option<bool>,
}
impl Event {
    const EMPTY: Self = Self {
        observation: 0,
        call: 0,
        scope: 0,
        parent: 0,
        api: "",
        variant: "",
        block: 0,
        phase: "",
        slot: None,
        range: Range {
            address: 0,
            len: 0,
            capacity: 0,
            visible: 0,
            occupied: false,
        },
        offsets: None,
        elapsed_ns: 0,
        faults: None,
        snapshot: page_residency::Snapshot::EMPTY,
        complete: false,
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
    scopes: usize,
    stack: [usize; 2],
    blocks: [usize; 2],
    depth: usize,
}
static STATE: Mutex<State> = Mutex::new(State {
    events: [Event::EMPTY; LIMIT],
    used: 0,
    errors: 0,
    page: 0,
    epoch: None,
    owner: None,
    observation: 0,
    calls: [0; 12],
    scopes: 0,
    stack: [0; 2],
    blocks: [0; 2],
    depth: 0,
});
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
pub fn native_page_size() -> Option<usize> {
    page_residency::page_size()
}
pub fn start(page: usize) {
    assert!(!ACTIVE.load(Ordering::Relaxed));
    assert!(page.is_power_of_two());
    let mut s = STATE.lock().unwrap();
    // Reset cursors, not the multi-megabyte static array on the worker stack.
    s.used = 0;
    s.errors = 0;
    s.page = page;
    s.epoch = Some(Instant::now());
    s.owner = Some(std::thread::current().id());
    s.observation = 0;
    s.calls = [0; 12];
    s.scopes = 0;
    s.depth = 0;
    s.stack = [0; 2];
    s.blocks = [0; 2];
    ACTIVE.store(true, Ordering::Relaxed);
}
pub fn observation(index: usize) {
    let mut s = STATE.lock().unwrap();
    assert!(index < 12);
    assert_eq!(s.depth, 0);
    s.observation = index;
}
/// Must be declared before every byte owner whose final range it observes.
pub struct Guard {
    active: bool,
    observation: usize,
    call: usize,
    scope: usize,
    parent: usize,
    depth: usize,
    api: &'static str,
    variant: &'static str,
    block: usize,
    ranges: [Range; SLOTS],
    complete: bool,
    valid: Option<bool>,
}
impl Guard {
    pub fn new(api: &'static str, variant: &'static str) -> Self {
        let mut g = Self {
            active: false,
            observation: 0,
            call: 0,
            scope: 0,
            parent: 0,
            depth: 0,
            api,
            variant,
            block: 0,
            ranges: [Range::default(); SLOTS],
            complete: false,
            valid: None,
        };
        if !ACTIVE.load(Ordering::Relaxed) {
            return g;
        }
        {
            let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
            if s.owner != Some(std::thread::current().id()) || s.depth == 2 {
                s.errors += 1;
                return g;
            }
            let observation = s.observation;
            let depth = s.depth;
            if depth == 0 {
                s.calls[observation] += 1;
            }
            s.scopes += 1;
            g.active = true;
            g.observation = observation;
            g.call = s.calls[observation];
            g.scope = s.scopes;
            g.parent = if depth == 0 { 0 } else { s.stack[depth - 1] };
            g.depth = depth;
            g.block = if depth == 0 { 0 } else { s.blocks[depth - 1] };
            s.stack[depth] = g.scope;
            s.blocks[depth] = g.block;
            s.depth += 1;
        }
        g.record("enter", None, Range::default());
        g
    }
    pub fn next_block(&mut self) {
        if !self.active {
            return;
        }
        self.block += 1;
        STATE.lock().unwrap_or_else(|e| e.into_inner()).blocks[self.depth] = self.block;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn span<T>(
        &mut self,
        phase: &'static str,
        role: usize,
        index: usize,
        pointer: *const T,
        len: usize,
        capacity: usize,
        visible: usize,
        occupied: bool,
    ) {
        if !self.active {
            return;
        }
        let Some(slot) = role
            .checked_mul(48)
            .and_then(|v| v.checked_add(index))
            .filter(|_| role < 2 && index < 48)
        else {
            STATE.lock().unwrap_or_else(|e| e.into_inner()).errors += 1;
            return;
        };
        let Some((len, capacity, visible)) = len
            .checked_mul(size_of::<T>())
            .zip(capacity.checked_mul(size_of::<T>()))
            .zip(visible.checked_mul(size_of::<T>()))
            .map(|((l, c), v)| (l, c, v))
        else {
            STATE.lock().unwrap_or_else(|e| e.into_inner()).errors += 1;
            return;
        };
        let range = Range {
            address: pointer as usize,
            len,
            capacity,
            visible,
            occupied,
        };
        self.ranges[slot] = range;
        self.record(phase, Some(slot), range);
    }
    /// Query the previous Vec range after assignment has dropped its owner.
    pub fn replaced(&self, role: usize, index: usize) {
        if !self.active {
            return;
        }
        if let Some(range) = role
            .checked_mul(48)
            .and_then(|v| v.checked_add(index))
            .filter(|_| role < 2 && index < 48)
            .and_then(|v| self.ranges.get(v))
            .copied()
        {
            if range.capacity != 0 {
                self.record("replaced", Some(role * 48 + index), range);
            }
        } else {
            STATE.lock().unwrap_or_else(|e| e.into_inner()).errors += 1;
        }
    }
    pub fn complete(&mut self, valid: Option<bool>) {
        self.complete = true;
        self.valid = valid;
    }
    fn record(&self, phase: &'static str, slot: Option<usize>, range: Range) {
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if s.owner != Some(std::thread::current().id())
            || s.depth != self.depth + 1
            || s.stack[self.depth] != self.scope
        {
            s.errors += 1;
            return;
        }
        if range.len > range.capacity
            || range.visible > range.len
            || !range.occupied && (range.len != 0 || range.capacity != 0 || range.visible != 0)
        {
            s.errors += 1;
        }
        if s.used == LIMIT {
            s.errors += 1;
            return;
        }
        let e = Event {
            observation: self.observation,
            call: self.call,
            scope: self.scope,
            parent: self.parent,
            api: self.api,
            variant: self.variant,
            block: self.block,
            phase,
            slot,
            range,
            offsets: (range.capacity != 0).then(|| {
                [
                    range.address % 64,
                    range.address % 4096,
                    range.address % s.page,
                ]
            }),
            elapsed_ns: s.epoch.unwrap().elapsed().as_nanos(),
            faults: faults(),
            snapshot: page_residency::query(range.address, range.capacity, s.page),
            complete: self.complete,
            valid: self.valid,
        };
        let used = s.used;
        s.events[used] = e;
        s.used += 1;
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        for (slot, range) in self.ranges.iter().copied().enumerate() {
            if range.capacity != 0 {
                self.record("released", Some(slot), range);
            }
        }
        self.record("exit", None, Range::default());
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if s.depth == self.depth + 1 && s.stack[self.depth] == self.scope {
            s.depth -= 1;
        } else {
            s.errors += 1;
        }
    }
}
struct Json<'a>(&'a State);
impl fmt::Display for Json<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0;
        write!(
            f,
            "{{\"schema\":\"stream-core-lifecycle-v1\",\"page_size\":{},\"errors\":{},\"calls\":{:?},\"release_approval\":false,\"events\":[",
            s.page, s.errors, s.calls
        )?;
        for (index, e) in s.events[..s.used].iter().enumerate() {
            if index != 0 {
                f.write_str(",")?;
            }
            write!(
                f,
                "{{\"observation\":{},\"call\":{},\"scope\":{},\"parent\":{},\"api\":\"{}\",\"variant\":\"{}\",\"block\":{},\"phase\":\"{}\",\"slot\":",
                e.observation, e.call, e.scope, e.parent, e.api, e.variant, e.block, e.phase
            )?;
            match e.slot {
                Some(n) => write!(f, "{n}")?,
                None => f.write_str("null")?,
            }
            write!(
                f,
                ",\"len\":{},\"capacity\":{},\"visible_len\":{},\"occupied\":{},\"offsets\":",
                e.range.len, e.range.capacity, e.range.visible, e.range.occupied
            )?;
            match e.offsets {
                Some(v) => write!(f, "{v:?}")?,
                None => f.write_str("null")?,
            }
            write!(f, ",\"elapsed_ns\":{},\"faults\":", e.elapsed_ns)?;
            match e.faults {
                Some(v) => write!(f, "{v:?}")?,
                None => f.write_str("null")?,
            }
            write!(
                f,
                ",\"snapshot\":{},\"complete\":{},\"valid\":",
                e.snapshot, e.complete
            )?;
            match e.valid {
                Some(v) => write!(f, "{v}")?,
                None => f.write_str("null")?,
            }
            f.write_str("}")?;
        }
        f.write_str("]}")
    }
}
pub fn stop() -> bool {
    ACTIVE.store(false, Ordering::Relaxed);
    let s = STATE.lock().unwrap();
    s.errors == 0 && s.depth == 0
}
pub fn export() -> bool {
    let valid = stop();
    println!("\nRSE_STREAM_CORE {}", Json(&STATE.lock().unwrap()));
    valid
}
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moved_and_absent_storage_is_not_reported_as_a_live_owner() {
        let _serial = TEST_LOCK.lock().unwrap();
        start(native_page_size().unwrap_or(4096));
        {
            let mut g = Guard::new("test", "option-transfer");
            let mut owner = vec![7u8; 4097];
            g.span(
                "reserved",
                0,
                0,
                owner.as_ptr(),
                owner.len(),
                owner.capacity(),
                owner.len(),
                true,
            );
            let moved = std::mem::take(&mut owner);
            g.span("moved", 0, 0, owner.as_ptr(), 0, 0, 0, true);
            g.span(
                "moved",
                1,
                0,
                moved.as_ptr(),
                moved.len(),
                moved.capacity(),
                moved.len(),
                true,
            );
            drop(moved);
            g.span::<u8>("cleared", 1, 0, std::ptr::null(), 0, 0, 0, false);
            g.complete(None);
        }
        assert!(stop());
        let s = STATE.lock().unwrap();
        assert!(!s.events[..s.used].iter().any(|e| e.phase == "released"));
        assert!(s.events[s.used - 1].complete);
    }
    #[test]
    fn early_exit_preserves_incomplete_scope_and_false_is_not_rewritten() {
        let _serial = TEST_LOCK.lock().unwrap();
        start(native_page_size().unwrap_or(4096));
        {
            let mut outer = Guard::new("verify_stream", "test");
            outer.next_block();
            {
                let mut child = Guard::new("verify_par", "test");
                child.complete(Some(false));
            }
            // The original stream can return false; recording must preserve it.
            outer.complete(Some(false));
        }
        drop(Guard::new("verify_stream", "interrupted"));
        assert!(stop());
        let s = STATE.lock().unwrap();
        assert_eq!(s.events[1].parent, s.events[0].scope);
        assert_eq!(s.events[1].block, 1);
        assert_eq!(s.events[2].valid, Some(false));
        assert!(!s.events[s.used - 1].complete);
    }
}
