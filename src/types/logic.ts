// ============ 逻辑分析仪类型 ============

/// 逻辑采样 — 与 Rust LogicSample 对应
export interface LogicSample {
  /// 微秒时间戳
  timestamp: number;
  /// 通道位图, bit i = 通道 i 的电平 (0/1)
  channels: number;
  /// 实际通道数
  channel_count: number;
}

/// 逻辑采样批次 — 与 Rust LogicSampleBatch 对应
export interface LogicSampleBatch {
  samples: LogicSample[];
}

/// I2C 事件 — 与 Rust I2cEvent 对应
export type I2cEvent =
  | { Start: null }
  | { Stop: null }
  | { Address: { addr: number; read: boolean; ack: boolean } }
  | { Data: { byte: number; ack: boolean } };

/// 解码事件 — 与 Rust DecodedEvent 对应
/// 注意: Rust serde 用 externally-tagged, 形如 { "Uart": { timestamp, byte, parity_ok } }
export type DecodedEvent =
  | { Uart: { timestamp: number; byte: number; parity_ok: boolean } }
  | { I2c: { timestamp: number; event: I2cEvent } }
  | { Spi: { timestamp: number; mosi: number; miso: number } };

/// 解码事件批次 — 与 Rust DecodedEventBatch 对应
export interface DecodedEventBatch {
  events: DecodedEvent[];
}

/// 逻辑解码器配置 — 与 Rust LogicDecoderConfig 对应
export type LogicDecoderConfig =
  | { kind: 'Uart'; params: { baud_rate: number; data_bits: number; parity: 'none' | 'odd' | 'even'; stop_bits: 'one' | 'two'; channel: number } }
  | { kind: 'I2c'; params: { sda_channel: number; scl_channel: number } }
  | { kind: 'Spi'; params: { sclk_channel: number; mosi_channel: number; miso_channel: number; cs_channel: number; mode: number } };
