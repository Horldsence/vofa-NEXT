use cmd_display::{DisplayEvent, DisplayRequest, RawDataOrigin};
use pipeline_data_plane::GraphOutputSnapshot;

#[test]
fn request_is_a_tagged_exhaustive_union() {
    let request: DisplayRequest = serde_json::from_value(serde_json::json!({
        "kind": "raw_data",
        "origin": { "kind": "decoder", "id": "decoder-1" },
        "direction": "rx",
        "search": "AA 55"
    }))
    .expect("raw-data request should deserialize");

    assert!(matches!(
        request,
        DisplayRequest::RawData {
            origin: RawDataOrigin::Decoder(id),
            direction,
            search,
        } if id == "decoder-1" && direction == "rx" && search == "AA 55"
    ));
}

#[test]
fn port_sample_request_uses_canonical_node_and_handle_key() {
    let request: DisplayRequest = serde_json::from_value(serde_json::json!({
        "kind": "port_samples",
        "source_node_id": "FireWater",
        "source_handle": "ch3"
    }))
    .expect("port sample request should deserialize");
    assert!(matches!(
        request,
        DisplayRequest::PortSamples {
            source_node_id,
            source_handle,
        } if source_node_id == "FireWater" && source_handle == "ch3"
    ));
}

#[test]
fn event_serialization_matches_typescript_discriminant() {
    let mut snapshot = GraphOutputSnapshot {
        tick: 7,
        graphs_version: 3,
        values: node_engine::ValuesMap::default(),
    };
    snapshot
        .values
        .entry("gauge-1".to_owned())
        .or_default()
        .insert("value".to_owned(), 42.0);
    let event = DisplayEvent::GraphOutputs(snapshot);

    let value = serde_json::to_value(event).expect("display event should serialize");
    assert_eq!(value["kind"], "graph_outputs");
    assert_eq!(value["payload"]["tick"], 7);
    assert_eq!(value["payload"]["values"]["gauge-1"]["value"], 42.0);
}

#[test]
fn optional_filters_default_to_backend_match_all() {
    let can: DisplayRequest = serde_json::from_value(serde_json::json!({
        "kind": "can_frames"
    }))
    .expect("CAN request should accept an omitted filter");
    let logic: DisplayRequest = serde_json::from_value(serde_json::json!({
        "kind": "logic_samples"
    }))
    .expect("logic request should accept an omitted filter");

    assert!(matches!(can, DisplayRequest::CanFrames { filter: None }));
    assert!(matches!(
        logic,
        DisplayRequest::LogicSamples { filter: None }
    ));
}
