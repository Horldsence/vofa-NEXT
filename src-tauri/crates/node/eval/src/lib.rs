//! # eval
//!
//! 节点图槽位运行时 — 快路径逐帧评估 + 快照物化。
//!
//! - [`eval`]: [`CompiledEval`] — 包裹 [`lower::SlotPlan`] 的平坦操作序列,
//!   `run` 逐帧纯数组读写零字符串哈希; `materialize` / `materialize_str`
//!   在快照发布点把槽位物化为 ValuesMap
//! - 输出值表类型别名 ([`ValuesMap`] / [`StringValuesMap`] — FxHash 优化)
//!   与每源最新帧/文本缓存类型别名 ([`SourceFramesMap`] / [`SourceTextsMap`])
//! - [`eval_ports`] / [`eval_str`]: f32/字符串端口覆盖写 helpers (稳态零分配)

mod eval;
mod eval_ports;
mod eval_str;

use rustc_hash::FxBuildHasher;
use std::collections::HashMap;

pub use eval::{CompiledEval, SourceFramesMap, SourceTextsMap};
pub use eval_ports::{node_out_entry, set_port};
pub use eval_str::{node_out_str_entry, set_str_port};

/// 图输出值表 (热路径) — FxHash 替代 SipHash, 高码率逐帧覆盖写时查找快 3~5 倍。
/// serde 对任意 BuildHasher+Default 的 HashMap 透明, 线上 JSON 格式不变。
pub type ValuesMap = HashMap<String, HashMap<String, f32, FxBuildHasher>, FxBuildHasher>;

/// 字符串输出值表 — Str 节点 String 域输出 (widgetId → portId → text), 仿 [`ValuesMap`]
pub type StringValuesMap = HashMap<String, HashMap<String, String, FxBuildHasher>, FxBuildHasher>;
