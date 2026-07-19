// ============ CAN 类型 ============

/// CAN 帧方向
export type CanDirection = 'Rx' | 'Tx';

/// CAN 帧 — 与 Rust CanFrame 对应
export interface CanFrame {
  /// 微秒时间戳
  timestamp: number;
  /// CAN ID (11 位标准 / 29 位扩展)
  id: number;
  /// true = 扩展帧
  extended: boolean;
  /// true = 远程帧
  rtr: boolean;
  /// 数据长度 (0-8)
  dlc: number;
  /// 数据字节
  data: number[];
  /// 收/发方向
  direction: CanDirection;
}

/// CAN 帧批次 — 与 Rust CanFrameBatch 对应
export interface CanFrameBatch {
  frames: CanFrame[];
}

/// candleLight USB 设备信息 — 与 Rust CandleDeviceInfo 对应
export interface CandleDeviceInfo {
  bus: number;
  address: number;
  vid: number;
  pid: number;
  manufacturer: string | null;
  product: string | null;
  serial_number: string | null;
}

/// CAN 波特率预设
export type CanBitrate = 'bps100k' | 'bps125k' | 'bps250k' | 'bps500k' | 'bps1m';

/// 单个 ID 的 CAN 负载统计 — 与 Rust CanIdLoadStats 对应
export interface CanIdLoadStats {
  id: number;
  extended: boolean;
  frame_count: number;
  total_bits: number;
  total_bytes: number;
}

/// CAN 负载历史采样点 — 与 Rust CanLoadHistoryPoint 对应
export interface CanLoadHistoryPoint {
  /// 微秒时间戳
  timestamp: number;
  /// 负载率 (0.0 - 1.0+)
  load_ratio: number;
  /// 帧率 (帧/秒)
  fps: number;
}

/// CAN 负载统计快照 — 与 Rust CanLoadSnapshot 对应
export interface CanLoadSnapshot {
  /// 窗口大小 (微秒)
  window_us: number;
  /// 窗口内总帧数
  frame_count: number;
  /// 窗口内总位数
  total_bits: number;
  /// 窗口内总字节数
  total_bytes: number;
  /// 当前负载率 (0.0 - 1.0+)
  load_ratio: number;
  /// 时序采样历史
  history: CanLoadHistoryPoint[];
  /// 按 ID 的负载分布 (按 total_bits 降序)
  per_id: CanIdLoadStats[];
  /// 按 ID 的负载率历史 (用于时序图叠加显示)
  per_id_history: CanIdLoadHistory[];
}

/// 单个 ID 的负载率历史 — 与 Rust CanIdLoadHistory 对应
export interface CanIdLoadHistory {
  id: number;
  extended: boolean;
  history: CanLoadHistoryPoint[];
}

/// slcan 传输配置
export interface SlcanConfig {
  port_name: string;
  baud_rate: number;
  can_bitrate: CanBitrate;
}

/// candleLight 传输配置
export interface CandleConfig {
  bus: number;
  address: number;
  can_bitrate: CanBitrate;
  channel: number;
}
