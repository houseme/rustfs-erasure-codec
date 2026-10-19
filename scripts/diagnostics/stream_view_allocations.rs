// Stream-specific attachment for the standalone allocation probe.
// The shared telemetry counter remains independent for other diagnostics.
#[test]
fn stream_view_callbacks_and_reserved_vec_io_do_not_allocate() {
    use std::io::{Read, Write};
    let _session = stream_views::test_session();
    stream_views::enable();
    let source = vec![vec![0x57u8; 4096]];
    let mut readers = vec![stream_views::Reader(source[0].as_slice())];
    let mut writers = stream_views::outputs(1, 4096);
    stream_views::begin(&source, 1, 4096);
    stream_views::call();
    stream_views::readers(&readers);
    stream_views::writers(&writers);
    let (_, counts) = count_heap(|| {
        let mut buffer = [0u8; 256];
        for _ in 0..16 {
            readers[0].read_exact(&mut buffer).unwrap();
            writers[0].write_all(&buffer).unwrap();
        }
    });
    assert_eq!(counts, Counts::default());
    assert_eq!(writers[0].0, source[0]);
    drop(writers);
    drop(readers);
    stream_views::end_call();
    stream_views::finish();
    stream_views::disable();
}
