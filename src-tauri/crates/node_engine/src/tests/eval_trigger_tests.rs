//! Trigger 节点求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use node_testkit::*;
#[test]
fn test_trigger_manual_number_rule_hit() {
    // manual 模式: 每帧以 command 匹配, number 规则命中 → value + matched (text 不写)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "GET_TEMP",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "GET_TEMP",
            "number",
            42.0,
            "",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    // 编译期槽位: value/matched 为 f32 槽位, text 为字符串槽位
    assert!(g.compiled().slot_of("tr1", "value").is_some());
    assert!(g.compiled().slot_of("tr1", "matched").is_some());
    assert!(g.compiled().str_slot_of("tr1", "text").is_some());

    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("value")), Some(&42.0));
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&1.0));
    // number 命中不写 text (对齐前端 runMatch 分派)
    assert!(!out_str.contains_key("tr1"));
}

#[test]
fn test_trigger_manual_string_rule_hit_routes_text() {
    // string 规则命中 → text 进 StringValuesMap + matched 写数值平面 (value 不覆盖)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "HELLO",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "HELLO",
            "string",
            0.0,
            "world",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("tr1").and_then(|m| m.get("text")),
        Some(&"world".to_string())
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&1.0));
    assert!(
        out.get("tr1").and_then(|m| m.get("value")).is_none(),
        "string 命中不写 value (对齐前端 runMatch)"
    );
}

#[test]
fn test_trigger_manual_miss_defaults() {
    // 未命中 → value = default_miss (-1) + matched = 0;
    // text 不写 (前端 miss 走 number 分支, 不提交 text — 保持上次值)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "NOPE",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "HELLO",
            "number",
            1.0,
            "",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("value")), Some(&-1.0));
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&0.0));
    assert!(!out_str.contains_key("tr1"));
}

#[test]
fn test_trigger_auto_level_matches_every_active_frame() {
    // auto + level: trigger 非零期间每帧匹配 (Range 规则用数值本身)
    let nodes = vec![
        make_input("knob1", "t1"),
        make_trigger(
            "tr1",
            "t1",
            "auto",
            "level",
            "",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Range,
                "1..10",
                "number",
                7.0,
                "",
            )],
        ),
    ];
    let edges = vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with = |v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
        input_values.clear();
        input_values.insert("knob1".to_string(), v);
        let mut out_str = StringValuesMap::default();
        let out = g.evaluate(
            &empty_frames(),
            &empty_texts(),
            &input_values,
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            ts,
            &mut out_str,
        );
        out.get("tr1")
            .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
    };

    assert_eq!(eval_with(0.0, &mut trigger_states), None); // 0 → 不激活
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0)));
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // level 持续触发
    assert_eq!(eval_with(50.0, &mut trigger_states), Some((-1.0, 0.0))); // 出界 → miss
}

#[test]
fn test_trigger_auto_rising_fires_once() {
    // auto + rising: 仅 0 → 正 上升沿匹配一次, 回落后再升重新触发
    let nodes = vec![
        make_input("knob1", "t1"),
        make_trigger(
            "tr1",
            "t1",
            "auto",
            "rising",
            "",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Range,
                "1..10",
                "number",
                7.0,
                "",
            )],
        ),
    ];
    let edges = vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with = |v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
        input_values.clear();
        input_values.insert("knob1".to_string(), v);
        let mut out_str = StringValuesMap::default();
        g.evaluate(
            &empty_frames(),
            &empty_texts(),
            &input_values,
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            ts,
            &mut out_str,
        )
        .get("tr1")
        .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
    };

    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // 上升沿
    assert_eq!(eval_with(5.0, &mut trigger_states), None); // 持续高位不再触发
    assert_eq!(eval_with(0.0, &mut trigger_states), None);
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // 再升再触发
}

#[test]
fn test_trigger_text_flows_through_str_chain() {
    // 任务 2 缺口用例: Trigger(string 规则, 真实文本) → Str(Mid) → Str(Upper)
    // 非空文本沿 string 边流动 (Trigger.text 已有字符串槽位)
    let nodes = vec![
        make_trigger(
            "tr1",
            "t1",
            "manual",
            "level",
            "GO",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Exact,
                "GO",
                "string",
                0.0,
                "hello world",
            )],
        ),
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 0.0,
                len: 5.0,
                size: 0.0,
            },
        ),
        make_str("up1", "t1", StrOp::Upper),
    ];
    let edges = vec![
        edge("e1", "tr1", "text", "mid1", "str"),
        edge("e2", "mid1", "result", "up1", "str"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("mid1").and_then(|m| m.get("result")),
        Some(&"hello".to_string())
    );
    let up = out_str.get("up1").and_then(|m| m.get("result")).cloned();
    assert_eq!(up, Some("HELLO".to_string()));
    assert!(!up.unwrap().is_empty(), "文本应非空沿 string 边流动");
}

