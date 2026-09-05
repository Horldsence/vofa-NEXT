//! 编译期槽位评估表 ([`CompiledEval`]) — f32 热路径
//!
//! 结构: 包裹 `lower::SlotPlan` 的平坦操作序列 + 槽位读写 + 零字符串哈希。
//!
//! 逐帧评估时仅有的字符串查找是 ProtocolSource 的帧源解析
//! (每源每帧一次, 编译期预排为 `frame_sources` 下标表)。

use std::collections::HashMap;

use rustc_hash::FxBuildHasher;
use vofa_core::DataFrame;

use lower::SlotPlan;

/// 多源最新帧缓存 — key = 全局 Protocol 节点 id, value = 该源最近一帧
/// (latest-value 融合: 每个源独立缓存, 求值时按源读取)
pub type SourceFramesMap = HashMap<String, DataFrame, FxBuildHasher>;

/// 每源最新文本缓存 — key = 全局 Protocol 节点 id, value = 该源原始字节的
/// UTF-8 lossy 解码文本 (RawData 协议写入, latest-value 融合, 仿 [`SourceFramesMap`])
pub type SourceTextsMap = HashMap<String, String, FxBuildHasher>;

/// 编译期槽位评估表 — 封装编译后端产物 (lowering 产物见 `lower::SlotPlan`),
/// 逐帧评估纯数组读写
pub struct CompiledEval {
    /// lowering 产物: 双域槽位表 + 平坦操作序列 + 帧源表
    plan: SlotPlan,
}

mod accessors;
mod materialize;
mod ops;
