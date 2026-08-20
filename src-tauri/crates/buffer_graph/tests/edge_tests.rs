use buffer_graph::Edge;

#[test]
fn edge_serde_round_trip() {
    let e = Edge {
        id: "e1".into(),
        source: "src".into(),
        source_handle: "ch0".into(),
        target: "tgt".into(),
        target_handle: "CH0".into(),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Edge = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, "e1");
    assert_eq!(back.source, "src");
    assert_eq!(back.target_handle, "CH0");
}

#[test]
fn edge_clone_preserves_all_fields() {
    let e = Edge {
        id: "e1".into(),
        source: "s".into(),
        source_handle: "sh".into(),
        target: "t".into(),
        target_handle: "th".into(),
    };
    let c = e.clone();
    assert_eq!(c.id, e.id);
    assert_eq!(c.source, e.source);
    assert_eq!(c.source_handle, e.source_handle);
    assert_eq!(c.target, e.target);
    assert_eq!(c.target_handle, e.target_handle);
}