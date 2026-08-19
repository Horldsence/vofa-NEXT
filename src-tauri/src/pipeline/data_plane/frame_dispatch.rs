//! 帧分发 — Protocol 节点产帧 → source_frames 缓存 + 数值平面触发
//!
//! `source_frames` 是两平面衔接点: 字节平面每源最新帧缓存 (key = Protocol 节点 id,
//! latest-value 融合), 数值平面 ProtocolSource 节点求值时按源读取
//! (CompiledOp::ProtocolSource, 见 vofa-next-nodes)。
//!
//! 触发规则 (见 [`crate::pipeline::graph_eval::process_source_batch`]):
//! 某源来帧 → 评估"引用了该源的 tab 图"与"无 ProtocolSource 的纯本地图"
//! (后者沿用旧行为: 单源时代任意来帧都评估); 同 tab 多源时其他源用缓存最新帧。

use vofa_next_core::DataFrame;

use super::DataPlaneState;

/// Protocol 节点产出一批帧: 逐帧更新 source_frames → push 到该源自己的 DataBuffer →
/// 评估被该源触发的 tab 图 → 派生边回写到该源 buffer。
///
/// 返回数值平面耗时 ns (push_frame + 图评估 + 派生 + 频谱, 观测用)。
pub fn on_frames(plane: &DataPlaneState, source_id: &str, frames: &[DataFrame]) -> u64 {
    if frames.is_empty() {
        return 0;
    }
    let buffer = plane.buffer_for(source_id);
    let mut buf = buffer.lock();
    let mut sf = plane.eval.source_frames.lock();
    let mut breakdown = crate::pipeline::graph_eval::EvalBreakdown::default();
    crate::pipeline::graph_eval::process_source_batch(
        &plane.eval,
        &mut sf,
        source_id,
        frames,
        &mut buf,
        &mut breakdown,
    );
    breakdown.push_frame_ns + breakdown.graph_eval_ns + breakdown.derived_ns + breakdown.spectrum_ns
}

/// 快照刷新 — 字节事件 (FrameDecoder 喂入) / 输入事件 (set_input_value 等) 之后,
/// 以 source_frames 现状对所有 tab 图做一次评估并发布 output_snapshot。
///
/// 取代旧 force_eval 空帧机制: ProtocolSource 从缓存读最新值, 不再被空帧清零;
/// FrameDecoder 输出来自 decoder_states 的 last_frame 缓存。
pub fn refresh_snapshot(plane: &DataPlaneState) {
    // 克隆小 map 后即释放锁, 避免与 process_source_batch 的锁序交织
    let sf = plane.eval.source_frames.lock().clone();
    crate::pipeline::graph_eval::evaluate_snapshot_now(&plane.eval, &sf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use vofa_next_nodes::{CompiledGraph, NodeDef, NodeKind};

    /// 数值平面端到端: ProtocolSource 引用 pt 源, on_frames 后快照/缓冲应有值
    #[test]
    fn on_frames_triggers_numeric_plane() {
        let state = AppState::new();
        let plane = state.data_plane.clone();
        let graph = CompiledGraph::compile(
            "t1".into(),
            vec![NodeDef {
                id: "src1".into(),
                tab_id: "t1".into(),
                kind: NodeKind::ProtocolSource {
                    node_id: "pt".into(),
                    channels: 2,
                    port_names: None,
                },
            }],
            vec![],
        )
        .unwrap();
        plane.eval.graphs.lock().insert("t1".into(), graph);

        let frames = vec![
            DataFrame::with_timestamp(1000, vec![1.0, 2.0]),
            DataFrame::with_timestamp(2000, vec![3.0, 4.0]),
        ];
        let ns = on_frames(&plane, "pt", &frames);
        assert!(ns > 0 || frames.len() == 2); // 耗时仅观测, 不断言

        // source_frames 缓存为最新帧
        assert_eq!(
            plane.source_frames.lock().get("pt").unwrap().channels,
            vec![3.0, 4.0]
        );
        // 按源 DataBuffer 收到 2 帧
        let buf = plane.buffer_for("pt");
        let b = buf.lock();
        assert_eq!(b.point_count(), 2);
        assert_eq!(b.get_channel(0, 2), vec![1.0, 3.0]);
        drop(b);
        // 快照含 ProtocolSource 输出 (批尾发布)
        let snap = plane.eval.output_snapshot.lock();
        let ports = snap.values.get("src1").expect("src1 应有输出");
        assert_eq!(ports.get("ch0"), Some(&3.0));
        assert_eq!(ports.get("ch1"), Some(&4.0));
    }

    /// 不引用该源的图不被触发 (含其他 ProtocolSource 的图)
    #[test]
    fn on_frames_skips_unrelated_graphs() {
        let state = AppState::new();
        let plane = state.data_plane.clone();
        let graph = CompiledGraph::compile(
            "t1".into(),
            vec![NodeDef {
                id: "src_other".into(),
                tab_id: "t1".into(),
                kind: NodeKind::ProtocolSource {
                    node_id: "other".into(),
                    channels: 1,
                    port_names: None,
                },
            }],
            vec![],
        )
        .unwrap();
        plane.eval.graphs.lock().insert("t1".into(), graph);

        on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![9.0])]);
        // 图引用的是 "other" 源, 不被 "pt" 触发 → 快照无 src_other 输出
        let snap = plane.eval.output_snapshot.lock();
        assert!(snap.values.get("src_other").is_none());
    }
}
