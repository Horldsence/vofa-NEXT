//! frame_dispatch 模块集成测试
//!
//! Protocol 节点产帧 → source_frames 缓存 + 数值平面触发的端到端验证。

use app_state::AppState;
use pipeline_data_plane::{frame_dispatch, DataPlaneState};
use vofa_core::DataFrame;
use node_engine::{CompiledGraph}, node_kind::{NodeDef, NodeKind};

/// 数值平面端到端: ProtocolSource 引用 pt 源, on_frames 后快照/缓冲应有值
#[test]
fn on_frames_triggers_numeric_plane() {
    let state = AppState::new();
    let plane: DataPlaneState = state.data_plane.clone();
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
    let ns = frame_dispatch::on_frames(&plane, "pt", &frames);
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
    let plane: DataPlaneState = state.data_plane.clone();
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

    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![9.0])]);
    // 图引用的是 "other" 源, 不被 "pt" 触发 → 快照无 src_other 输出
    let snap = plane.eval.output_snapshot.lock();
    assert!(snap.values.get("src_other").is_none());
}
