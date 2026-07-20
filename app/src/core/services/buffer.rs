use crate::core::state::AppState;
use vofa_next_buffer::WaveformWindow;
use vofa_next_core::Result;

/// 同步查询: 获取最近 N 个波形点
pub async fn get_recent_waveform(state: &AppState, count: usize) -> Result<WaveformWindow> {
    let buf = state.buffer.lock();
    Ok(buf.get_recent(count))
}

/// 同步查询: 获取时间窗口内的波形
///
/// start_ms / end_ms 为相对最新时间戳的偏移 (毫秒, 负数=过去)
pub async fn get_waveform_window(
    state: &AppState,
    start_ms: i64,
    end_ms: i64,
) -> Result<WaveformWindow> {
    let buf = state.buffer.lock();
    Ok(buf.get_window(start_ms, end_ms))
}

/// 清空数据缓冲区
pub async fn clear_buffer(state: &AppState) -> Result<()> {
    state.buffer.lock().clear();
    Ok(())
}

/// 设置缓冲区通道数 (清空已有数据)
pub async fn set_buffer_channels(state: &AppState, count: usize) -> Result<()> {
    state.buffer.lock().set_channels(count);
    Ok(())
}

/// 获取缓冲区当前通道数和点数
pub async fn get_buffer_info(state: &AppState) -> Result<(usize, usize)> {
    let buf = state.buffer.lock();
    Ok((buf.channel_count(), buf.point_count()))
}

/// 设置波形缓冲区最大点数
pub async fn set_waveform_buffer_capacity(state: &AppState, max_points: usize) -> Result<()> {
    state.buffer.lock().set_max_points(max_points);
    Ok(())
}

/// 设置原始数据收集器容量 (字节)
pub async fn set_rawdata_buffer_capacity(state: &AppState, capacity: usize) -> Result<()> {
    state.raw_data_collector.lock().set_capacity(capacity);
    Ok(())
}

/// 设置 CAN 帧缓冲区最大帧数
pub async fn set_can_buffer_capacity(state: &AppState, capacity: usize) -> Result<()> {
    state.can_buffer.lock().set_max_size(capacity);
    Ok(())
}

/// 设置逻辑采样缓冲区最大采样数
pub async fn set_logic_buffer_capacity(state: &AppState, capacity: usize) -> Result<()> {
    state.logic_buffer.lock().set_max_size(capacity);
    Ok(())
}

// ============ 原始数据 ============

/// 清空原始数据收集器
pub async fn clear_raw_data_collector(state: &AppState) -> Result<()> {
    state.raw_data_collector.lock().clear();
    Ok(())
}
