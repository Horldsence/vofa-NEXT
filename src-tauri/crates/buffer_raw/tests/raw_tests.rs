use buffer_raw::{DirectionFilter, RawDataCollector, RawDataDirection, RawDrain, SearchPattern};

#[test]
fn push_and_read() {
    let mut col = RawDataCollector::with_capacity(1024);
    col.push_chunk(100, RawDataDirection::Rx, b"hello");
    col.push_chunk(200, RawDataDirection::Rx, b"world");
    assert_eq!(col.total_bytes(), 10);
    assert_eq!(col.chunk_count(), 2);

    let (chunks, next) = col.read_from(0, 10);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].2, b"hello");
    assert_eq!(chunks[1].2, b"world");
    assert_eq!(next, 2);

    let (chunks2, next2) = col.read_from(0, 10);
    assert_eq!(chunks2.len(), 2);
    assert_eq!(next2, 2);
}

#[test]
fn drops_oldest() {
    let mut col = RawDataCollector::with_capacity(10);
    col.push_chunk(1, RawDataDirection::Rx, b"0123456789");
    assert_eq!(col.dropped_bytes(), 0);
    col.push_chunk(2, RawDataDirection::Rx, b"xx");
    assert_eq!(col.dropped_bytes(), 10);
    assert_eq!(col.base_index(), 1);

    let (chunks, next) = col.read_from(0, 1024);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].2, b"xx");
    assert_eq!(next, 2);
}

#[test]
fn read_max_bytes() {
    let mut col = RawDataCollector::with_capacity(1024);
    col.push_chunk(1, RawDataDirection::Rx, b"12345");
    col.push_chunk(2, RawDataDirection::Rx, b"67890");
    col.push_chunk(3, RawDataDirection::Rx, b"abcde");

    let (chunks, next) = col.read_from(0, 12);
    assert_eq!(chunks.len(), 2);
    assert_eq!(next, 2);
}

#[test]
fn clear() {
    let mut col = RawDataCollector::with_capacity(1024);
    col.push_chunk(1, RawDataDirection::Rx, b"data");
    col.clear();
    assert_eq!(col.total_bytes(), 0);
    assert_eq!(col.dropped_bytes(), 0);
    assert_eq!(col.base_index(), 0);
    let (chunks, _) = col.read_from(0, 1024);
    assert!(chunks.is_empty());
}

#[test]
fn read_filtered() {
    let mut col = RawDataCollector::with_capacity(1024);
    col.push_chunk(1, RawDataDirection::Rx, b"hello");
    col.push_chunk(2, RawDataDirection::Tx, b"world");
    col.push_chunk(3, RawDataDirection::Rx, b"again");

    let (chunks, next) = col.read_filtered_from(0, 1024, DirectionFilter::Rx, None);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].2, b"hello");
    assert_eq!(chunks[1].2, b"again");
    assert_eq!(next, 3);

    let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::Tx, None);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].2, b"world");

    let pat = SearchPattern::parse("ell").unwrap();
    let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::All, Some(&pat));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].2, b"hello");

    let pat = SearchPattern::parse("77 6f").unwrap();
    let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::All, Some(&pat));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].2, b"world");
}

#[test]
fn zero_capacity_clamps_to_one() {
    let mut col = RawDataCollector::with_capacity(0);
    col.push_chunk(0, RawDataDirection::Rx, b"12345");
    assert!(col.dropped_bytes() >= 4);
}

#[test]
fn set_capacity_shrinks() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"0123456789");
    col.push_chunk(1, RawDataDirection::Rx, b"abcdefghij");
    col.set_capacity(5);
    assert!(col.dropped_bytes() > 0);
    assert!(col.base_index() > 0);
}

#[test]
fn set_capacity_larger_no_op() {
    let mut col = RawDataCollector::with_capacity(10);
    col.push_chunk(0, RawDataDirection::Rx, b"abc");
    let dropped_before = col.dropped_bytes();
    col.set_capacity(1000);
    assert_eq!(col.dropped_bytes(), dropped_before);
}

