//! CAN 后端抽象 — 用于诊断层 (ISO-TP / UDS / OBD-II / J1939) 接入底层 CAN 帧流
//!
//! 这是 [`protocol_engine`] 中 `ProtocolEngine::feed` 的对偶:
//! - `ProtocolEngine` 把"原始字节流 → CanFrame" 的解码做掉
//! - `CanBackend` 把"CanFrame 收发" 暴露成统一接口给上层诊断引擎使用
//!
//! 实现通常由 `automotive_can` crate 提供,通过桥接
//! `TransportManager` 的原始字节流 + `ProtocolEngine` 编解码完成。
//!
//! 设计为 `async_trait` + `Send + Sync`,可在 tokio task 间共享。

use async_trait::async_trait;
use tokio::sync::broadcast;
use can_types::CanFrame;
use vofa_core::Result;

/// CAN 后端 — 给诊断引擎提供 CanFrame 收发能力的抽象
///
/// 一个 `CanBackend` 实例对应一条活动的 CAN 总线连接 (Slcan / CandleLight / SocketCAN)。
/// 上层诊断引擎通过 [`subscribe_frames`] 获取实时 CanFrame 流,
/// 通过 [`send_frame`] 把诊断请求 (ISO-TP 单帧/多帧,UDS/OBD-II PDU) 推到总线。
#[async_trait]
pub trait CanBackend: Send + Sync {
    /// 发送一帧到 CAN 总线
    ///
    /// 实现内部负责按底层传输格式 (slcan ASCII / candleLight 二进制) 编码,
    /// 并通过 `TransportManager` 的 write_tx 推到设备。
    async fn send_frame(&self, frame: &CanFrame) -> Result<()>;

    /// 订阅 CanFrame 流 — 多消费者语义
    ///
    /// 每次 call 返回独立的 Receiver,与其它订阅者互不干扰。
    /// 实现内部从 TransportManager 的字节流订阅,经 ProtocolEngine 解码后广播。
    fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame>;

    /// 后端名称 (用于日志/调试)
    fn name(&self) -> &str;
}