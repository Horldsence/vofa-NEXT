//! UDS 服务 ID 与否定响应码

use serde::{Deserialize, Serialize};

/// UDS 请求 SID (ISO 14229-1 服务标识符,高位固定为 0x40 之外的服务字)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum UdsService {
    /// 0x10 诊断会话控制
    DiagnosticSessionControl,
    /// 0x11 ECU 复位
    EcuReset,
    /// 0x27 安全访问
    SecurityAccess,
    /// 0x22 按 ID 读数据
    ReadDataByIdentifier,
    /// 0x23 按 ID 读内存
    ReadMemoryByAddress,
    /// 0x2E 按 ID 写数据
    WriteDataByIdentifier,
    /// 0x19 读 DTC 信息
    ReadDtcInformation,
    /// 0x14 清除 DTC
    ClearDiagnosticInformation,
    /// 0x31 例程控制
    RoutineControl,
    /// 0x34 请求下载
    RequestDownload,
    /// 0x36 传输数据
    TransferData,
    /// 0x37 请求传输退出
    RequestTransferExit,
    /// 0x3E 测试仪在线 (心跳)
    TesterPresent,
    /// 0x85 控制 DTC 设置
    ControlDtcSetting,
    /// 未知/自定义服务 (保留原始 SID 字节)
    Other(u8),
}

impl UdsService {
    /// 从 SID 字节构造
    pub const fn from_byte(sid: u8) -> Self {
        match sid {
            0x10 => Self::DiagnosticSessionControl,
            0x11 => Self::EcuReset,
            0x27 => Self::SecurityAccess,
            0x22 => Self::ReadDataByIdentifier,
            0x23 => Self::ReadMemoryByAddress,
            0x2E => Self::WriteDataByIdentifier,
            0x19 => Self::ReadDtcInformation,
            0x14 => Self::ClearDiagnosticInformation,
            0x31 => Self::RoutineControl,
            0x34 => Self::RequestDownload,
            0x36 => Self::TransferData,
            0x37 => Self::RequestTransferExit,
            0x3E => Self::TesterPresent,
            0x85 => Self::ControlDtcSetting,
            other => Self::Other(other),
        }
    }

    /// 转回 SID 字节
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::DiagnosticSessionControl => 0x10,
            Self::EcuReset => 0x11,
            Self::SecurityAccess => 0x27,
            Self::ReadDataByIdentifier => 0x22,
            Self::ReadMemoryByAddress => 0x23,
            Self::WriteDataByIdentifier => 0x2E,
            Self::ReadDtcInformation => 0x19,
            Self::ClearDiagnosticInformation => 0x14,
            Self::RoutineControl => 0x31,
            Self::RequestDownload => 0x34,
            Self::TransferData => 0x36,
            Self::RequestTransferExit => 0x37,
            Self::TesterPresent => 0x3E,
            Self::ControlDtcSetting => 0x85,
            Self::Other(b) => b,
        }
    }
}

/// UDS 否定响应码 (NRC, ISO 14229-1 §11.2)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum UdsNrc {
    /// 0x10 通用拒绝
    GeneralReject,
    /// 0x11 服务不支持
    ServiceNotSupported,
    /// 0x12 子功能不支持
    SubFunctionNotSupported,
    /// 0x13 错误的格式 / 长度
    IncorrectMessageLengthOrInvalidFormat,
    /// 0x22 条件不满足
    ConditionsNotCorrect,
    /// 0x24 请求超出范围
    RequestOutOfRange,
    /// 0x31 超出范围 (参数)
    RequestOutOfRange31,
    /// 0x33 安全访问拒绝
    SecurityAccessDenied,
    /// 0x35 无效的密钥
    InvalidKey,
    /// 0x36 超出尝试次数
    ExceedNumberOfAttempts,
    /// 0x37 所需时间延迟未到达
    RequiredTimeDelayNotExpired,
    /// 0x70 上传/下载未接受
    UploadDownloadNotAccepted,
    /// 0x72 编程失败
    GeneralProgrammingFailure,
    /// 0x73 序列号错误
    WrongBlockSequenceCounter,
    /// 未知 NRC (保留原始字节)
    Other(u8),
}

impl UdsNrc {
    /// 从 NRC 字节构造
    pub const fn from_byte(b: u8) -> Self {
        match b {
            0x10 => Self::GeneralReject,
            0x11 => Self::ServiceNotSupported,
            0x12 => Self::SubFunctionNotSupported,
            0x13 => Self::IncorrectMessageLengthOrInvalidFormat,
            0x22 => Self::ConditionsNotCorrect,
            0x24 => Self::RequestOutOfRange,
            0x31 => Self::RequestOutOfRange31,
            0x33 => Self::SecurityAccessDenied,
            0x35 => Self::InvalidKey,
            0x36 => Self::ExceedNumberOfAttempts,
            0x37 => Self::RequiredTimeDelayNotExpired,
            0x70 => Self::UploadDownloadNotAccepted,
            0x72 => Self::GeneralProgrammingFailure,
            0x73 => Self::WrongBlockSequenceCounter,
            other => Self::Other(other),
        }
    }
}
