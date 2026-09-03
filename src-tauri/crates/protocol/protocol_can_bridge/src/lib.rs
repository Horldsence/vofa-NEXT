//! CAN 桥接协议引擎 — Slcan + CandleLight + RawData
//!
//! - `slcan`: Lawicel ASCII 协议 (t/T/r/R 命令 + \r 终止)
//! - `candle`: candleLight (GSUSB) 24 字节二进制协议
//! - `rawdata`: 透传引擎 — 不解析, 仅占位 (前端直接显示字节)

pub mod candle;
pub mod rawdata;
pub mod slcan;

pub use candle::{
    CandleEngine, CAND_CMD_RX, CAND_CMD_TX, CAND_FRAME_SIZE, CAND_ID_EFF, CAND_ID_MASK, CAND_ID_RTR,
};
pub use rawdata::RawDataEngine;
pub use slcan::SlcanEngine;
