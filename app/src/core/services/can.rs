use crate::core::state::AppState;
use vofa_next_core::{CanFrame, CandleDeviceInfo, Result};

// ============ CAN 帧相关 ============

/// 发送 CAN 帧
///
/// 通过当前协议引擎的 encode_can 编码为字节, 再通过传输层发送。
/// 若当前协议不是 CAN 协议 (encode_can 返回空), 直接返回 Ok。
pub async fn send_can_frame(state: &AppState, frame: CanFrame) -> Result<()> {
    let data = state.protocol.lock().encode_can(&frame);
    if data.is_empty() {
        return Ok(()); // 非 CAN 协议, 忽略
    }
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 同步查询: 获取最近 N 个 CAN 帧
pub async fn get_recent_can_frames(state: &AppState, count: usize) -> Result<Vec<CanFrame>> {
    Ok(state.can_buffer.lock().get_recent(count))
}

/// 清空 CAN 帧缓冲区
pub async fn clear_can_buffer(state: &AppState) -> Result<()> {
    state.can_buffer.lock().clear();
    Ok(())
}

/// 获取 CAN 缓冲区当前帧数
pub async fn get_can_buffer_info(state: &AppState) -> Result<usize> {
    Ok(state.can_buffer.lock().len())
}

/// 列出所有 candleLight 设备
pub async fn list_candle_devices() -> Result<Vec<CandleDeviceInfo>> {
    vofa_next_transport::candle::list_devices()
}
