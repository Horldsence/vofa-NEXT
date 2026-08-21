use buffer_graph::{Edge, NodeGraph};
use vofa_core::DataFrame;

fn make_edge(id: &str, src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
    Edge {
        id: id.into(),
        source: src.into(),
        source_handle: src_h.into(),
        target: tgt.into(),
        target_handle: tgt_h.into(),
    }
}

#[test]
fn empty_graph() {
    let graph = NodeGraph::new();
    let frame = DataFrame::new(vec![1.0, 2.0]);
    let routes = graph.route_frame(&frame);
    assert!(routes.is_empty());
}

#[test]
fn single_connection() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "channel_source", "ch0", "waveform1", "CH0"));

    let frame = DataFrame::new(vec![42.0, 99.0]);
    let routes = graph.route_frame(&frame);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].target_node, "waveform1");
    assert_eq!(routes[0].target_handle, "CH0");
    assert!((routes[0].value - 42.0).abs() < f32::EPSILON);
}

#[test]
fn multi_connection_same_source() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "ch_src", "ch0", "waveform1", "CH0"));
    graph.add_edge(make_edge("e2", "ch_src", "ch0", "pie1", "seg0"));

    let frame = DataFrame::new(vec![55.0]);
    let routes = graph.route_frame(&frame);
    assert_eq!(routes.len(), 2);
    assert!(routes.iter().any(|r| r.target_node == "waveform1"));
    assert!(routes.iter().any(|r| r.target_node == "pie1"));
}

#[test]
fn multi_channel_routing() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "ch_src", "ch0", "waveform1", "CH0"));
    graph.add_edge(make_edge("e2", "ch_src", "ch1", "waveform1", "CH1"));

    let frame = DataFrame::new(vec![10.0, 20.0]);
    let routes = graph.route_frame(&frame);
    assert_eq!(routes.len(), 2);
    let ch0 = routes.iter().find(|r| r.target_handle == "CH0").unwrap();
    let ch1 = routes.iter().find(|r| r.target_handle == "CH1").unwrap();
    assert!((ch0.value - 10.0).abs() < f32::EPSILON);
    assert!((ch1.value - 20.0).abs() < f32::EPSILON);
}

#[test]
fn route_value() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "knob1", "value", "label1", "value"));

    let routes = graph.route_value("knob1", 123.0);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].target_node, "label1");
    assert!((routes[0].value - 123.0).abs() < f32::EPSILON);
}

#[test]
fn no_route_for_unknown_source() {
    let graph = NodeGraph::new();
    let routes = graph.route_value("nonexistent", 1.0);
    assert!(routes.is_empty());
}

#[test]
fn remove_edge() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src", "ch0", "tgt", "CH0"));
    graph.add_edge(make_edge("e2", "src", "ch1", "tgt", "CH1"));

    graph.remove_edge("e1");
    assert_eq!(graph.edges().len(), 1);
    assert_eq!(graph.edges()[0].id, "e2");
}

#[test]
fn update_edges() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src", "ch0", "tgt", "CH0"));
    graph.update_edges(vec![make_edge("e2", "new_src", "ch0", "new_tgt", "CH0")]);

    assert_eq!(graph.edges().len(), 1);
    assert_eq!(graph.edges()[0].id, "e2");
}

#[test]
fn cycle_detection_no_cycle() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "a", "ch0", "b", "CH0"));
    graph.add_edge(make_edge("e2", "b", "ch0", "c", "CH0"));
    assert!(!graph.has_cycle());
}

#[test]
fn cycle_detection_with_cycle() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "a", "ch0", "b", "CH0"));
    graph.add_edge(make_edge("e2", "b", "ch0", "a", "CH0"));
    assert!(graph.has_cycle());
}

#[test]
fn edges_to() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src1", "ch0", "tgt1", "CH0"));
    graph.add_edge(make_edge("e2", "src2", "ch0", "tgt1", "CH1"));
    graph.add_edge(make_edge("e3", "src3", "ch0", "tgt2", "CH0"));

    let edges = graph.edges_to("tgt1");
    assert_eq!(edges.len(), 2);
}

#[test]
fn edges_from() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src1", "ch0", "tgt1", "CH0"));
    graph.add_edge(make_edge("e2", "src1", "ch1", "tgt2", "CH0"));
    graph.add_edge(make_edge("e3", "src2", "ch0", "tgt3", "CH0"));

    let edges = graph.edges_from("src1");
    assert_eq!(edges.len(), 2);
}

#[test]
fn route_frame_handles_exceeding_channels_ignored() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src", "ch0", "tgt", "CH0"));
    let frame = DataFrame::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let routes = graph.route_frame(&frame);
    assert_eq!(routes.len(), 1);
}

#[test]
fn route_value_returns_empty_for_no_edges() {
    let graph = NodeGraph::new();
    let routes = graph.route_value("any", 0.0);
    assert!(routes.is_empty());
}

#[test]
fn remove_nonexistent_edge_is_noop() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "src", "ch0", "tgt", "CH0"));
    graph.remove_edge("nonexistent");
    assert_eq!(graph.edges().len(), 1);
}

#[test]
fn default_impl_creates_empty_graph() {
    let graph = NodeGraph::default();
    assert!(graph.edges().is_empty());
}

#[test]
fn cycle_detection_self_loop() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "a", "ch0", "a", "CH0"));
    assert!(graph.has_cycle());
}

#[test]
fn cycle_detection_three_node_cycle() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "a", "ch0", "b", "CH0"));
    graph.add_edge(make_edge("e2", "b", "ch0", "c", "CH0"));
    graph.add_edge(make_edge("e3", "c", "ch0", "a", "CH0"));
    assert!(graph.has_cycle());
}

#[test]
fn route_value_multi_targets() {
    let mut graph = NodeGraph::new();
    graph.add_edge(make_edge("e1", "knob", "value", "label1", "value"));
    graph.add_edge(make_edge("e2", "knob", "value", "label2", "value"));
    let routes = graph.route_value("knob", 7.0);
    assert_eq!(routes.len(), 2);
    assert!(routes.iter().any(|r| r.target_node == "label1"));
    assert!(routes.iter().any(|r| r.target_node == "label2"));
}