//! Source-copy structural observer. Never linked into the shipped codec.
//! System requests are forwarded unchanged. The ledger contains request IDs and
//! modulo offsets, never addresses or inferred physical allocation identities.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::UnsafeCell,
    fmt,
    io::Write,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};
#[cfg(target_os = "linux")]
use std::{
    fs,
    io::{self, Read},
};
mod page_residency;
const EVENTS: usize = 65536;
const ALLOCATIONS: usize = 2048;
const THREADS: usize = 64;
const DEPTH: usize = 16;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct Allocation {
    id: usize,
    address: usize,
    bytes: usize,
    align: usize,
    live: bool,
}
impl Allocation {
    const EMPTY: Self = Self {
        id: 0,
        address: 0,
        bytes: 0,
        align: 0,
        live: false,
    };
}
#[derive(Clone, Copy)]
struct Thread {
    key: usize,
    depth: usize,
    roles: [&'static str; DEPTH],
}
impl Thread {
    const EMPTY: Self = Self {
        key: 0,
        depth: 0,
        roles: [""; DEPTH],
    };
}
#[derive(Clone, Copy)]
struct Event {
    kind: &'static str,
    observation: Option<usize>,
    call: usize,
    thread: usize,
    region: &'static str,
    allocation: Option<usize>,
    allocation_offset: Option<usize>,
    previous: Option<usize>,
    role: &'static str,
    stage: &'static str,
    index: usize,
    len: usize,
    capacity: Option<usize>,
    element_size: usize,
    shape: Option<[usize; 3]>,
    storage: &'static str,
    occupied: bool,
    present: Option<bool>,
    bytes: usize,
    align: usize,
    offsets: Option<[usize; 3]>,
    elapsed_ns: u128,
    faults: Option<[u64; 2]>,
    snapshot: page_residency::Snapshot,
}
impl Event {
    const EMPTY: Self = Self {
        kind: "",
        observation: None,
        call: 0,
        thread: 0,
        region: "",
        allocation: None,
        allocation_offset: None,
        previous: None,
        role: "",
        stage: "",
        index: 0,
        len: 0,
        capacity: None,
        element_size: 1,
        shape: None,
        storage: "",
        occupied: false,
        present: None,
        bytes: 0,
        align: 0,
        offsets: None,
        elapsed_ns: 0,
        faults: None,
        snapshot: page_residency::Snapshot::EMPTY,
    };
}
struct State {
    events: [Event; EVENTS],
    used: usize,
    errors: usize,
    page: usize,
    epoch: Option<Instant>,
    owner: usize,
    observation: Option<usize>,
    calls: [usize; 12],
    next_call: usize,
    call: usize,
    phase: &'static str,
    allocations: [Allocation; ALLOCATIONS],
    next_allocation: usize,
    threads: [Thread; THREADS],
    threads_used: usize,
}
// No OS mutex, TLS initialization or allocating wait primitive is used in
// allocator callbacks. This deliberately serializes diagnostic requests.
struct StateLock {
    held: AtomicBool,
    data: UnsafeCell<State>,
}
// SAFETY: every State access requires the acquire/release guard below. State
// contains only Send scalar/array/static-string/Instant fields; no borrowed
// caller references are retained. The guard exposes no unprotected data.
unsafe impl Sync for StateLock {}
struct StateGuard<'a> {
    lock: &'a StateLock,
    _not_send: core::marker::PhantomData<*mut ()>,
}
impl StateLock {
    const fn new(data: State) -> Self {
        Self {
            held: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
    fn lock(&self) -> StateGuard<'_> {
        while self
            .held
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        StateGuard {
            lock: self,
            _not_send: core::marker::PhantomData,
        }
    }
}
impl Deref for StateGuard<'_> {
    type Target = State;
    fn deref(&self) -> &State {
        // SAFETY: this guard exclusively owns the lock until its Drop releases it.
        unsafe { &*self.lock.data.get() }
    }
}
impl DerefMut for StateGuard<'_> {
    fn deref_mut(&mut self) -> &mut State {
        // SAFETY: the lock is held, and the unique guard is mutably borrowed.
        unsafe { &mut *self.lock.data.get() }
    }
}
impl Drop for StateGuard<'_> {
    fn drop(&mut self) {
        self.lock.held.store(false, Ordering::Release);
    }
}
static STATE: StateLock = StateLock::new(State {
    events: [Event::EMPTY; EVENTS],
    used: 0,
    errors: 0,
    page: 4096,
    epoch: None,
    owner: 0,
    observation: None,
    calls: [0; 12],
    next_call: 0,
    call: 0,
    phase: "setup",
    allocations: [Allocation::EMPTY; ALLOCATIONS],
    next_allocation: 0,
    threads: [Thread::EMPTY; THREADS],
    threads_used: 0,
});
fn thread_key() -> usize {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        unsafe extern "C" {
            fn pthread_self() -> usize;
        }
        // SAFETY: pthread_self takes no arguments and returns the calling
        // thread's opaque pointer/integer-sized identity on supported targets.
        unsafe { pthread_self() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        0
    }
}
impl State {
    fn thread(&mut self) -> usize {
        let key = thread_key();
        if let Some(i) = self.threads[..self.threads_used]
            .iter()
            .position(|t| t.key == key)
        {
            return i;
        }
        if key == 0 || self.threads_used == THREADS {
            self.errors = self.errors.saturating_add(1);
            return 0;
        }
        let i = self.threads_used;
        self.threads_used += 1;
        self.threads[i] = Thread {
            key,
            ..Thread::EMPTY
        };
        i
    }
    fn event(&mut self, kind: &'static str) -> Event {
        let thread = self.thread();
        let t = self.threads[thread];
        Event {
            kind,
            observation: self.observation,
            call: self.call,
            thread,
            region: if t.depth == 0 {
                self.phase
            } else {
                t.roles[t.depth - 1]
            },
            elapsed_ns: self.epoch.map_or(0, |v| v.elapsed().as_nanos()),
            ..Event::EMPTY
        }
    }
    fn push(&mut self, event: Event) {
        if self.used == EVENTS {
            self.errors = self.errors.saturating_add(1);
            return;
        }
        self.events[self.used] = event;
        self.used += 1;
    }
    fn find(&self, address: usize) -> Option<usize> {
        self.allocations
            .iter()
            .position(|a| a.live && a.address == address)
    }
    fn install(&mut self, address: usize, bytes: usize, align: usize) -> Option<usize> {
        if address == 0 {
            self.errors = self.errors.saturating_add(1);
            return None;
        }
        if self.find(address).is_some() {
            self.errors = self.errors.saturating_add(1);
            return None;
        }
        let Some(slot) = self.allocations.iter().position(|a| !a.live) else {
            self.errors = self.errors.saturating_add(1);
            return None;
        };
        let Some(id) = self.next_allocation.checked_add(1) else {
            self.errors = self.errors.saturating_add(1);
            return None;
        };
        self.next_allocation = id;
        self.allocations[slot] = Allocation {
            id: self.next_allocation,
            address,
            bytes,
            align,
            live: true,
        };
        Some(self.next_allocation)
    }
    fn offsets(&self, address: usize, bytes: usize) -> Option<[usize; 3]> {
        (bytes != 0 && address != 0).then(|| [address % 64, address % 4096, address % self.page])
    }
}

