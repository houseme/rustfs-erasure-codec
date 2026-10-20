static DYNAMIC_PROBE_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn dynamic_allocator_callbacks_and_views_use_no_observer_heap() {
    let _serial = DYNAMIC_PROBE_SERIAL.lock().unwrap();
    let mut root = dynamic_buffers::Root::start(dynamic_buffers::test_page_size());
    root.quiet();
    let ((), counts) = count_heap(|| {
        dynamic_buffers::observation(0);
        let _epoch = dynamic_buffers::Epoch::begin();
        let _region = dynamic_buffers::Region::new("decode");
        let layout = std::alloc::Layout::from_size_align(128, 64).unwrap();
        // SAFETY: explicitly exercise the System-forwarding observer; these
        // direct System requests bypass the independent Rust heap counter.
        // Only callback/probe/formatting allocations would be counted.
        unsafe {
            let pointer = dynamic_buffers::Allocator.alloc_zeroed(layout);
            assert!(!pointer.is_null());
            dynamic_buffers::span(
                "matrix",
                "reserved",
                0,
                pointer,
                128,
                Some(128),
                "heap",
                true,
            );
            for index in 0..128 {
                pointer.add(index).write(index as u8);
            }
            let larger = dynamic_buffers::Allocator.realloc(pointer, layout, 256);
            assert!(!larger.is_null());
            for index in 0..128 {
                assert_eq!(larger.add(index).read(), index as u8);
            }
            dynamic_buffers::span(
                "matrix",
                "initialized",
                0,
                larger,
                128,
                Some(256),
                "heap",
                true,
            );
            dynamic_buffers::Allocator.dealloc(
                larger,
                std::alloc::Layout::from_size_align(256, 64).unwrap(),
            );
        }
    });
    assert_eq!(counts, Counts::default());
    drop(root);
    assert_eq!(dynamic_buffers::test_errors(), 0);
}

#[test]
fn dynamic_slot_presence_is_independent_of_buffer_ownership() {
    let _serial = DYNAMIC_PROBE_SERIAL.lock().unwrap();
    let slots = [rustfs_erasure_codec::ShardSlot::new_missing(vec![7u8; 32])];
    let mut root = dynamic_buffers::Root::start(dynamic_buffers::test_page_size());
    root.quiet();
    let ((), counts) = count_heap(|| {
        dynamic_buffers::observation(0);
        let _epoch = dynamic_buffers::Epoch::begin();
        dynamic_buffers::slots("dynamic", "missing", &slots);
    });
    assert_eq!(counts, Counts::default());
    drop(root);
    assert_eq!(dynamic_buffers::test_errors(), 0);
}

#[test]
fn concurrent_dynamic_callbacks_preserve_original_allocations() {
    let _serial = DYNAMIC_PROBE_SERIAL.lock().unwrap();
    let mut root = dynamic_buffers::Root::start(dynamic_buffers::test_page_size());
    root.quiet();
    dynamic_buffers::observation(0);
    let epoch = dynamic_buffers::Epoch::begin();
    let handles: Vec<_> = (0..4)
        .map(|thread| {
            std::thread::spawn(move || {
                let _region = dynamic_buffers::Region::new("decode");
                for _ in 0..32 {
                    let layout = std::alloc::Layout::from_size_align(128, 64).unwrap();
                    // SAFETY: exercise independent valid System allocations on each
                    // thread; initialize every byte read and free the updated layout.
                    unsafe {
                        let p = dynamic_buffers::Allocator.alloc_zeroed(layout);
                        assert!(!p.is_null());
                        for i in 0..128 {
                            p.add(i).write(thread);
                        }
                        let q = dynamic_buffers::Allocator.realloc(p, layout, 256);
                        assert!(!q.is_null());
                        for i in 0..128 {
                            assert_eq!(q.add(i).read(), thread);
                        }
                        dynamic_buffers::Allocator
                            .dealloc(q, std::alloc::Layout::from_size_align(256, 64).unwrap());
                    }
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    drop(epoch);
    drop(root);
    assert_eq!(dynamic_buffers::test_errors(), 0);
}
