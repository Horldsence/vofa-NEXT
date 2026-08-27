//! Custom 节点求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use node_testkit::*;
#[test]
fn test_evaluate_custom_node() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_custom("c1", "t1", vec!["value"], vec!["out"]),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "c1", "value")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![5.0])]);
    let mut custom_outputs: HashMap<String, HashMap<String, f32>> = HashMap::new();
    let mut m = HashMap::new();
    m.insert("out".to_string(), 99.0);
    custom_outputs.insert("c1".to_string(), m);

    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &custom_outputs,
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("c1").and_then(|m| m.get("out")), Some(&99.0));

    // collect_custom_inputs 应返回 c1.value = 5.0
    let custom_inputs = g.collect_custom_inputs(&out);
    assert_eq!(
        custom_inputs.get("c1").and_then(|m| m.get("value")),
        Some(&5.0)
    );
}
