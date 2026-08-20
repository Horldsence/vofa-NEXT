//! `core::frame` 单元测试
//!
//! 覆盖:
//! - `DataFrame::new` 与 `with_timestamp` 时间戳行为
//! - `DataFrame` 序列化/反序列化 round-trip
//! - `RawData` 序列化
//! - `ConnectionState` 部分相等 + serde
//! - `PortInfo` 字段透传
//! - `TransportStats` 默认值与累加
//! - `now_us` 单调性 / 非负性

use vofa_core::frame::{
    now_us, ConnectionState, DataFrame, PortInfo, RawData, TransportStats,
};

#[test]
fn dataframe_new_uses_current_timestamp() {
    let before = now_us();
    let f = DataFrame::new(vec![1.0, 2.0, 3.0]);
    let after = now_us();
    assert!(f.timestamp >= before);
    assert!(f.timestamp <= after);
    assert_eq!(f.channels, vec![1.0, 2.0, 3.0]);
}

#[test]
fn dataframe_with_timestamp_preserves_value() {
    let f = DataFrame::with_timestamp(123_456_789, vec![0.5, -1.5]);
    assert_eq!(f.timestamp, 123_456_789);
    assert_eq!(f.channels, vec![0.5, -1.5]);
}

#[test]
fn dataframe_len_and_is_empty() {
    assert_eq!(DataFrame::new(vec![1.0]).len(), 1);
    assert!(!DataFrame::new(vec![1.0]).is_empty());
    assert_eq!(DataFrame::new(vec![]).len(), 0);
    assert!(DataFrame::new(vec![]).is_empty());
}

#[test]
fn dataframe_serde_roundtrip() {
    let f = DataFrame::with_timestamp(987_654, vec![0.1, 0.2, 0.3]);
    let json = serde_json::to_string(&f).unwrap();
    let restored: DataFrame = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.timestamp, f.timestamp);
    assert_eq!(restored.channels, f.channels);
}

#[test]
fn rawdata_construction_and_serde() {
    let r = RawData::new(42, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(r.timestamp, 42);
    assert_eq!(r.data, vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let json = serde_json::to_string(&r).unwrap();
    let restored: RawData = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.data, r.data);
}

#[test]
fn connection_state_eq_and_serde() {
    for s in [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Error,
    ] {
        // 部分相等 + 复制 + 序列化(变体名小写)
        let s2 = s;
        assert_eq!(s, s2);
        let json = serde_json::to_string(&s).unwrap();
        let restored: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, s);
    }
}

#[test]
fn portinfo_field_propagation() {
    let p = PortInfo {
        name: "COM4".into(),
        port_type: "USB".into(),
        vid: Some(0x1A86),
        pid: Some(0x7523),
        serial_number: Some("ABC123".into()),
        manufacturer: Some("wch.cn".into()),
        product: Some("CH340".into()),
        description: Some("USB-SERIAL CH340".into()),
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"name\":\"COM4\""));
    assert!(json.contains("\"vid\":6790")); // 0x1A86 = 6790
}

#[test]
fn transportstats_default_is_all_zero() {
    let s = TransportStats::default();
    assert_eq!(s.rx_bytes, 0);
    assert_eq!(s.tx_bytes, 0);
    assert_eq!(s.rx_frames, 0);
    assert_eq!(s.tx_frames, 0);
    assert_eq!(s.rx_dropped, 0);
}

#[test]
fn transportstats_rx_dropped_default_tolerated_when_missing() {
    // 老 JSON 没有 rx_dropped 字段,反序列化应回退 0
    let json = r#"{"rx_bytes":1,"tx_bytes":2,"rx_frames":3,"tx_frames":4}"#;
    let s: TransportStats = serde_json::from_str(json).unwrap();
    assert_eq!(s.rx_dropped, 0);
    assert_eq!(s.rx_bytes, 1);
}

#[test]
fn now_us_is_non_decreasing_in_repeated_calls() {
    let a = now_us();
    let b = now_us();
    assert!(b >= a, "now_us must be non-decreasing: a={a} b={b}");
}

#[test]
fn now_us_returns_u64() {
    // 仅编译期/类型检查
    let t: u64 = now_us();
    let _ = t;
}