/// Installed only by the generated diagnostic test executable, not this module.
pub struct Allocator;
// SAFETY: all original pointers, layouts and requested sizes are forwarded to
// System. Fixed storage, native queries and scalar locking never invoke Rust
// allocation. The observer changes timing/concurrency and is not a benchmark.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        unsafe { self.allocate(layout, false) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: preserve the original zeroed allocation contract.
        unsafe { self.allocate(layout, true) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let generation = GENERATION.load(Ordering::Acquire);
        if !ACTIVE.load(Ordering::Acquire) {
            // SAFETY: forward the original System allocation unchanged.
            unsafe { System.dealloc(pointer, layout) };
            return;
        }
        let mut s = STATE.lock();
        if !ACTIVE.load(Ordering::Acquire) || GENERATION.load(Ordering::Acquire) != generation {
            // SAFETY: the original valid allocation is forwarded after the
            // observation cutoff; no event can mutate an exported ledger.
            unsafe { System.dealloc(pointer, layout) };
            return;
        }
        let slot = s.find(pointer as usize);
        if let Some(i) = slot
            && (s.allocations[i].bytes != layout.size() || s.allocations[i].align != layout.align())
        {
            s.errors = s.errors.saturating_add(1);
        }
        // SAFETY: the observer has not changed the pointer or original layout.
        unsafe { System.dealloc(pointer, layout) };
        let mut e = s.event("free");
        e.bytes = layout.size();
        e.align = layout.align();
        e.allocation = slot.map(|i| s.allocations[i].id);
        e.allocation_offset = e.allocation.map(|_| 0);
        if let Some(i) = slot {
            s.allocations[i].live = false;
        }
        e.offsets = s.offsets(pointer as usize, layout.size());
        // A numerical former-range query reads no freed contents. A mapped
        // range does not prove that the original allocation remains alive.
        e.snapshot = page_residency::query(pointer as usize, layout.size(), s.page);
        s.push(e);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
        let generation = GENERATION.load(Ordering::Acquire);
        if !ACTIVE.load(Ordering::Acquire) {
            // SAFETY: forward the original pointer/layout and new size.
            return unsafe { System.realloc(pointer, layout, bytes) };
        }
        let mut s = STATE.lock();
        if !ACTIVE.load(Ordering::Acquire) || GENERATION.load(Ordering::Acquire) != generation {
            // SAFETY: preserve the original realloc request after cutoff.
            return unsafe { System.realloc(pointer, layout, bytes) };
        }
        let old = s.find(pointer as usize);
        let previous = old.map(|i| s.allocations[i].id);
        // SAFETY: forward the original allocation unchanged to System.
        let result = unsafe { System.realloc(pointer, layout, bytes) };
        let mut e = s.event("realloc");
        e.bytes = bytes;
        e.align = layout.align();
        e.previous = previous;
        if !result.is_null() {
            if let Some(i) = old {
                s.allocations[i].live = false;
            }
            e.allocation = s.install(result as usize, bytes, layout.align());
            e.allocation_offset = e.allocation.map(|_| 0);
        } else {
            s.errors = s.errors.saturating_add(1);
        }
        e.offsets = s.offsets(result as usize, bytes);
        e.snapshot = page_residency::query(
            result as usize,
            if result.is_null() { 0 } else { bytes },
            s.page,
        );
        s.push(e);
        result
    }
}
impl Allocator {
    /// # Safety
    /// `layout` is a nonzero valid allocation request, as required by GlobalAlloc.
    unsafe fn allocate(&self, layout: Layout, zeroed: bool) -> *mut u8 {
        let generation = GENERATION.load(Ordering::Acquire);
        if !ACTIVE.load(Ordering::Acquire) {
            // SAFETY: forward the valid original layout and initialization mode.
            return unsafe {
                if zeroed {
                    System.alloc_zeroed(layout)
                } else {
                    System.alloc(layout)
                }
            };
        }
        let mut s = STATE.lock();
        if !ACTIVE.load(Ordering::Acquire) || GENERATION.load(Ordering::Acquire) != generation {
            // SAFETY: unchanged valid request and original initialization mode.
            return unsafe {
                if zeroed {
                    System.alloc_zeroed(layout)
                } else {
                    System.alloc(layout)
                }
            };
        }
        // SAFETY: original allocator request, with no observer-owned buffer.
        let pointer = unsafe {
            if zeroed {
                System.alloc_zeroed(layout)
            } else {
                System.alloc(layout)
            }
        };
        let mut e = s.event(if zeroed { "alloc_zeroed" } else { "alloc" });
        e.bytes = layout.size();
        e.align = layout.align();
        e.allocation = s.install(pointer as usize, layout.size(), layout.align());
        e.allocation_offset = e.allocation.map(|_| 0);
        e.offsets = s.offsets(pointer as usize, layout.size());
        e.snapshot = page_residency::query(
            pointer as usize,
            if pointer.is_null() { 0 } else { layout.size() },
            s.page,
        );
        s.push(e);
        pointer
    }
}

