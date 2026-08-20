//! OBD-II (SAE J1979) 模式 + DTC

use serde::{Deserialize, Serialize};

/// OBD-II 服务模式 (SAE J1979)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ObdMode {
    /// 0x01 当前数据流
    CurrentData,
    /// 0x02 冻结帧
    FreezeFrame,
    /// 0x03 读 DTC
    ReadDtc,
    /// 0x04 清 DTC
    ClearDtc,
    /// 0x05 测试结果 (非 CAN 连续)
    TestResultsNonCan,
    /// 0x06 测试结果 (CAN 屏幕化)
    TestResultsCan,
    /// 0x07 待定 DTC
    PendingDtc,
    /// 0x08 控制操作
    ControlOperation,
    /// 0x09 车辆信息
    VehicleInfo,
    /// 0x0A 永久 DTC
    PermanentDtc,
    /// 未知模式 (保留原始字节)
    Other(u8),
}

impl ObdMode {
    /// 从 mode 字节构造
    pub const fn from_byte(b: u8) -> Self {
        match b {
            0x01 => Self::CurrentData,
            0x02 => Self::FreezeFrame,
            0x03 => Self::ReadDtc,
            0x04 => Self::ClearDtc,
            0x05 => Self::TestResultsNonCan,
            0x06 => Self::TestResultsCan,
            0x07 => Self::PendingDtc,
            0x08 => Self::ControlOperation,
            0x09 => Self::VehicleInfo,
            0x0A => Self::PermanentDtc,
            other => Self::Other(other),
        }
    }

    /// 转回 mode 字节
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::CurrentData => 0x01,
            Self::FreezeFrame => 0x02,
            Self::ReadDtc => 0x03,
            Self::ClearDtc => 0x04,
            Self::TestResultsNonCan => 0x05,
            Self::TestResultsCan => 0x06,
            Self::PendingDtc => 0x07,
            Self::ControlOperation => 0x08,
            Self::VehicleInfo => 0x09,
            Self::PermanentDtc => 0x0A,
            Self::Other(b) => b,
        }
    }
}

/// DTC 状态位掩码 (ISO 15031-6 DTC statusMask)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DtcStatus(pub u8);

impl DtcStatus {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
    pub const fn is_active(self) -> bool {
        self.0 & 0x01 != 0
    }
    pub const fn is_pending(self) -> bool {
        self.0 & 0x04 != 0
    }
    pub const fn is_permanent(self) -> bool {
        self.0 & 0x08 != 0
    }
    /// `is_confirmed` 与 `is_permanent` 在 ISO 15031-6 中 bit 含义相同 — 保留别名
    pub const fn is_confirmed(self) -> bool {
        self.is_permanent()
    }
}

/// DTC (诊断故障码) — 标准 OBD-II 5 字符代码 (如 P0420)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dtc {
    /// 5 字符代码 (如 "P0420")
    pub code: String,
    /// 状态位
    pub status: DtcStatus,
}
