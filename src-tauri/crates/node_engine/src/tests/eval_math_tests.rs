//! Math 节点求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use node_testkit::*;
#[test]
fn test_evaluate_math_add() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 2),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![10.0, 20.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    // m1.result = 10 + 20 = 30
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&30.0));
}

#[test]
fn test_evaluate_math_chain() {
    // m1 = ch0 + ch1, m2 = m1 * m1
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 2),
        make_math("m1", "t1", MathOp::Add, 2),
        make_math("m2", "t1", MathOp::Mul, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
        edge("e3", "m1", "result", "m2", "in0"),
        edge("e4", "m1", "result", "m2", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![3.0, 4.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    // m1 = 3 + 4 = 7, m2 = 7 * 7 = 49
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&7.0));
    assert_eq!(out.get("m2").and_then(|m| m.get("result")), Some(&49.0));
}

#[test]
fn test_unary_math() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_math("m1", "t1", MathOp::Abs, 1),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "m1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![-5.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&5.0));
}