pub struct Root {
    active: bool,
    export: bool,
}
impl Root {
    pub fn start(page: usize) -> Self {
        let mut s = STATE.lock();
        if ACTIVE.load(Ordering::Acquire)
            || thread_key() == 0
            || !page.is_power_of_two()
            || !(1024..=1048576).contains(&page)
        {
            s.errors = s.errors.saturating_add(1);
            return Self {
                active: false,
                export: false,
            };
        }
        s.used = 0;
        s.errors = 0;
        s.page = page;
        s.owner = thread_key();
        s.epoch = Some(Instant::now());
        s.observation = None;
        s.calls = [0; 12];
        s.call = 0;
        s.next_call = 0;
        s.phase = "setup";
        s.next_allocation = 0;
        s.threads_used = 0;
        s.allocations.fill(Allocation::EMPTY);
        s.threads.fill(Thread::EMPTY);
        s.thread();
        GENERATION.fetch_add(1, Ordering::AcqRel);
        ACTIVE.store(true, Ordering::Release);
        Self {
            active: true,
            export: true,
        }
    }
    pub fn quiet(&mut self) {
        self.export = false;
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        ACTIVE.store(false, Ordering::Release);
        let mut s = STATE.lock();
        if s.call != 0 || s.threads[..s.threads_used].iter().any(|t| t.depth != 0) {
            s.errors = s.errors.saturating_add(1);
        }
        let finish = s.event("finish");
        s.push(finish);
        if self.export {
            let mut output = std::io::stdout().lock();
            let _ = writeln!(output, "\nRSE_DYNAMIC_BUFFERS {}", Report(&s));
        }
    }
}
pub fn observation(index: usize) {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let mut s = STATE.lock();
    if index >= 12 || s.owner != thread_key() || s.call != 0 {
        s.errors = s.errors.saturating_add(1);
        return;
    }
    s.observation = Some(index);
}
pub struct Epoch {
    active: bool,
    id: usize,
}
impl Epoch {
    pub fn begin() -> Self {
        if !ACTIVE.load(Ordering::Acquire) {
            return Self {
                active: false,
                id: 0,
            };
        }
        let mut s = STATE.lock();
        if s.call != 0 || s.owner != thread_key() || s.observation.is_none() {
            s.errors = s.errors.saturating_add(1);
            return Self {
                active: false,
                id: 0,
            };
        }
        let observation = s.observation.unwrap();
        s.calls[observation] += 1;
        s.next_call += 1;
        s.call = s.next_call;
        s.phase = "caller";
        let id = s.call;
        let e = s.event("begin");
        s.push(e);
        Self { active: true, id }
    }
}
impl Drop for Epoch {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut s = STATE.lock();
        if s.call != self.id || s.owner != thread_key() {
            s.errors = s.errors.saturating_add(1);
            return;
        }
        let e = s.event("end");
        s.push(e);
        s.call = 0;
        s.phase = "between";
    }
}
pub struct Region {
    active: bool,
    thread: usize,
    depth: usize,
}
impl Region {
    pub fn new(role: &'static str) -> Self {
        if !ACTIVE.load(Ordering::Acquire) {
            return Self {
                active: false,
                thread: 0,
                depth: 0,
            };
        }
        let mut s = STATE.lock();
        let thread = s.thread();
        let depth = s.threads[thread].depth;
        if depth == DEPTH {
            s.errors = s.errors.saturating_add(1);
            return Self {
                active: false,
                thread,
                depth,
            };
        }
        s.threads[thread].roles[depth] = role;
        s.threads[thread].depth += 1;
        if s.owner == thread_key() {
            s.phase = role;
        }
        let e = s.event("enter");
        s.push(e);
        Self {
            active: true,
            thread,
            depth,
        }
    }
}
impl Drop for Region {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut s = STATE.lock();
        if s.threads[self.thread].key != thread_key()
            || s.threads[self.thread].depth != self.depth + 1
        {
            s.errors = s.errors.saturating_add(1);
            return;
        }
        let e = s.event("leave");
        s.push(e);
        s.threads[self.thread].depth -= 1;
        if s.owner == thread_key() {
            s.phase = if self.depth != 0 {
                s.threads[self.thread].roles[self.depth - 1]
            } else if s.call != 0 {
                "caller"
            } else {
                "setup"
            };
        }
    }
}

