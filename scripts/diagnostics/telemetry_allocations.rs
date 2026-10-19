//! Test-only allocator probe. Never linked into the formal sampler.
#![allow(dead_code)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    alloc: usize,
    realloc: usize,
    dealloc: usize,
}
thread_local! { static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) }; }
struct CountingAllocator;
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record(alloc: usize, realloc: usize, dealloc: usize) {
    let _ = COUNTS.try_with(|state| {
        if let Some(mut counts) = state.get() {
            counts.alloc += alloc;
            counts.realloc += realloc;
            counts.dealloc += dealloc;
            state.set(Some(counts));
        }
    });
}
// SAFETY: every request is delegated unchanged to System. Thread-local scalar
// counters neither allocate nor modify ownership; unrelated threads are excluded.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(1, 0, 0);
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(1, 0, 0);
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(0, 1, 0);
        // SAFETY: pointer/layout identify the original System allocation.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(0, 0, 1);
        // SAFETY: pointer/layout identify the original System allocation.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn normal_telemetry_does_not_touch_the_rust_heap() {
    assert!(std::env::var_os("RSE_SAMPLER_DIAGNOSTIC_PROCESS").is_none());
    let (observation, counts) =
        count_heap(|| (0..16).map(|_| sampling::telemetry()).last().unwrap());
    assert_ne!(
        observation.to_string(),
        "null",
        "real Linux telemetry must be available"
    );
    assert_eq!(counts, Counts::default());
    let json: serde_json::Value = serde_json::from_str(&observation.to_string()).unwrap();
    for field in ["process_minor_faults", "process_major_faults"] {
        assert!(
            json[field].as_u64().is_some(),
            "real process fault counter missing"
        );
    }
}

fn count_heap<R>(work: impl FnOnce() -> R) -> (R, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|state| state.set(None));
        }
    }
    COUNTS.with(|state| state.set(Some(Counts::default())));
    let reset = Reset;
    let result = work();
    let counts = COUNTS.with(|state| state.replace(None).unwrap());
    drop(reset);
    (result, counts)
}

#[test]
fn hotspot_patterns_match_the_actual_original_formatter_without_heap_activity() {
    use std::fmt::Write;
    struct FixedWriter {
        bytes: [u8; 128],
        len: usize,
    }
    impl std::fmt::Write for FixedWriter {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            let end = self.len.checked_add(value.len()).ok_or(std::fmt::Error)?;
            let destination = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
            destination.copy_from_slice(value.as_bytes());
            self.len = end;
            Ok(())
        }
    }
    for (data, indices) in [
        (10, &[][..]),
        (10, &[0][..]),
        (10, &[0, 2][..]),
        (10, &[1, 10][..]),
        (10, &[2][..]),
        (10, &[0, 2, 4][..]),
        (10, &[0, 4][..]),
        (10, &[1][..]),
        (32, &[0, 2][..]),
        (32, &[2, 6][..]),
        (4, &[6, 0, 6, 3][..]),
        (32, &[0, 2, 4, 6][..]),
    ] {
        let (original, old_counts) = count_heap(|| original_pattern(data, indices));
        assert!(
            old_counts.alloc > 0,
            "the actual original formatter must allocate"
        );
        let (writer, counts) = count_heap(|| {
            let value = pattern::ShardPattern::new(data, indices);
            let mut writer = FixedWriter {
                bytes: [0; 128],
                len: 0,
            };
            write!(&mut writer, "{value}").unwrap();
            writer
        });
        assert_eq!(counts, Counts::default());
        assert_eq!(&writer.bytes[..writer.len], original.as_bytes());
        println!(
            "RSE_METADATA_ALLOCATION {{\"data_shards\":{data},\"indices\":{indices:?},\"original_alloc\":{},\"original_realloc\":{},\"original_dealloc\":{},\"new_alloc\":{},\"new_realloc\":{},\"new_dealloc\":{}}}",
            old_counts.alloc,
            old_counts.realloc,
            old_counts.dealloc,
            counts.alloc,
            counts.realloc,
            counts.dealloc
        );
    }
}

#[test]
fn workload_state_and_metadata_keep_the_rust_heap_untouched() {
    use std::fmt::Write;
    struct Writer {
        bytes: [u8; 2048],
        used: usize,
    }
    impl std::fmt::Write for Writer {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            let end = self.used + value.len();
            self.bytes
                .get_mut(self.used..end)
                .ok_or(std::fmt::Error)?
                .copy_from_slice(value.as_bytes());
            self.used = end;
            Ok(())
        }
    }
    let (_, counts) = count_heap(|| {
        let mut rng = workload::InputRng::new(workload::hotspot_seed("case", 10, 4, 1061));
        let before = rng.point();
        let mut data = [0u8; 1061];
        rng.fill(&mut data);
        let trace = workload::RngTrace::new(before, rng.point(), 1061);
        let value = workload::Workload::hotspot(42, ("Vec<Option<Vec<u8>>>", "test", 1, 1), trace);
        let mut writer = Writer {
            bytes: [0; 2048],
            used: 0,
        };
        write!(&mut writer, "{value}").unwrap();
        std::hint::black_box(writer);
    });
    assert_eq!(counts, Counts::default());
}

#[test]
#[cfg(target_os = "linux")]
fn fixed_affinity_checks_do_not_touch_the_rust_heap() {
    let (_, counts) = count_heap(|| {
        for _ in 0..16 {
            std::hint::black_box(sampling::affinity_cpu());
        }
    });
    assert_eq!(counts, Counts::default());
}