#[test]
fn read_from_stale_index_aligns_to_base() {
    let mut col = RawDataCollector::with_capacity(10);
    col.push_chunk(0, RawDataDirection::Rx, b"0123456789");
    col.push_chunk(1, RawDataDirection::Rx, b"abcdefghij");
    let (chunks, _) = col.read_from(0, 1024);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].2, b"abcdefghij");
}

#[test]
fn read_from_start_at_existing_index() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"hello");
    col.push_chunk(1, RawDataDirection::Rx, b"world");
    let (chunks, next) = col.read_from(0, 1024);
    assert_eq!(chunks.len(), 2);
    assert_eq!(next, 2);
}

#[test]
fn read_from_max_bytes_zero_includes_one_chunk() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"hello");
    let (chunks, _) = col.read_from(0, 0);
    assert_eq!(chunks.len(), 1);
}

#[test]
fn remaining_bytes_from_full() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"hello");
    col.push_chunk(1, RawDataDirection::Rx, b"world");
    assert_eq!(col.remaining_bytes_from(0), 10);
    assert_eq!(col.stored_bytes(), 10);
}

#[test]
fn remaining_bytes_from_partial() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"hello");
    col.push_chunk(1, RawDataDirection::Rx, b"world");
    assert_eq!(col.remaining_bytes_from(1), 5);
}

#[test]
fn remaining_bytes_from_stale_clamps() {
    let mut col = RawDataCollector::with_capacity(5);
    col.push_chunk(0, RawDataDirection::Rx, b"01234");
    col.push_chunk(1, RawDataDirection::Rx, b"56789");
    assert!(col.base_index() > 0);
    assert_eq!(col.remaining_bytes_from(0), 5);
}

#[test]
fn raw_drain_into_batch_base64_encodes() {
    let drain = RawDrain {
        chunks: vec![(123, RawDataDirection::Rx, b"hello".to_vec())],
        total_bytes: 5,
        dropped_bytes: 0,
    };
    let batch = drain.into_batch();
    assert_eq!(batch.chunks.len(), 1);
    assert_eq!(batch.chunks[0].bytes_b64, "aGVsbG8=");
    assert_eq!(batch.chunks[0].timestamp_us, 123);
    assert_eq!(batch.total_bytes, 5);
    assert_eq!(batch.dropped_bytes, 0);
}

#[test]
fn raw_drain_into_batch_seq_zero_assigned_by_dispatcher() {
    let drain = RawDrain {
        chunks: vec![],
        total_bytes: 0,
        dropped_bytes: 0,
    };
    let batch = drain.into_batch();
    assert_eq!(batch.seq, 0);
}

#[test]
fn default_uses_default_capacity() {
    let col = RawDataCollector::default();
    assert_eq!(col.chunk_count(), 0);
    assert!(col.stored_bytes() < RawDataCollector::DEFAULT_CAPACITY);
}

#[test]
fn raw_data_direction_serde_round_trip() {
    for (d, expected) in [
        (RawDataDirection::Rx, "\"rx\""),
        (RawDataDirection::Tx, "\"tx\""),
    ] {
        assert_eq!(serde_json::to_string(&d).unwrap(), expected);
        let back: RawDataDirection = serde_json::from_str(expected).unwrap();
        assert_eq!(back, d);
    }
}

#[test]
fn raw_data_direction_default_is_rx() {
    assert_eq!(RawDataDirection::default(), RawDataDirection::Rx);
}

#[test]
fn clone_preserves_state() {
    let mut col = RawDataCollector::with_capacity(100);
    col.push_chunk(0, RawDataDirection::Rx, b"hello");
    col.push_chunk(1, RawDataDirection::Rx, b"world");
    let cloned = col.clone();
    assert_eq!(cloned.chunk_count(), 2);
    assert_eq!(cloned.total_bytes(), 10);
    assert_eq!(cloned.base_index(), 0);
}