/// Observe initialized elements and the owner's true capacity, or a borrowed
/// view with unknown capacity. No element or uninitialized capacity is read.
#[allow(clippy::too_many_arguments)]
pub fn span<T>(
    role: &'static str,
    stage: &'static str,
    index: usize,
    pointer: *const T,
    len: usize,
    capacity: Option<usize>,
    storage: &'static str,
    occupied: bool,
) {
    view(
        role, stage, index, pointer, len, capacity, storage, occupied, None, None,
    );
}
#[allow(clippy::too_many_arguments)]
fn view<T>(
    role: &'static str,
    stage: &'static str,
    index: usize,
    pointer: *const T,
    len: usize,
    capacity: Option<usize>,
    storage: &'static str,
    occupied: bool,
    present: Option<bool>,
    shape: Option<[usize; 3]>,
) {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let element = std::mem::size_of::<T>();
    let Some(bytes) = capacity.unwrap_or(len).checked_mul(element) else {
        STATE.lock().errors += 1;
        return;
    };
    let faults = faults();
    let mut s = STATE.lock();
    if element == 0
        || capacity.is_some_and(|c| len > c)
        || !occupied && (len != 0 || capacity.unwrap_or(0) != 0)
    {
        s.errors = s.errors.saturating_add(1);
        return;
    }
    let address = pointer as usize;
    let mut e = s.event("view");
    e.role = role;
    e.stage = stage;
    e.index = index;
    e.len = len;
    e.capacity = capacity;
    e.element_size = element;
    e.shape = shape;
    e.storage = storage;
    e.occupied = occupied;
    e.present = present;
    e.bytes = bytes;
    e.faults = faults;
    e.offsets = s.offsets(address, bytes);
    if let Some(a) = s.allocations.iter().find(|a| {
        a.live
            && bytes != 0
            && address >= a.address
            && address
                .checked_add(bytes)
                .zip(a.address.checked_add(a.bytes))
                .is_some_and(|(end, limit)| end <= limit)
    }) {
        e.allocation = Some(a.id);
        e.allocation_offset = Some(address - a.address);
    }
    e.snapshot = page_residency::query(address, bytes, s.page);
    s.push(e);
}
pub fn elements<T>(role: &'static str, stage: &'static str, shards: &[Vec<T>]) {
    for (i, v) in shards.iter().enumerate() {
        view(
            role,
            stage,
            i,
            v.as_ptr(),
            v.len(),
            Some(v.capacity()),
            "heap",
            true,
            Some(true),
            None,
        );
    }
}
pub fn optional_elements<T>(role: &'static str, stage: &'static str, shards: &[Option<Vec<T>>]) {
    for (i, v) in shards.iter().enumerate() {
        if let Some(v) = v {
            view(
                role,
                stage,
                i,
                v.as_ptr(),
                v.len(),
                Some(v.capacity()),
                "heap",
                true,
                Some(true),
                None,
            );
        } else {
            view::<T>(
                role,
                stage,
                i,
                core::ptr::null(),
                0,
                Some(0),
                "empty",
                false,
                Some(false),
                None,
            );
        }
    }
}
pub fn buffers(role: &'static str, stage: &'static str, shards: &[Vec<u8>]) {
    for (i, v) in shards.iter().enumerate() {
        span(
            role,
            stage,
            i,
            v.as_ptr(),
            v.len(),
            Some(v.capacity()),
            "heap",
            true,
        );
    }
}
pub fn vectors(role: &'static str, stage: &'static str, shards: &[Vec<u8>]) {
    elements(role, stage, shards);
}
pub fn optional(role: &'static str, stage: &'static str, shards: &[Option<Vec<u8>>]) {
    optional_elements(role, stage, shards);
}
/// Decode work is initialized by its original zeroed constructor; shape also
/// records the active prefix without forming any additional data references.
pub fn work<T>(
    role: &'static str,
    stage: &'static str,
    pointer: *const T,
    capacity: usize,
    shape: [usize; 3],
) {
    view(
        role,
        stage,
        0,
        pointer,
        capacity,
        Some(capacity),
        "heap",
        true,
        None,
        Some(shape),
    );
}

