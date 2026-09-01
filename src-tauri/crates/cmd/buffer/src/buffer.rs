//! 波形缓冲区 Tauri 命令 — 同步查询入口 + 停止快照 + 原始导出
//!
//! 波形订阅统一走 [`display`] 的 `subscribe_data` 单一入口 (VNDP 协议);
//! 本模块保留缓冲区的拉取/清空/容量命令、`compute_frame_bytes` IPC、
//! Stop 波形快照 (create/query/release) 与原始样本读取/CSV 导出。

use app_state::{AppState, WaveformSnapshot};
use buffer_databuffer::{WaveformSeriesSelection, WaveformWindow};
use serde::Serialize;
use std::io::{self, BufWriter, Write};
use std::sync::atomic::Ordering;
use tauri::State;
use vofa_core::Result;

/// 同步查询: 获取最近 N 个波形点 (source = 数据源 Protocol 节点 id)
#[tauri::command]
pub async fn get_recent_waveform(
    state: State<'_, AppState>,
    source: String,
    count: usize,
) -> Result<WaveformWindow> {
    let buf = state.data_plane.buffer_for(&source);
    let window = buf.lock().get_recent(count);
    Ok(window)
}

/// 同步查询: 获取时间窗口内的波形
///
/// start_ms / end_ms 为相对最新时间戳的偏移 (毫秒, 负数=过去)
#[tauri::command]
pub async fn get_waveform_window(
    state: State<'_, AppState>,
    source: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<WaveformWindow> {
    let buf = state.data_plane.buffer_for(&source);
    let window = buf.lock().get_window(start_ms, end_ms);
    Ok(window)
}

#[derive(Debug, Serialize)]
pub struct WaveformSnapshotCreated {
    pub snapshot_id: String,
    pub overview: WaveformWindow,
}

fn snapshot_error(message: impl Into<String>) -> vofa_core::Error {
    io::Error::other(message.into()).into()
}

async fn resolve_waveform_buffer(
    state: &State<'_, AppState>,
    source: &str,
    snapshot_id: Option<String>,
) -> Result<std::sync::Arc<buffer_databuffer::DataBuffer>> {
    if let Some(snapshot_id) = snapshot_id {
        let snapshots = state.waveform_snapshots.lock();
        let snapshot = snapshots
            .get(&snapshot_id)
            .ok_or_else(|| snapshot_error("波形停止快照已释放或不存在"))?;
        if snapshot.source != source {
            return Err(snapshot_error("波形停止快照与当前数据源不匹配"));
        }
        return Ok(snapshot.buffer.clone());
    }

    let live = state.data_plane.buffer_for(source);
    tokio::task::spawn_blocking(move || std::sync::Arc::new(live.lock().clone()))
        .await
        .map_err(|error| snapshot_error(format!("克隆实时波形缓存失败: {error}")))
}

fn checked_absolute_range(
    buffer: &buffer_databuffer::DataBuffer,
    start_timestamp_us: u64,
    end_timestamp_us: u64,
) -> Result<(u64, u64)> {
    let (oldest, latest) = buffer
        .time_bounds_us()
        .ok_or_else(|| snapshot_error("波形缓存为空"))?;
    let requested_start = start_timestamp_us.min(end_timestamp_us);
    let requested_end = start_timestamp_us.max(end_timestamp_us);
    if requested_start < oldest || requested_end > latest {
        return Err(snapshot_error(
            "所选时间范围已超出原始环形缓存，未导出残缺数据",
        ));
    }
    Ok((requested_start, requested_end))
}

fn snapshot_memory_bytes(snapshots: &std::collections::HashMap<String, WaveformSnapshot>) -> usize {
    snapshots
        .values()
        .map(|snapshot| snapshot.estimated_bytes)
        .fold(0usize, usize::saturating_add)
}

const fn snapshot_fits_memory_budget(used: usize, requested: usize, budget: usize) -> bool {
    match used.checked_add(requested) {
        Some(total) => total <= budget,
        None => false,
    }
}

/// 克隆当前原始环形缓存，供 Stop 后缩放和平移重采样。
#[tauri::command]
pub async fn create_waveform_snapshot(
    state: State<'_, AppState>,
    source: String,
) -> Result<WaveformSnapshotCreated> {
    let buffer = state.data_plane.buffer_for(&source);
    let (snapshot_buffer, estimated_bytes, overview) = tokio::task::spawn_blocking(move || {
        let snapshot = {
            let buffer = buffer.lock();
            if buffer.point_count() == 0 {
                return None;
            }
            buffer.clone()
        };
        let estimated_bytes = snapshot.estimated_bytes();
        let overview = snapshot.get_min_max(2_000);
        Some((snapshot, estimated_bytes, overview))
    })
    .await
    .map_err(|error| snapshot_error(format!("创建停止快照失败: {error}")))?
    .ok_or_else(|| snapshot_error("当前波形缓存为空，无法创建停止快照"))?;
    let budget_bytes = state
        .pipeline_config
        .read()
        .memory_budget_mb
        .saturating_mul(1024 * 1024);
    let mut snapshots = state.waveform_snapshots.lock();
    let used_bytes = snapshot_memory_bytes(&snapshots);
    if !snapshot_fits_memory_budget(used_bytes, estimated_bytes, budget_bytes) {
        let estimated_tenths_mb = estimated_bytes.saturating_mul(10) / (1024 * 1024);
        return Err(snapshot_error(format!(
            "停止快照需要约 {}.{} MB，波形快照总量将超过当前 {} MB 内存预算",
            estimated_tenths_mb / 10,
            estimated_tenths_mb % 10,
            state.pipeline_config.read().memory_budget_mb
        )));
    }
    let snapshot_id = format!(
        "waveform-{}",
        state
            .next_waveform_snapshot_id
            .fetch_add(1, Ordering::Relaxed)
    );
    snapshots.insert(
        snapshot_id.clone(),
        WaveformSnapshot {
            source,
            buffer: std::sync::Arc::new(snapshot_buffer),
            estimated_bytes,
        },
    );
    Ok(WaveformSnapshotCreated {
        snapshot_id,
        overview,
    })
}

/// 从停止快照按新的时基和水平位置在后台重新生成 LTTB 视觉采样。
#[tauri::command]
pub async fn query_waveform_snapshot(
    state: State<'_, AppState>,
    snapshot_id: String,
    start_ms: f64,
    end_ms: f64,
    max_points: usize,
    selection: WaveformSeriesSelection,
) -> Result<WaveformWindow> {
    let buffer = state
        .waveform_snapshots
        .lock()
        .get(&snapshot_id)
        .map(|snapshot| snapshot.buffer.clone())
        .ok_or_else(|| snapshot_error("波形停止快照已释放或不存在"))?;
    tokio::task::spawn_blocking(move || {
        buffer.get_window_lttb(start_ms, end_ms, max_points, &selection)
    })
    .await
    .map_err(|error| snapshot_error(format!("LTTB 后台计算失败: {error}")))
}

/// 释放停止快照占用的原始缓存。
#[tauri::command]
pub fn release_waveform_snapshot(state: State<'_, AppState>, snapshot_id: String) -> Result<()> {
    state.waveform_snapshots.lock().remove(&snapshot_id);
    Ok(())
}

/// 读取绝对时间范围内的原始样本；用于有行数上限的剪贴板复制。
#[tauri::command]
pub async fn get_waveform_raw_range(
    state: State<'_, AppState>,
    source: String,
    snapshot_id: Option<String>,
    start_timestamp_us: u64,
    end_timestamp_us: u64,
    max_rows: usize,
    selection: WaveformSeriesSelection,
) -> Result<WaveformWindow> {
    let buffer = resolve_waveform_buffer(&state, &source, snapshot_id).await?;
    tokio::task::spawn_blocking(move || {
        let (requested_start, requested_end) =
            checked_absolute_range(&buffer, start_timestamp_us, end_timestamp_us)?;
        let window =
            buffer.get_window_raw_absolute_selected(requested_start, requested_end, &selection);
        if window.raw_window_points > max_rows {
            return Err(snapshot_error(format!(
                "所选范围包含 {} 行，超过剪贴板上限 {max_rows} 行；请使用文件导出",
                window.raw_window_points
            )));
        }
        Ok(window)
    })
    .await
    .map_err(|error| snapshot_error(format!("读取原始波形失败: {error}")))?
}

/// 按绝对时间范围将原始样本直接流式写入 CSV 文件。
#[tauri::command]
pub async fn export_waveform_csv(
    state: State<'_, AppState>,
    source: String,
    snapshot_id: Option<String>,
    start_timestamp_us: u64,
    end_timestamp_us: u64,
    selection: WaveformSeriesSelection,
    path: String,
) -> Result<usize> {
    let buffer = resolve_waveform_buffer(&state, &source, snapshot_id).await?;
    tokio::task::spawn_blocking(move || {
        let (requested_start, requested_end) =
            checked_absolute_range(&buffer, start_timestamp_us, end_timestamp_us)?;
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        let rows = buffer.write_raw_csv(&mut writer, requested_start, requested_end, &selection)?;
        writer.flush()?;
        Ok(rows)
    })
    .await
    .map_err(|error| snapshot_error(format!("CSV 后台导出失败: {error}")))?
}
/// 清空数据缓冲区 (source = 数据源 Protocol 节点 id)
#[tauri::command]
pub async fn clear_buffer(state: State<'_, AppState>, source: String) -> Result<()> {
    state.data_plane.buffer_for(&source).lock().clear();
    Ok(())
}

/// 设置缓冲区通道数 (清空已有数据)
#[tauri::command]
pub async fn set_buffer_channels(
    state: State<'_, AppState>,
    source: String,
    count: usize,
) -> Result<()> {
    state
        .data_plane
        .buffer_for(&source)
        .lock()
        .set_channels(count);
    Ok(())
}

/// 获取缓冲区当前通道数和点数
#[tauri::command]
pub async fn get_buffer_info(state: State<'_, AppState>, source: String) -> Result<(usize, usize)> {
    let buf = state.data_plane.buffer_for(&source);
    let b = buf.lock();
    Ok((b.channel_count(), b.point_count()))
}

/// 设置波形缓冲区最大点数
#[tauri::command]
pub async fn set_waveform_buffer_capacity(
    state: State<'_, AppState>,
    source: String,
    max_points: usize,
) -> Result<()> {
    state
        .data_plane
        .buffer_for(&source)
        .lock()
        .set_max_points(max_points);
    Ok(())
}
/// 命令发送帧字节打包 — 后端单一权威 (`compute_frame_bytes` IPC)
///
/// `frame`: 来自前端的 `CommandFrameDto` (snake_case 序列化)
/// `inputs`: var_ref 端口的实时输入值 (按 port_name 索引, f64 表示;
///
/// 返回 `ComputedFrameDto { bytes: Vec<u8> | null, error: String | null, per_block }`。
/// 错误时 `bytes` 为 null 并附带 `块 #N: ...` 形式错误信息。
// serde IPC 契约使用 std hasher
#[allow(clippy::implicit_hasher)]
#[tauri::command]
pub async fn compute_command_frame_bytes(
    frame: crate::CommandFrameDto,
    inputs: std::collections::HashMap<String, f64>,
) -> crate::ComputedFrameDto {
    crate::compute_frame_bytes(&frame, &inputs)
}

#[cfg(test)]
mod waveform_snapshot_tests {
    use super::{snapshot_fits_memory_budget, snapshot_memory_bytes};
    use app_state::WaveformSnapshot;
    use buffer_databuffer::DataBuffer;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn snapshot_memory_limit_accounts_for_all_live_snapshots() {
        let snapshot = |source: &str, estimated_bytes| WaveformSnapshot {
            source: source.into(),
            buffer: Arc::new(DataBuffer::new(1, 1)),
            estimated_bytes,
        };
        let snapshots = HashMap::from([
            ("one".into(), snapshot("source", 400)),
            ("two".into(), snapshot("source", 500)),
        ]);

        assert_eq!(snapshot_memory_bytes(&snapshots), 900);
        assert!(snapshot_fits_memory_budget(900, 100, 1_000));
        assert!(!snapshot_fits_memory_budget(900, 101, 1_000));
        assert!(!snapshot_fits_memory_budget(usize::MAX, 1, usize::MAX));
    }
}
