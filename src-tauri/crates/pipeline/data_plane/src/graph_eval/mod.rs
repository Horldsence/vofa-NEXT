//! 数值平面评估 — 槽位热路径 (process_source_batch) + 事件驱动快照评估
//!
//! 两平面重构后:
//! - 热路径按源触发: 某 Protocol 源来帧 → 仅评估"引用该源的 tab 图"与
//!   "无 ProtocolSource 的纯本地图" (后者沿用旧单源行为: 任意来帧都评估);
//!   每帧先把该帧写入 source_frames[source] (其他源保持缓存最新帧, latest-value 融合),
//!   再走 CompiledEval::run 槽位评估 — 调用方式/槽位复用/批内锁粒度与旧版一致
//! - 快照评估 (evaluate_snapshot_now): 字节/输入事件后以 source_frames 现状评估,
//!   取代旧 force_eval 空帧机制

mod guards;
mod hot_path;
mod predicates;
mod snapshot_eval;

pub use guards::{PutBack, TakeGuard};
pub use hot_path::{merge_str_map, process_source_batch, EvalBreakdown, SlotBufs};
pub use predicates::{graph_requires_full_batch, graph_triggered_by, records_waveform_history};
pub use snapshot_eval::{
    evaluate_snapshot_now, reset_all_transient_state, reset_source_transient_state,
};
