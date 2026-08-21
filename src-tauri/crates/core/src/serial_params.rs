//! 串口/串行协议基础参数
//!
//! 提升到 `vofa_core` 是为了避免 `logic_types`、`can_types` 等下游 crate 对完整
//! `config` crate 的依赖(下层 `config` 需要 `CanBitrate`/`LogicDecoderConfig` 等
//! 跨域类型)。基础参数类型无跨域依赖,可独立被任意下游 crate 使用。
//!
//! 本模块覆盖:
//! - [`Parity`][]: 无/奇/偶
//! - [`StopBits`]: 1/2 停止位
//! - [`FlowControl`]: 无 / 软件(XON-XOFF) / 硬件(RTS/CTS)

use serde::{Deserialize, Serialize};

/// 串口奇偶校验
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
}

/// 串口停止位数
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StopBits {
    #[default]
    One,
    Two,
}

/// 串口流控模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    #[default]
    None,
    Software,
    Hardware,
}
