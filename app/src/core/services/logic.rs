use crate::core::state::AppState;
use vofa_next_core::{DecodedEventBatch, LogicSampleBatch, Result};

// ============ 逻辑分析仪 ============

/// 同步查询: 获取最近 N 个逻辑采样
pub async fn get_recent_logic_samples(
    state: &AppState,
    count: usize,
) -> Result<LogicSampleBatch> {
    let samples = state.logic_buffer.lock().get_recent(count);
    Ok(LogicSampleBatch { samples })
}

/// 清空逻辑采样缓冲区
pub async fn clear_logic_buffer(state: &AppState) -> Result<()> {
    state.logic_buffer.lock().clear();
    Ok(())
}

/// 获取逻辑采样缓冲区当前数量
pub async fn get_logic_buffer_info(state: &AppState) -> Result<usize> {
    Ok(state.logic_buffer.lock().len())
}

/// 同步查询: 获取最近 N 个解码事件
pub async fn get_recent_decoded_events(
    state: &AppState,
    count: usize,
) -> Result<DecodedEventBatch> {
    let events = state.decoded_buffer.lock().get_recent(count);
    Ok(DecodedEventBatch { events })
}

/// 清空解码事件缓冲区
pub async fn clear_decoded_buffer(state: &AppState) -> Result<()> {
    state.decoded_buffer.lock().clear();
    Ok(())
}

/// 获取解码事件缓冲区当前数量
pub async fn get_decoded_buffer_info(state: &AppState) -> Result<usize> {
    Ok(state.decoded_buffer.lock().len())
}
