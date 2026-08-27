//! Input 节点求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use node_testkit::*;
#[test]
fn test_evaluate_input_node() {
    let nodes = vec![make_input("knob1", "t1")];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut input_values = HashMap::new();
    input_values.insert("knob1".to_string(), 42.0_f32);
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &input_values,
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("knob1").and_then(|m| m.get("value")), Some(&42.0));
}
