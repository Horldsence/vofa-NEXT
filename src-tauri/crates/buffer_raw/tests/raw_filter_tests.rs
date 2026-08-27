use buffer_raw::{chunk_matches, DirectionFilter, RawDataDirection, SearchPattern, StoredChunk};

fn make_chunk(direction: RawDataDirection, bytes: &[u8]) -> StoredChunk {
    StoredChunk {
        timestamp_us: 0,
        direction,
        bytes: bytes.to_vec(),
    }
}

#[test]
fn direction_filter_default_is_all() {
    let f = DirectionFilter::default();
    assert_eq!(f, DirectionFilter::All);
    assert!(f.matches(RawDataDirection::Rx));
    assert!(f.matches(RawDataDirection::Tx));
}

#[test]
fn direction_filter_rx_only() {
    let f = DirectionFilter::Rx;
    assert!(f.matches(RawDataDirection::Rx));
    assert!(!f.matches(RawDataDirection::Tx));
}

#[test]
fn direction_filter_tx_only() {
    let f = DirectionFilter::Tx;
    assert!(!f.matches(RawDataDirection::Rx));
    assert!(f.matches(RawDataDirection::Tx));
}

#[test]
fn search_pattern_empty_input_returns_none() {
    assert!(SearchPattern::parse("").is_none());
    assert!(SearchPattern::parse("   ").is_none());
}

#[test]
fn search_pattern_text() {
    let p = SearchPattern::parse("hello").unwrap();
    assert_eq!(p.as_bytes(), b"hello");
    assert!(p.matches(b"say hello world"));
    assert!(!p.matches(b"goodbye"));
}

#[test]
fn search_pattern_hex_with_spaces() {
    let p = SearchPattern::parse("31 32 33").unwrap();
    assert_eq!(p.as_bytes(), &[0x31, 0x32, 0x33]);
    assert!(p.matches(b"abc123def"));
}

#[test]
fn search_pattern_hex_no_spaces() {
    let p = SearchPattern::parse("313233").unwrap();
    assert_eq!(p.as_bytes(), &[0x31, 0x32, 0x33]);
    assert!(p.matches(b"x123"));
}

#[test]
fn search_pattern_single_byte_hex() {
    let p = SearchPattern::parse("41").unwrap();
    assert_eq!(p.as_bytes(), &[0x41]);
    assert!(p.matches(b"XYZABC"));
}

#[test]
fn search_pattern_odd_hex_length_parses_last_digit_alone() {
    let p = SearchPattern::parse("313").unwrap();
    assert_eq!(p.as_bytes(), &[0x31, 0x03]);
}

#[test]
fn search_pattern_matches_single_byte_optimization() {
    // 1 字节 hex 模式走 contains 分支, 多字节 hex 模式走 windows 分支
    // (parse 默认 hex 分支, 单字符如 "41" 解析为 [0x41])
    let single = SearchPattern::parse("41").unwrap(); // 'A' = 0x41
    assert_eq!(single.as_bytes(), &[0x41]);
    assert!(single.len() == 1);
    assert!(single.matches(b"BANANA"));
    let multi = SearchPattern::parse("414e41").unwrap(); // 'ANA' hex
    assert_eq!(multi.as_bytes(), &[0x41, 0x4E, 0x41]);
    assert!(multi.matches(b"BANANA"));
    assert!(!multi.matches(b"BBBB"));
}

#[test]
fn search_pattern_empty_matches_anything() {
    let p = SearchPattern::from_bytes(Vec::new());
    assert!(p.is_empty());
    assert!(p.matches(b""));
    assert!(p.matches(b"hello"));
}

#[test]
fn search_pattern_invalid_hex_digit_falls_through_to_utf8() {
    let p = SearchPattern::parse("31 32 XX").unwrap();
    assert_eq!(p.as_bytes(), b"31 32 XX");
    assert!(p.matches(b"...31 32 XX..."));
}

#[test]
fn chunk_matches_direction_filter_rejects() {
    let chunk = make_chunk(RawDataDirection::Tx, b"hello");
    let (ok, tail) = chunk_matches(
        &chunk,
        DirectionFilter::Rx,
        None,
        b"prev_tail_should_be_ignored",
    );
    assert!(!ok);
    assert!(tail.is_empty());
}

#[test]
fn chunk_matches_no_pattern_always_ok() {
    let chunk = make_chunk(RawDataDirection::Rx, b"hello");
    let (ok, _) = chunk_matches(&chunk, DirectionFilter::All, None, b"");
    assert!(ok);
}

#[test]
fn chunk_matches_pattern_finds_substring() {
    let chunk = make_chunk(RawDataDirection::Rx, b"say hello world");
    let pat = SearchPattern::parse("hello").unwrap();
    let (ok, _) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"");
    assert!(ok);
}

#[test]
fn chunk_matches_pattern_misses() {
    let chunk = make_chunk(RawDataDirection::Rx, b"goodbye");
    let pat = SearchPattern::parse("hello").unwrap();
    let (ok, _) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"");
    assert!(!ok);
}

#[test]
fn chunk_matches_cross_boundary_with_prev_tail() {
    // 搜索模式 "hello" (5 字节, 含非 hex 'h' → 走 text 分支),
    // prev_tail = "hell" (4 字节 = pattern.len() - 1),
    // 当前 chunk = "o world" → combined = "hell" + "o world" = "hello world"
    let chunk = make_chunk(RawDataDirection::Rx, b"o world");
    let pat = SearchPattern::parse("hello").unwrap();
    let (ok, _) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"hell");
    assert!(ok);
}

#[test]
fn chunk_matches_empty_pattern_with_data() {
    let chunk = make_chunk(RawDataDirection::Rx, b"anything");
    let pat = SearchPattern::from_bytes(Vec::new());
    let (ok, _) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"");
    assert!(ok);
}

#[test]
fn chunk_matches_new_tail_is_last_n_minus_one_bytes() {
    let chunk = make_chunk(RawDataDirection::Rx, b"0123456789");
    let pat = SearchPattern::parse("xyz").unwrap();
    let (_, tail) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"");
    assert_eq!(tail, b"89");
}

#[test]
fn chunk_matches_new_tail_pattern_shorter_than_data() {
    let chunk = make_chunk(RawDataDirection::Rx, b"abcdef");
    let pat = SearchPattern::parse("xy").unwrap();
    let (_, tail) = chunk_matches(&chunk, DirectionFilter::All, Some(&pat), b"");
    assert_eq!(tail.len(), 1);
}
