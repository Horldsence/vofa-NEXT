use crate::core::state::AppState;
use vofa_next_core::{CanLoadSnapshot, Result};

/// 从当前 TransportConfig 提取 CAN 波特率 (bps)
///
/// 仅 Slcan / CandleLight 配置携带 CAN 波特率; 其他传输方式返回 None。
async fn extract_can_bitrate_from_transport(state: &AppState) -> Option<u32> {
    let manager = state.transport.lock().await;
    match manager.config() {
        Some(vofa_next_core::TransportConfig::Slcan(s)) => Some(s.can_bitrate.bps()),
        Some(vofa_next_core::TransportConfig::CandleLight(c)) => Some(c.can_bitrate.bps()),
        _ => None,
    }
}

/// 计算有效 CAN 波特率 (bps)
///
/// - 若 `override_bps` 为 Some(n) 且 n > 0, 使用 n (手动覆盖)
/// - 否则尝试从当前 TransportConfig 读取
/// - 都没有则返回 500_000 (默认值, 避免传 0 导致除零)
async fn resolve_can_bitrate(state: &AppState, override_bps: Option<u32>) -> u32 {
    if let Some(bps) = override_bps {
        if bps > 0 {
            return bps;
        }
    }
    extract_can_bitrate_from_transport(state)
        .await
        .unwrap_or(500_000)
}

/// 获取 CAN 负载统计快照
///
/// `bitrate_bps`: 可选手动覆盖波特率; None/0 = 自动从 TransportConfig 读取
pub async fn get_can_load_stats(
    state: &AppState,
    bitrate_bps: Option<u32>,
) -> Result<CanLoadSnapshot> {
    let bitrate = resolve_can_bitrate(state, bitrate_bps).await;
    let stats = state.can_load_stats.lock();
    Ok(stats.snapshot(bitrate))
}

/// 设置 CAN 负载统计滑动窗口大小 (微秒)
///
/// 例如 1_000_000 = 1 秒, 100_000 = 100ms
pub async fn set_can_load_window(state: &AppState, window_us: u64) -> Result<()> {
    state.can_load_stats.lock().set_window_us(window_us);
    Ok(())
}

/// 清空 CAN 负载统计
pub async fn clear_can_load_stats(state: &AppState) -> Result<()> {
    state.can_load_stats.lock().clear();
    Ok(())
}

/// 获取当前 CAN 波特率 (从 TransportConfig 提取, 用于 UI 默认值)
///
/// 返回 (bps, source) — source 描述来源 ("slcan" / "candle" / "default")
pub async fn get_current_can_bitrate(state: &AppState) -> Result<(u32, String)> {
    let manager = state.transport.lock().await;
    if let Some(cfg) = manager.config() {
        match cfg {
            vofa_next_core::TransportConfig::Slcan(s) => {
                return Ok((s.can_bitrate.bps(), "slcan".to_string()));
            }
            vofa_next_core::TransportConfig::CandleLight(c) => {
                return Ok((c.can_bitrate.bps(), "candle".to_string()));
            }
            _ => {}
        }
    }
    Ok((500_000, "default".to_string()))
}

/// 导出 CAN 负载统计为 CSV 文件
///
/// 自动保存到用户下载目录, 文件名格式: `vofa-can-load-YYYYMMDD-HHMMSS.csv`
///
/// CSV 结构:
/// - 元信息头 (# 开头): 导出时间 / 波特率 / 窗口大小
/// - Section: History — 时间戳, 负载率, 帧率
/// - Section: Per-ID — ID, 扩展帧, 帧数, 总位数, 总字节数
/// - Section: Per-ID History — ID, 扩展帧, 时间戳, 负载率
///
/// 返回完整文件路径
pub async fn export_can_load_csv(state: &AppState, bitrate_bps: Option<u32>) -> Result<String> {
    use std::io::Write;

    let bitrate = resolve_can_bitrate(state, bitrate_bps).await;
    let snap = state.can_load_stats.lock().snapshot(bitrate);

    // 生成时间戳 (本地时间, 不依赖 chrono)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (yyyy, mm, dd, hh, min, ss) = secs_to_local_components(now);
    let timestamp_str = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        yyyy, mm, dd, hh, min, ss
    );
    let filename = format!(
        "vofa-can-load-{:04}{:02}{:02}-{:02}{:02}{:02}.csv",
        yyyy, mm, dd, hh, min, ss
    );

    let csv = format_can_load_csv(&snap, bitrate, &timestamp_str);

    // 选择保存路径: 优先 Downloads, 失败则用当前目录
    let path = match directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|d| d.to_path_buf()))
    {
        Some(d) => d.join(&filename),
        None => std::env::current_dir()
            .map(|d| d.join(&filename))
            .map_err(|e| vofa_next_core::Error::Config(format!("无法确定下载目录: {}", e)))?,
    };

    let mut file = std::fs::File::create(&path)?;
    file.write_all(csv.as_bytes())?;

    tracing::info!("CAN 负载 CSV 已导出: {}", path.display());
    Ok(path.to_string_lossy().to_string())
}

