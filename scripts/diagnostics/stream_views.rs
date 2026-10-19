//! Bounded, diagnostic-only views at the real stream I/O boundary.
//! No source patch, allocator replacement, pointer dereference or address export.
use std::fmt::{self, Write as _};
#[cfg(target_os = "linux")]
use std::fs;
use std::io::{self, Read, Write};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

const MAX_EVENTS: usize = 4096;
const MAX_SHARDS: usize = 48;
const ROLES: [&str; 9] = [
    "retained_source",
    "source_read_view",
    "read_scratch_view",
    "parity_write_view",
    "writer_before",
    "writer_after",
    "cursor_before_missing",
    "cursor_after_recovery",
    "cursor_missing",
];

#[derive(Clone, Copy, Default)]
struct Event {
    call: usize,
    role: usize,
    index: usize,
    len: usize,
    capacity: Option<usize>,
    offsets: Option<[usize; 3]>,
    bytes: usize,
    reused: Option<bool>,
}

// These diagnostic windows do not modify the ordinary sampler or release gates.
// Whole-process faults include pool threads; scheduler counters cover only the
// calling thread. Procfs reads and metadata can perturb timing/page residency.
const PHASES: [&str; 4] = ["call", "reserve", "append", "drop"];
#[derive(Clone, Copy, Default)]
struct Window {
    call: usize,
    phase: usize,
    index: Option<usize>,
    bytes: usize,
    start_ns: u128,
    end_ns: u128,
    before: Option<[u64; 4]>,
    after: Option<[u64; 4]>,
}
#[derive(Clone, Copy)]
struct Token {
    window: Window,
}
fn counters() -> Option<[u64; 4]> {
    #[cfg(target_os = "linux")]
    {
        // The two parsers below are extracted unchanged from the ordinary
        // observer by the renderer, so this has the same bounded/no-heap rules.
        let mut buffer = [0u8; 4096];
        let (minor, major) =
            process_faults(read_bounded("/proc/self/stat", &mut buffer).unwrap()).unwrap();
        let mut buffer = [0u8; 256];
        let mut fields = read_bounded("/proc/thread-self/schedstat", &mut buffer)
            .unwrap()
            .split_whitespace();
        let run = fields.next().unwrap().parse().unwrap();
        let wait = fields.next().unwrap().parse().unwrap();
        Some([minor, major, run, wait])
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
fn start(phase: usize, index: Option<usize>, bytes: usize) -> Option<Token> {
    if !enabled() {
        return None;
    }
    let (call, epoch) = {
        let state = STATE.lock().unwrap();
        assert!(
            state.owner == Some(std::thread::current().id()),
            "writer window outside measuring thread"
        );
        (state.calls, state.epoch.unwrap())
    };
    let before = counters();
    Some(Token {
        window: Window {
            call,
            phase,
            index,
            bytes,
            start_ns: epoch.elapsed().as_nanos(),
            before,
            ..Window::default()
        },
    })
}
fn end(token: Option<Token>) {
    let Some(mut token) = token else {
        return;
    };
    token.window.end_ns = STATE.lock().unwrap().epoch.unwrap().elapsed().as_nanos();
    token.window.after = counters();
    let mut state = STATE.lock().unwrap();
    let used = state.windows_used;
    if used == MAX_EVENTS {
        state.overflow += 1;
        return;
    }
    state.windows[used] = token.window;
    state.windows_used += 1;
}
pub(super) fn drop_outputs(values: Vec<Writer>) {
    let token = start(3, None, values.iter().map(|v| v.0.len()).sum());
    drop(values);
    end(token);
}

#[derive(Clone, Copy, Default)]
struct Bounds {
    address: usize,
    count: usize,
    stride: usize,
}
impl Bounds {
    fn index(self, address: usize) -> usize {
        let delta = address
            .checked_sub(self.address)
            .expect("unregistered I/O adapter");
        assert!(self.stride > 0 && delta.is_multiple_of(self.stride));
        let index = delta / self.stride;
        assert!(index < self.count, "I/O adapter outside registered storage");
        index
    }
}

struct State {
    events: [Event; MAX_EVENTS],
    used: usize,
    overflow: usize,
    calls: usize,
    observation: usize,
    completed: usize,
    iterations: usize,
    page_size: usize,
    last: [[Option<usize>; MAX_SHARDS]; 9],
    readers: Bounds,
    writers: Bounds,
    writer_capacity: [usize; MAX_SHARDS],
    epoch: Option<Instant>,
    owner: Option<std::thread::ThreadId>,
    active_call: Option<Token>,
    windows: [Window; MAX_EVENTS],
    windows_used: usize,
}
impl State {
    const fn new() -> Self {
        Self {
            events: [Event {
                call: 0,
                role: 0,
                index: 0,
                len: 0,
                capacity: None,
                offsets: None,
                bytes: 0,
                reused: None,
            }; MAX_EVENTS],
            used: 0,
            overflow: 0,
            calls: 0,
            observation: 0,
            completed: 0,
            iterations: 0,
            page_size: 4096,
            last: [[None; MAX_SHARDS]; 9],
            readers: Bounds {
                address: 0,
                count: 0,
                stride: 0,
            },
            writers: Bounds {
                address: 0,
                count: 0,
                stride: 0,
            },
            writer_capacity: [0; MAX_SHARDS],
            epoch: None,
            owner: None,
            active_call: None,
            windows: [Window {
                call: 0,
                phase: 0,
                index: None,
                bytes: 0,
                start_ns: 0,
                end_ns: 0,
                before: None,
                after: None,
            }; MAX_EVENTS],
            windows_used: 0,
        }
    }
    fn record(
        &mut self,
        role: usize,
        index: usize,
        address: usize,
        len: usize,
        capacity: Option<usize>,
        bytes: usize,
    ) {
        assert!(role < ROLES.len() && index < MAX_SHARDS);
        assert!(capacity.is_none_or(|cap| len <= cap));
        let has_storage = capacity.map_or(len > 0, |cap| cap > 0);
        let previous = self.last[role][index];
        self.last[role][index] = has_storage.then_some(address);
        if self.used == MAX_EVENTS {
            self.overflow += 1;
            return;
        }
        self.events[self.used] = Event {
            call: self.calls,
            role,
            index,
            len,
            capacity,
            offsets: has_storage.then_some([
                address % 64,
                address % 4096,
                address % self.page_size,
            ]),
            bytes,
            reused: if has_storage {
                previous.map(|old| old == address)
            } else {
                None
            },
        };
        self.used += 1;
    }
}
static STATE: Mutex<State> = Mutex::new(State::new());
static ENABLED: AtomicBool = AtomicBool::new(false);
pub(super) fn enable() {
    ENABLED.store(true, Ordering::Release);
}
fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}
#[cfg(test)]
pub(super) fn disable() {
    ENABLED.store(false, Ordering::Release);
}
#[cfg(test)]
pub(super) fn test_session() -> std::sync::MutexGuard<'static, ()> {
    static SESSION: Mutex<()> = Mutex::new(());
    SESSION.lock().unwrap()
}

