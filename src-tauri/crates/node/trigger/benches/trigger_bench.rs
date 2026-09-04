//! 触发器匹配基准 — `TriggerMatcher::match_input` 规则类型 × 规则数矩阵
//!
//! 场景: Exact / Prefix / Contains / Regex / Range / Glob 六种匹配类型 ×
//! {1, 8} 条规则 × 命中/未命中输入; 8 规则时命中位于末尾 (全量扫描最坏形态)。
//!
//! 已知形态: match_input 每次调用 clone 全部规则表 (regex/glob 缓存写入需
//! 可变借用) — 该开销随规则数线性扩展, 由 `{type}_1_*` vs `{type}_8_*` 对照;
//! Regex/Glob 另含按 rule.id 的编译缓存 (首次后零编译成本)。

#![allow(clippy::cast_precision_loss)] // 基准小整数 (规则下标 ≤ 7) 转 f32, 精度损失无意义

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use trigger::{TriggerMatchType, TriggerMatcher, TriggerRuleDef};

fn rule(id: &str, mt: TriggerMatchType, pattern: &str) -> TriggerRuleDef {
    TriggerRuleDef {
        id: id.to_string(),
        pattern: pattern.to_string(),
        match_type: mt,
        flags: None,
        output_type: "number".to_string(),
        output_value: 1.0,
        output_text: String::new(),
        enabled: true,
    }
}

/// 第 i 条规则的模式 (i 从 0 起)
fn pattern_for(mt: TriggerMatchType, i: usize) -> String {
    match mt {
        TriggerMatchType::Exact => format!("cmd{i}"),
        TriggerMatchType::Prefix => format!("cmd{i}_pre"),
        TriggerMatchType::Contains => format!("xx cmd{i}_mid yy"),
        TriggerMatchType::Regex => format!("cmd{i}_[a-z]+"),
        TriggerMatchType::Range => format!("{}..{}", i * 10, i * 10 + 9),
        TriggerMatchType::Glob => format!("cmd{i}_*"),
    }
}

/// 命中第 n-1 (末尾) 条规则的输入 (8 规则时为全量扫描最坏形态)
fn hit_for(mt: TriggerMatchType, n: usize) -> (String, Option<f32>) {
    let i = n - 1;
    let cmd = match mt {
        TriggerMatchType::Exact => format!("cmd{i}"),
        TriggerMatchType::Prefix => format!("cmd{i}_pre123"),
        TriggerMatchType::Contains => format!("aa cmd{i}_mid bb"),
        TriggerMatchType::Regex => format!("cmd{i}_xyz"),
        TriggerMatchType::Range => String::new(),
        TriggerMatchType::Glob => format!("cmd{i}_anything"),
    };
    let numeric = match mt {
        // 整数运算后单次转换, i ≤ 7 无实际精度损失
        TriggerMatchType::Range => Some((i * 10 + 5) as f32),
        _ => None,
    };
    (cmd, numeric)
}

const fn type_name(mt: TriggerMatchType) -> &'static str {
    match mt {
        TriggerMatchType::Exact => "exact",
        TriggerMatchType::Prefix => "prefix",
        TriggerMatchType::Contains => "contains",
        TriggerMatchType::Regex => "regex",
        TriggerMatchType::Range => "range",
        TriggerMatchType::Glob => "glob",
    }
}

fn bench_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("trigger_match");
    // 未命中任何规则的输入 (Range: -1 落在全部区间之外)
    let (miss_cmd, miss_num) = (String::from("zzz miss"), Some(-1.0));
    for mt in [
        TriggerMatchType::Exact,
        TriggerMatchType::Prefix,
        TriggerMatchType::Contains,
        TriggerMatchType::Regex,
        TriggerMatchType::Range,
        TriggerMatchType::Glob,
    ] {
        for n in [1, 8] {
            let rules: Vec<TriggerRuleDef> = (0..n)
                .map(|i| rule(&format!("r{i}"), mt, &pattern_for(mt, i)))
                .collect();
            let (hit_cmd, hit_num) = hit_for(mt, n);
            let tn = type_name(mt);
            for (label, cmd, numeric) in [
                ("hit", hit_cmd.as_str(), hit_num),
                ("miss", miss_cmd.as_str(), miss_num),
            ] {
                let mut matcher = TriggerMatcher::new(rules.clone(), -1.0, "MISS".to_string());
                group.bench_function(format!("{tn}_{n}_{label}"), |b| {
                    b.iter(|| black_box(matcher.match_input(cmd, numeric)));
                });
            }
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/trigger_match"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_match
}
criterion_main!(benches);