/// 将 UNIX 秒数转换为本地时间组件 (年月日时分秒)
/// 简化实现, 不依赖 chrono — 假设本地时区为系统设置的时区
fn secs_to_local_components(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // 用 libc localtime_r 获取本地时间 (跨平台)
    #[cfg(unix)]
    {
        use std::os::raw::*;
        extern "C" {
            fn localtime_r(time: *const c_long, result: *mut libc_tm) -> *mut libc_tm;
        }
        #[repr(C)]
        struct libc_tm {
            tm_sec: c_int,
            tm_min: c_int,
            tm_hour: c_int,
            tm_mday: c_int,
            tm_mon: c_int,
            tm_year: c_int,
            tm_wday: c_int,
            tm_yday: c_int,
            tm_isdst: c_int,
            tm_gmtoff: c_long,
            tm_zone: *const c_char,
        }
        let t: c_long = secs as c_long;
        let mut tm = libc_tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        unsafe {
            localtime_r(&t, &mut tm);
            (
                (tm.tm_year + 1900) as u32,
                (tm.tm_mon + 1) as u32,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
                tm.tm_sec as u32,
            )
        }
    }
    #[cfg(not(unix))]
    {
        // 非 Unix 简化回退: 用 UTC
        let days = secs / 86400;
        let sec_of_day = secs % 86400;
        let hh = (sec_of_day / 3600) as u32;
        let min = ((sec_of_day % 3600) / 60) as u32;
        let ss = (sec_of_day % 60) as u32;
        // 简化日期计算 (从 1970-01-01 开始)
        let mut year = 1970u32;
        let mut remaining_days = days as u32;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let days_in_year = if leap { 366 } else { 365 };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_per_month = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut month = 1u32;
        for &dim in &days_per_month {
            if remaining_days < dim {
                break;
            }
            remaining_days -= dim;
            month += 1;
        }
        (year, month, remaining_days + 1, hh, min, ss)
    }
}

/// 格式化 CanLoadSnapshot 为 CSV 字符串
fn format_can_load_csv(snap: &CanLoadSnapshot, bitrate: u32, export_time: &str) -> String {
    let mut s = String::with_capacity(8192);
    // 元信息头
    s.push_str("# VOFA-Next CAN Load Stats Export\n");
    s.push_str(&format!("# Export Time: {}\n", export_time));
    s.push_str(&format!("# Bitrate: {} bps\n", bitrate));
    s.push_str(&format!(
        "# Window: {} us ({})\n",
        snap.window_us,
        if snap.window_us >= 1_000_000 {
            format!("{}s", snap.window_us / 1_000_000)
        } else {
            format!("{}ms", snap.window_us / 1000)
        }
    ));
    s.push_str(&format!(
        "# Summary: frames={}, total_bits={}, total_bytes={}, load_ratio={:.4}\n",
        snap.frame_count, snap.total_bits, snap.total_bytes, snap.load_ratio
    ));
    s.push('\n');

    // Section: History
    s.push_str("# Section: History\n");
    s.push_str("timestamp_us,load_ratio,fps\n");
    for p in &snap.history {
        s.push_str(&format!(
            "{},{:.6},{:.2}\n",
            p.timestamp, p.load_ratio, p.fps
        ));
    }
    s.push('\n');

    // Section: Per-ID
    s.push_str("# Section: Per-ID\n");
    s.push_str("id_hex,extended,frame_count,total_bits,total_bytes\n");
    for id_stat in &snap.per_id {
        s.push_str(&format!(
            "0x{:X},{},{},{},{}\n",
            id_stat.id,
            id_stat.extended,
            id_stat.frame_count,
            id_stat.total_bits,
            id_stat.total_bytes
        ));
    }
    s.push('\n');

    // Section: Per-ID History
    s.push_str("# Section: Per-ID History\n");
    s.push_str("id_hex,extended,timestamp_us,load_ratio\n");
    for h in &snap.per_id_history {
        for p in &h.history {
            s.push_str(&format!(
                "0x{:X},{},{},{:.6}\n",
                h.id, h.extended, p.timestamp, p.load_ratio
            ));
        }
    }

    s
}
