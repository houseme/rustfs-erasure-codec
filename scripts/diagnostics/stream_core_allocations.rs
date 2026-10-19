#[test]
fn stream_core_snapshots_and_owner_transitions_do_not_use_the_heap() {
    let _serial = stream_core::TEST_LOCK.lock().unwrap();
    let bytes = vec![0u8; 4097];
    stream_core::start(stream_core::native_page_size().unwrap_or(4096));
    stream_core::observation(0);
    let (_, counts) = count_heap(|| {
        let mut outer = stream_core::Guard::new("verify_stream", "retained-scratch");
        outer.span(
            "reserved",
            0,
            0,
            bytes.as_ptr(),
            bytes.len(),
            bytes.capacity(),
            0,
            true,
        );
        outer.next_block();
        {
            let mut nested = stream_core::Guard::new("verify_par", "smallvec-per-block");
            nested.span(
                "initialized",
                0,
                0,
                bytes.as_ptr(),
                bytes.len(),
                bytes.capacity(),
                bytes.len(),
                true,
            );
            nested.complete(Some(false));
        }
        // Both queries use numbers only. No reference to freed/uninitialized
        // storage is constructed, and former mappings need not be unmapped.
        outer.replaced(0, 0);
        outer.complete(Some(false));
        drop(outer);
    });
    assert_eq!(counts, Counts::default());
    assert!(stream_core::stop());
}
