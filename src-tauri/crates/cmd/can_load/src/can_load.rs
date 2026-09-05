use app_state::AppState;
use can_types::CanLoadSnapshot;
use tauri::{AppHandle, Manager, State};
use vofa_core::Result;

/// 从指定 Transport 节点的 TransportConfig 提取 CAN 波特率 (bps)
///
/// 仅 Slcan / CandleLight 配置携带 CAN 波特率; 其他传输方式返回 None。
async fn extract_can_bitrate_from_transport(state: &AppState, node_id: &str) -> Option<u32> {
    let manager = state.transport.lock().await;
    match manager.config(node_id) {
        Some(vofa_core::TransportConfig::Slcan(s)) => Some(s.can_bitrate.bps()),
        Some(vofa_core::TransportConfig::CandleLight(c)) => Some(c.can_bitrate.bps()),
        _ => None,
    }
}

/// 计算有效 CAN 波特率 (bps) — 纯决策 (优先级: override>0 > transport 配置 > 默认)
///
/// 抽出为无 IO 纯函数供单测覆盖优先级; IO 侧只负责取 transport 配置值。
fn resolve_can_bitrate_choice(override_bps: Option<u32>, transport_bps: Option<u32>) -> u32 {
    if let Some(bps) = override_bps {
        if bps > 0 {
            return bps;
        }
    }
    transport_bps.unwrap_or(500_000)
}

/// 计算有效 CAN 波特率 (bps)
///
/// - 若 `override_bps` 为 Some(n) 且 n > 0, 使用 n (手动覆盖)
/// - 否则尝试从指定 Transport 节点的配置读取
/// - 都没有则返回 500_000 (默认值, 避免前端传 0 导致除零)
async fn resolve_can_bitrate(state: &AppState, node_id: &str, override_bps: Option<u32>) -> u32 {
    let transport_bps = extract_can_bitrate_from_transport(state, node_id).await;
    resolve_can_bitrate_choice(override_bps, transport_bps)
}

/// 获取 CAN 负载统计快照
///
/// `node_id`: 用于自动解析波特率的 Transport 节点 id
/// `bitrate_bps`: 可选手动覆盖波特率; None/0 = 自动从 TransportConfig 读取
#[tauri::command]
pub async fn get_can_load_stats(
    state: State<'_, AppState>,
    node_id: String,
    bitrate_bps: Option<u32>,
) -> Result<CanLoadSnapshot> {
    let bitrate = resolve_can_bitrate(&state, &node_id, bitrate_bps).await;
    let load_stats = state.can_load_stats.lock();
    Ok(load_stats.snapshot(bitrate))
}

/// 设置 CAN 负载统计滑动窗口大小 (微秒)
///
/// 例如 1_000_000 = 1 秒, 100_000 = 100ms
#[tauri::command]
pub async fn set_can_load_window(state: State<'_, AppState>, window_us: u64) -> Result<()> {
    state.can_load_stats.lock().set_window_us(window_us);
    Ok(())
}

/// 清空 CAN 负载统计
#[tauri::command]
pub async fn clear_can_load_stats(state: State<'_, AppState>) -> Result<()> {
    state.can_load_stats.lock().clear();
    Ok(())
}