#[test]
fn test_trigger_value_feeds_str_num_port_via_math() {
    // 任务 2 缺口用例: Str 数值端口已连接时走上游值 (Trigger.value → Math → Mid.pos)
    // pos=2 (上游) 时 Mid("hello", 2, 2) = "el" (1-based, 见 StrOp::Mid 测试);
    // 若误用内联回退 pos=9 则越界得 ""
    let nodes = vec![
        make_trigger(
            "tr_num",
            "t1",
            "manual",
            "level",
            "GO",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Exact,
                "GO",
                "number",
                2.0,
                "",
            )],
        ),
        make_trigger(
            "tr_text",
            "t1",
            "manual",
            "level",
            "IN",
            vec![trigger_rule(
                "r2",
                TriggerMatchType::Exact,
                "IN",
                "string",
                0.0,
                "hello",
            )],
        ),
        make_math("m1", "t1", MathOp::Abs, 1),
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 9.0,
                len: 2.0,
                size: 0.0,
            },
        ),
    ];
    let edges = vec![
        edge("e1", "tr_num", "value", "m1", "in0"),
        edge("e2", "m1", "result", "mid1", "pos"),
        edge("e3", "tr_text", "text", "mid1", "str"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("mid1").and_then(|m| m.get("result")),
        Some(&"el".to_string()),
        "pos 应用上游值 2.0 (而非内联回退 9.0)"
    );
}

#[test]
fn test_trigger_manual_tracks_prev_no_false_rising_on_mode_switch() {
    // 对齐前端 useEffect: 非 auto 模式仍每帧跟踪 prevTriggerRef。
    // manual 期间 trigger 输入 0→5; 切回 auto+rising (图重编译, 配置仅 mode 变化
    // → 不重建 TriggerState, prev 保留) 且输入保持 5 → 不应误触发上升沿。
    let rules = || {
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Range,
            "1..10",
            "number",
            7.0,
            "",
        )]
    };
    let edges = || vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g_manual = CompiledGraph::compile(
        "t1".into(),
        vec![
            make_input("knob1", "t1"),
            make_trigger("tr1", "t1", "manual", "rising", "GO", rules()),
        ],
        edges(),
    )
    .unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with =
        |g: &CompiledGraph, v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
            input_values.clear();
            input_values.insert("knob1".to_string(), v);
            let mut out_str = StringValuesMap::default();
            g.evaluate(
                &empty_frames(),
                &empty_texts(),
                &input_values,
                &HashMap::new(),
                &mut HashMap::new(),
                &HashMap::new(),
                &mut HashMap::new(),
                ts,
                &mut out_str,
            )
            .get("tr1")
            .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
        };

    // manual: 输入 0 → 5, prev 被跟踪 (command "GO" 不匹配 Range, miss)
    assert_eq!(
        eval_with(&g_manual, 0.0, &mut trigger_states),
        Some((-1.0, 0.0))
    );
    assert_eq!(
        eval_with(&g_manual, 5.0, &mut trigger_states),
        Some((-1.0, 0.0))
    );

    // 切回 auto+rising (仅 mode 变化, TriggerState 不重建), 输入保持 5
    let g_auto = CompiledGraph::compile(
        "t1".into(),
        vec![
            make_input("knob1", "t1"),
            make_trigger("tr1", "t1", "auto", "rising", "GO", rules()),
        ],
        edges(),
    )
    .unwrap();
    assert_eq!(
        eval_with(&g_auto, 5.0, &mut trigger_states),
        None,
        "prev 已在 manual 期间跟踪为 5, 不应误触发上升沿"
    );
    // 回落到 0 后再升: 正常触发
    assert_eq!(eval_with(&g_auto, 0.0, &mut trigger_states), None);
    assert_eq!(
        eval_with(&g_auto, 5.0, &mut trigger_states),
        Some((7.0, 1.0))
    );
}
