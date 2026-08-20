use buffer_graph::RoutedData;

#[test]
fn routed_data_new_construction() {
    let r = RoutedData::new("tgt", "CH0", 42.5);
    assert_eq!(r.target_node, "tgt");
    assert_eq!(r.target_handle, "CH0");
    assert!((r.value - 42.5).abs() < f32::EPSILON);
}

#[test]
fn routed_data_clone() {
    let r = RoutedData::new("tgt", "CH0", 1.0);
    let c = r.clone();
    assert_eq!(c.target_node, r.target_node);
    assert_eq!(c.target_handle, r.target_handle);
    assert_eq!(c.value, r.value);
}