/// 获取指定 Transport 节点的当前 CAN 波特率 (从 TransportConfig 提取, 用于前端 UI 默认值)
///
/// 返回 (bps, source) — source 描述来源 ("slcan" / "candle" / "default")
#[tauri::command]
pub async fn get_current_can_bitrate(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<(u32, String)> {
    let manager = state.transport.lock().await;
    if let Some(cfg) = manager.config(&node_id) {
        match cfg {
            vofa_core::TransportConfig::Slcan(s) => {
                return Ok((s.can_bitrate.bps(), "slcan".to_string()));
            }
            vofa_core::TransportConfig::CandleLight(c) => {
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
#[tauri::command]
pub async fn export_can_load_csv(
    state: State<'_, AppState>,
    app: AppHandle,
    node_id: String,
    bitrate_bps: Option<u32>,
) -> Result<String> {
    use std::io::Write;

    let bitrate = resolve_can_bitrate(&state, &node_id, bitrate_bps).await;
    let snap = state.can_load_stats.lock().snapshot(bitrate);

    // 生成时间戳 (本地时间, 不依赖 chrono)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (yyyy, mm, dd, hh, min, ss) = secs_to_local_components(now);
    let timestamp_str = format!("{yyyy:04}-{mm:02}-{dd:02}T{hh:02}:{min:02}:{ss:02}");
    let filename = format!("vofa-can-load-{yyyy:04}{mm:02}{dd:02}-{hh:02}{min:02}{ss:02}.csv");

    let csv = format_can_load_csv(&snap, bitrate, &timestamp_str);

    // 选择保存路径: 优先 Downloads, 失败则用当前目录
    let path = match app.path().download_dir() {
        Ok(d) => d.join(&filename),
        Err(_) => std::env::current_dir()
            .map(|d| d.join(&filename))
            .map_err(|e| vofa_core::Error::Config(error::ConfigError::DownloadDir(e)))?,
    };

    let mut file = std::fs::File::create(&path)?;
    file.write_all(csv.as_bytes())?;

    log::info!("CAN 负载 CSV 已导出: {}", path.display());
    Ok(path.to_string_lossy().to_string())
}

/// 将 UNIX 秒数转换为本地时间组件 (年月日时分秒)
/// 简化实现, 不依赖 chrono — 假设本地时区为系统设置的时区
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // unix 秒值域远小于 c_long 上限, tm 字段均为非负小整数
fn secs_to_local_components(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // 用 libc localtime_r 获取本地时间 (跨平台)
    #[cfg(unix)]
    {
        use std::os::raw::{c_char, c_int, c_long};
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
        // SAFETY: `t` 指向本函数初始化的有效 c_long,`tm` 是已清零初始化的
        // libc_tm;localtime_r 仅写入 tm,按 POSIX 不修改全局状态(线程安全)。
        unsafe {
            localtime_r(&raw const t, &raw mut tm);
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
    use std::fmt::Write as _;
    let mut s = String::with_capacity(8192);
    // 元信息头
    s.push_str("# VOFA-Next CAN Load Stats Export\n");
    // String 的 write! 实现不会失败, 直接忽略返回值
    let _ = writeln!(s, "# Export Time: {export_time}");
    let _ = writeln!(s, "# Bitrate: {bitrate} bps");
    let window_desc = if snap.window_us >= 1_000_000 {
        format!("{}s", snap.window_us / 1_000_000)
    } else {
        format!("{}ms", snap.window_us / 1000)
    };
    let _ = writeln!(s, "# Window: {} us ({window_desc})", snap.window_us);
    let _ = writeln!(
        s,
        "# Summary: frames={}, total_bits={}, total_bytes={}, load_ratio={:.4}",
        snap.frame_count, snap.total_bits, snap.total_bytes, snap.load_ratio
    );
    s.push('\n');

    // Section: History
    s.push_str("# Section: History\n");
    s.push_str("timestamp_us,load_ratio,fps\n");
    for p in &snap.history {
        let _ = writeln!(s, "{},{:.6},{:.2}", p.timestamp, p.load_ratio, p.fps);
    }
    s.push('\n');

    // Section: Per-ID
    s.push_str("# Section: Per-ID\n");
    s.push_str("id_hex,extended,frame_count,total_bits,total_bytes\n");
    for id_stat in &snap.per_id {
        let _ = writeln!(
            s,
            "0x{:X},{},{},{},{}",
            id_stat.id,
            id_stat.extended,
            id_stat.frame_count,
            id_stat.total_bits,
            id_stat.total_bytes
        );
    }
    s.push('\n');

    // Section: Per-ID History
    s.push_str("# Section: Per-ID History\n");
    s.push_str("id_hex,extended,timestamp_us,load_ratio\n");
    for h in &snap.per_id_history {
        for p in &h.history {
            let _ = writeln!(
                s,
                "0x{:X},{},{},{:.6}",
                h.id, h.extended, p.timestamp, p.load_ratio
            );
        }
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use can_types::{CanIdLoadHistory, CanIdLoadStats, CanLoadHistoryPoint, CanLoadSnapshot};

    fn snapshot(window_us: u64) -> CanLoadSnapshot {
        CanLoadSnapshot {
            window_us,
            frame_count: 120,
            total_bits: 12_345,
            total_bytes: 1_543,
            load_ratio: 0.25,
            history: vec![CanLoadHistoryPoint {
                timestamp: 1_000,
                load_ratio: 0.25,
                fps: 60.0,
            }],
            per_id: vec![CanIdLoadStats {
                id: 0x1ABCDEF0,
                extended: true,
                frame_count: 120,
                total_bits: 12_345,
                total_bytes: 1_543,
            }],
            per_id_history: vec![CanIdLoadHistory {
                id: 0x1ABCDEF0,
                extended: true,
                history: vec![CanLoadHistoryPoint {
                    timestamp: 2_000,
                    load_ratio: 0.1,
                    fps: 30.0,
                }],
            }],
        }
    }

    // ---- 波特率决策优先级 ----

    #[test]
    fn override_bitrate_wins_over_transport() {
        assert_eq!(
            resolve_can_bitrate_choice(Some(1_000_000), Some(500_000)),
            1_000_000
        );
    }

    #[test]
    fn zero_override_falls_through_to_transport() {
        assert_eq!(resolve_can_bitrate_choice(Some(0), Some(250_000)), 250_000);
    }

    #[test]
    fn transport_config_used_when_no_override() {
        assert_eq!(resolve_can_bitrate_choice(None, Some(250_000)), 250_000);
    }

    #[test]
    fn default_500k_prevents_division_by_zero() {
        assert_eq!(resolve_can_bitrate_choice(None, None), 500_000);
        assert_eq!(resolve_can_bitrate_choice(Some(0), None), 500_000);
    }

    // ---- CSV 格式化 ----

    #[test]
    fn csv_contains_metadata_header_and_three_sections() {
        let csv = format_can_load_csv(&snapshot(1_000_000), 500_000, "2026-01-01T00:00:00");
        assert!(csv.starts_with("# VOFA-Next CAN Load Stats Export\n"));
        assert!(csv.contains("# Export Time: 2026-01-01T00:00:00"));
        assert!(csv.contains("# Bitrate: 500000 bps"));
        assert!(csv.contains(
            "# Summary: frames=120, total_bits=12345, total_bytes=1543, load_ratio=0.2500"
        ));
        // 三个 Section 按序出现
        let history = csv.find("# Section: History").expect("History section");
        let per_id = csv.find("# Section: Per-ID\n").expect("Per-ID section");
        let per_id_history = csv
            .find("# Section: Per-ID History")
            .expect("Per-ID History section");
        assert!(history < per_id && per_id < per_id_history);
        assert!(csv.contains("timestamp_us,load_ratio,fps\n"));
        assert!(csv.contains("id_hex,extended,frame_count,total_bits,total_bytes\n"));
        assert!(csv.contains("id_hex,extended,timestamp_us,load_ratio\n"));
    }

    #[test]
    fn csv_window_uses_s_or_ms_description() {
        assert!(format_can_load_csv(&snapshot(1_000_000), 500_000, "t")
            .contains("# Window: 1000000 us (1s)"));
        assert!(format_can_load_csv(&snapshot(100_000), 500_000, "t")
            .contains("# Window: 100000 us (100ms)"));
    }

    #[test]
    fn csv_ids_are_uppercase_hex_with_0x_prefix() {
        let csv = format_can_load_csv(&snapshot(1_000_000), 500_000, "t");
        assert!(csv.contains("0x1ABCDEF0,true,120,12345,1543"), "Per-ID 行");
        assert!(
            csv.contains("0x1ABCDEF0,true,2000,0.100000"),
            "Per-ID History 行"
        );
        assert!(csv.contains("1000,0.250000,60.00"), "History 行");
    }

    #[test]
    fn csv_is_empty_safe_when_no_history() {
        let mut snap = snapshot(1_000_000);
        snap.history.clear();
        snap.per_id.clear();
        snap.per_id_history.clear();
        let csv = format_can_load_csv(&snap, 500_000, "t");
        assert!(csv.contains("# Section: History"));
        assert!(csv.matches('\n').count() >= 8, "空数据也应有表头结构");
    }

    // ---- 本地时间换算 (Unix 路径依赖 TZ, 只断言范围) ----

    #[test]
    fn local_components_are_in_valid_ranges() {
        let (year, month, day, hour, minute, second) = secs_to_local_components(0);
        assert!(year >= 1970);
        assert!((1..=12).contains(&month), "month={month}");
        assert!((1..=31).contains(&day), "day={day}");
        assert!(hour <= 23 && minute <= 59 && second <= 59);
    }
}
