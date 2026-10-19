#[test]
fn verify_scratch_callbacks_do_not_allocate_or_release_heap_storage() {
    let _serial = verify_scratch::TEST_LOCK.lock().unwrap();
    let bytes = vec![0u8; 1061];
    verify_scratch::start(verify_scratch::native_page_size().unwrap_or(4096));
    verify_scratch::observation(0);
    let (_, counts) = count_heap(|| {
        let mut guard = verify_scratch::Guard::new(bytes.len());
        guard.reserved(bytes.as_ptr(), bytes.capacity(), false);
        guard.initialized(bytes.as_ptr(), bytes.len(), bytes.capacity());
        guard.verified(true);
        drop(guard);
    });
    assert_eq!(counts, Counts::default());
    assert!(verify_scratch::stop());
}