pub(super) fn begin(source: &[Vec<u8>], iterations: usize, page_size: usize) {
    if !enabled() {
        return;
    }
    assert!(page_size.is_power_of_two() && (1024..=1048576).contains(&page_size));
    let mut state = STATE.lock().unwrap();
    state.observation += 1;
    state.used = 0;
    state.windows_used = 0;
    state.epoch = Some(Instant::now());
    state.owner = Some(std::thread::current().id());
    state.active_call = None;
    state.overflow = 0;
    state.calls = 0;
    state.completed = 0;
    state.iterations = iterations;
    state.page_size = page_size;
    state.last = [[None; MAX_SHARDS]; 9];
    for (index, buffer) in source.iter().enumerate() {
        state.record(
            0,
            index,
            buffer.as_ptr() as usize,
            buffer.len(),
            Some(buffer.capacity()),
            0,
        );
    }
}
pub(super) fn call() {
    if enabled() {
        {
            let mut state = STATE.lock().unwrap();
            assert!(state.active_call.is_none(), "overlapping stream calls");
            state.calls += 1;
        }
        let token = start(0, None, 0);
        STATE.lock().unwrap().active_call = token;
    }
}
pub(super) fn end_call() {
    if enabled() {
        let token = STATE.lock().unwrap().active_call.take();
        assert!(token.is_some(), "unpaired stream call");
        end(token);
        let mut state = STATE.lock().unwrap();
        state.completed += 1;
        state.readers = Bounds::default();
        state.writers = Bounds::default();
    }
}
pub(super) fn readers(values: &[Reader<'_>]) {
    if enabled() {
        STATE.lock().unwrap().readers = Bounds {
            address: values.as_ptr() as usize,
            count: values.len(),
            stride: std::mem::size_of::<Reader<'_>>(),
        };
    }
}
pub(super) fn writers(values: &[Writer]) {
    if enabled() {
        STATE.lock().unwrap().writers = Bounds {
            address: values.as_ptr() as usize,
            count: values.len(),
            stride: std::mem::size_of::<Writer>(),
        };
    }
}
pub(super) fn cursors(values: &[std::io::Cursor<Vec<u8>>], role: usize) {
    if !enabled() {
        return;
    }
    let mut state = STATE.lock().unwrap();
    for (index, cursor) in values.iter().enumerate() {
        let buffer = cursor.get_ref();
        state.record(
            role,
            index,
            buffer.as_ptr() as usize,
            buffer.len(),
            Some(buffer.capacity()),
            0,
        );
    }
}

#[repr(transparent)]
pub(super) struct Reader<'a>(pub(super) &'a [u8]);
impl Read for Reader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let source = self.0;
        let result = self.0.read(buffer);
        if enabled() {
            let mut state = STATE.lock().unwrap();
            let index = state.readers.index(self as *const Self as usize);
            let count = result.as_ref().copied().unwrap_or(0);
            state.record(
                1,
                index,
                source.as_ptr() as usize,
                source.len(),
                None,
                count,
            );
            state.record(
                2,
                index,
                buffer.as_ptr() as usize,
                buffer.len(),
                None,
                count,
            );
        }
        result
    }
}
#[repr(transparent)]
pub(super) struct Writer(pub(super) Vec<u8>);
impl Writer {
    fn append_window(&self, bytes: usize) -> Option<Token> {
        if !enabled() {
            return None;
        }
        let index = STATE
            .lock()
            .unwrap()
            .writers
            .index(self as *const Self as usize);
        start(2, Some(index), bytes)
    }
    fn before(&self, buffer: &[u8]) {
        if !enabled() {
            return;
        }
        let mut state = STATE.lock().unwrap();
        let index = state.writers.index(self as *const Self as usize);
        state.writer_capacity[index] = self.0.capacity();
        state.record(
            3,
            index,
            buffer.as_ptr() as usize,
            buffer.len(),
            None,
            buffer.len(),
        );
        state.record(
            4,
            index,
            self.0.as_ptr() as usize,
            self.0.len(),
            Some(self.0.capacity()),
            0,
        );
    }
    fn after(&self, count: usize) {
        if !enabled() {
            return;
        }
        let mut state = STATE.lock().unwrap();
        let index = state.writers.index(self as *const Self as usize);
        assert_eq!(
            state.writer_capacity[index],
            self.0.capacity(),
            "writer reservation changed"
        );
        if self.0.capacity() > 0 {
            assert!(
                state.last[4][index] == Some(self.0.as_ptr() as usize),
                "writer storage moved"
            );
        }
        state.record(
            5,
            index,
            self.0.as_ptr() as usize,
            self.0.len(),
            Some(self.0.capacity()),
            count,
        );
    }
}
impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.before(buffer);
        let token = self.append_window(buffer.len());
        let result = self.0.write(buffer);
        end(token);
        self.after(result.as_ref().copied().unwrap_or(0));
        result
    }
    fn write_all(&mut self, buffer: &[u8]) -> io::Result<()> {
        self.before(buffer);
        let token = self.append_window(buffer.len());
        let result = self.0.write_all(buffer);
        end(token);
        self.after(if result.is_ok() { buffer.len() } else { 0 });
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
pub(super) fn outputs(count: usize, size: usize) -> Vec<Writer> {
    let token = start(1, None, count.checked_mul(size).unwrap());
    let result = (0..count)
        .map(|_| Writer(Vec::with_capacity(size)))
        .collect();
    end(token);
    result
}

struct JsonState<'a>(&'a State);
impl fmt::Display for JsonState<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.0;
        write!(
            f,
            "{{\"schema\":\"stream-buffer-views-v2\",\"observation\":{},\"iterations\":{},\"calls\":{},\"completed\":{},\"page_size\":{},\"overflow\":{},\"release_approval\":false,\"events\":[",
            state.observation - 1,
            state.iterations,
            state.calls,
            state.completed,
            state.page_size,
            state.overflow
        )?;
        for (n, event) in state.events[..state.used].iter().enumerate() {
            if n > 0 {
                f.write_char(',')?;
            }
            write!(
                f,
                "{{\"call\":{},\"role\":\"{}\",\"index\":{},\"len\":{},\"capacity\":",
                event.call, ROLES[event.role], event.index, event.len
            )?;
            match event.capacity {
                Some(cap) => write!(f, "{cap}")?,
                None => f.write_str("null")?,
            }
            f.write_str(",\"offsets\":")?;
            match event.offsets {
                Some(offsets) => write!(f, "{:?}", offsets)?,
                None => f.write_str("null")?,
            }
            write!(
                f,
                ",\"bytes\":{},\"same_address_as_previous_view\":",
                event.bytes
            )?;
            match event.reused {
                Some(value) => write!(f, "{value}")?,
                None => f.write_str("null")?,
            }
            f.write_char('}')?;
        }
        f.write_str("],\"windows\":[")?;
        for (n, window) in state.windows[..state.windows_used].iter().enumerate() {
            if n > 0 {
                f.write_char(',')?;
            }
            write!(
                f,
                "{{\"call\":{},\"phase\":\"{}\",\"index\":",
                window.call, PHASES[window.phase]
            )?;
            match window.index {
                Some(index) => write!(f, "{index}")?,
                None => f.write_str("null")?,
            }
            write!(
                f,
                ",\"bytes\":{},\"start_ns\":{},\"end_ns\":{},\"before\":",
                window.bytes, window.start_ns, window.end_ns
            )?;
            for (i, counters) in [window.before, window.after].iter().enumerate() {
                if i > 0 {
                    f.write_str(",\"after\":")?;
                }
                match counters {
                    Some(values) => write!(f, "{values:?}")?,
                    None => f.write_str("null")?,
                }
            }
            f.write_char('}')?;
        }
        f.write_str("]}")
    }
}
pub(super) fn finish() {
    if !enabled() {
        return;
    }
    let state = STATE.lock().unwrap();
    println!("\nRSE_STREAM_VIEWS {}", JsonState(&state));
    assert_eq!(state.calls, state.iterations);
    assert_eq!(state.completed, state.calls);
    assert_eq!(
        state.overflow, 0,
        "bounded view audit overflow; retained incomplete diagnostic"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapters_preserve_layout_and_slice_vec_io() {
        let _session = test_session();
        assert_eq!(
            std::mem::size_of::<Reader<'_>>(),
            std::mem::size_of::<&[u8]>()
        );
        assert_eq!(
            std::mem::align_of::<Reader<'_>>(),
            std::mem::align_of::<&[u8]>()
        );
        assert_eq!(
            std::mem::size_of::<Writer>(),
            std::mem::size_of::<Vec<u8>>()
        );
        assert_eq!(
            std::mem::align_of::<Writer>(),
            std::mem::align_of::<Vec<u8>>()
        );
        // Semantic tests run with the ordinary test entry. Activation is
        // serialized with the independent heap probe.
        if enabled() {
            return;
        }
        let mut input = Reader(b"short tail");
        let mut target = Writer(Vec::with_capacity(32));
        let mut buffer = [0u8; 6];
        loop {
            let n = input.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            target.write_all(&buffer[..n]).unwrap();
        }
        assert_eq!(target.0, b"short tail");
        assert_eq!(target.0.capacity(), 32);
    }
    #[test]
    fn adapters_preserve_stream_parity_and_tail_verification() {
        let _session = test_session();
        if enabled() {
            return;
        }
        use rustfs_erasure_codec::galois_8::ReedSolomon;
        use rustfs_erasure_codec::stream::{StreamIoMode, StreamOptions};
        for (data_count, parity_count) in [(4, 2), (10, 4)] {
            for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
                let codec = ReedSolomon::new(data_count, parity_count).unwrap();
                let source: Vec<_> = (0..data_count).map(|i| vec![i as u8 + 1; 2051]).collect();
                let options = StreamOptions::new()
                    .with_block_size(1024)
                    .with_io_mode(mode);
                let mut original: Vec<_> = source.iter().map(Vec::as_slice).collect();
                let mut expected: Vec<_> = (0..parity_count)
                    .map(|_| Vec::with_capacity(2051))
                    .collect();
                codec
                    .encode_stream(&mut original, &mut expected, &options)
                    .unwrap();
                let mut readers: Vec<_> = source.iter().map(|v| Reader(v.as_slice())).collect();
                let mut writers = outputs(parity_count, 2051);
                codec
                    .encode_stream(&mut readers, &mut writers, &options)
                    .unwrap();
                assert!(writers.iter().zip(&expected).all(|(a, b)| a.0 == *b));
                let mut readers: Vec<_> = source
                    .iter()
                    .chain(&expected)
                    .map(|v| Reader(v.as_slice()))
                    .collect();
                assert!(codec.verify_stream(&mut readers, &options).unwrap());
                expected[0][2050] ^= 1;
                let mut readers: Vec<_> = source
                    .iter()
                    .chain(&expected)
                    .map(|v| Reader(v.as_slice()))
                    .collect();
                assert!(!codec.verify_stream(&mut readers, &options).unwrap());
            }
        }
    }

    #[test]
    fn empty_views_do_not_export_dangling_addresses_and_overflow_is_explicit() {
        static TEST_STATE: Mutex<State> = Mutex::new(State::new());
        let mut state = TEST_STATE.lock().unwrap();
        state.used = 0;
        state.overflow = 0;
        state.last = [[None; MAX_SHARDS]; 9];
        state.record(4, 0, 1, 0, Some(0), 0);
        assert!(state.events[0].offsets.is_none());
        state.used = MAX_EVENTS;
        state.record(4, 0, 16, 0, Some(4096), 0);
        assert_eq!(state.overflow, 1);
        state.used = 0;
        state.overflow = 0;
    }
}