pub fn slots(role: &'static str, stage: &'static str, shards: &[crate::ShardSlot<Vec<u8>>]) {
    for (i, v) in shards.iter().enumerate() {
        let b = v.as_inner();
        view(
            role,
            stage,
            i,
            b.as_ptr(),
            b.len(),
            Some(b.capacity()),
            "heap",
            true,
            Some(v.is_present()),
            None,
        );
    }
}

fn faults() -> Option<[u64; 2]> {
    #[cfg(target_os = "linux")]
    {
        process_faults()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
struct Json<T>(Option<T>);
impl<T: fmt::Debug> fmt::Display for Json<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(v) = &self.0 {
            write!(f, "{v:?}")
        } else {
            f.write_str("null")
        }
    }
}
struct Report<'a>(&'a State);
impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0;
        write!(
            f,
            "{{\"schema\":\"dynamic-buffer-lifecycle-v1\",\"page_size\":{},\"errors\":{},\"calls\":{:?},\"release_approval\":false,\"events\":[",
            s.page, s.errors, s.calls
        )?;
        for (i, e) in s.events[..s.used].iter().enumerate() {
            if i != 0 {
                f.write_str(",")?;
            }
            write!(
                f,
                concat!(
                    "{{\"kind\":\"{}\",\"observation\":{},\"call\":{},\"thread\":{},\"region\":\"{}\",",
                    "\"allocation\":{},\"allocation_offset\":{},\"previous\":{},\"role\":\"{}\",\"stage\":\"{}\",\"index\":{},",
                    "\"len\":{},\"capacity\":{},\"element_size\":{},\"shape\":{},\"storage\":\"{}\",\"occupied\":{},\"present\":{},",
                    "\"bytes\":{},\"align\":{},\"offsets\":{},\"elapsed_ns\":{},\"faults\":{},\"snapshot\":{}}}"
                ),
                e.kind,
                Json(e.observation),
                e.call,
                e.thread,
                e.region,
                Json(e.allocation),
                Json(e.allocation_offset),
                Json(e.previous),
                e.role,
                e.stage,
                e.index,
                e.len,
                Json(e.capacity),
                e.element_size,
                Json(e.shape),
                e.storage,
                e.occupied,
                Json(e.present),
                e.bytes,
                e.align,
                Json(e.offsets),
                e.elapsed_ns,
                Json(e.faults),
                e.snapshot
            )?;
        }
        f.write_str("]}")
    }
}

#[cfg(test)]
pub fn test_errors() -> usize {
    STATE.lock().errors
}

#[cfg(test)]
pub fn test_page_size() -> usize {
    page_residency::page_size().unwrap_or(4096)
}

pub fn errors() -> usize {
    STATE.lock().errors
}

pub fn native_page_size() -> usize {
    page_residency::page_size().unwrap_or(4096)
}
