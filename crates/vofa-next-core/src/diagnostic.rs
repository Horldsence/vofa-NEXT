//! 诊断协议层类型 — ISO-TP / UDS / OBD-II / J1939 统一事件模型
//!
//! 这些类型由 `vofa-next-automotive` crate 产生,通过 Tauri Channel 推送到前端。
//! 字段命名与前端 `src/types/index.ts` 中的 `DiagnosticMessage` 联合类型对齐 (snake_case)。

use serde::{Deserialize, Serialize};

// ============ 通用辅助类型 ============

/// ISO-TP 地址模式 (Normal / Extended / Mixed)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IsoTpAddressMode {
    Normal,
    Extended,
    Mixed,
}

impl Default for IsoTpAddressMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ============ UDS 类型 ============

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
    pub fn from_byte(sid: u8) -> Self {
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
    pub fn to_byte(self) -> u8 {
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
    pub fn from_byte(b: u8) -> Self {
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

// ============ OBD-II 类型 ============

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
    pub fn from_byte(b: u8) -> Self {
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

    pub fn to_byte(self) -> u8 {
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
    pub fn is_active(self) -> bool {
        self.0 & 0x01 != 0
    }
    pub fn is_pending(self) -> bool {
        self.0 & 0x04 != 0
    }
    pub fn is_permanent(self) -> bool {
        self.0 & 0x08 != 0
    }
    pub fn is_confirmed(self) -> bool {
        self.0 & 0x08 != 0
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

// ============ J1939 类型 ============

/// J1939 报文标识 (优先级 / PGN / 源地址)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct J1939Id {
    pub priority: u8,
    pub pgn: u32,
    pub source: u8,
    pub destination: u8,
}

/// J1939 SPN (Suspect Parameter Number) 解码值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct J1939Spn {
    /// SPN 编号
    pub spn: u32,
    /// 可读名称
    pub name: String,
    /// 解码后的值
    pub value: f64,
    /// 单位 (如 "rpm", "kPa", "°C")
    pub unit: String,
}

// ============ 诊断事件统一枚举 ============

/// 诊断消息 — 跨 ISO-TP/UDS/OBD-II/J1939 的统一事件模型
///
/// 序列化采用 internally-tagged,前端可按 `kind` 字段判别联合类型:
/// `{ "kind": "UdsRequest", "service": "DiagnosticSessionControl", "sub_func": 3, "data": [...] }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DiagnosticMessage {
    /// ISO-TP 原始事件 (调试用)
    IsoTpFrame {
        timestamp: u64,
        tx_id: u32,
        rx_id: u32,
        data: Vec<u8>,
        direction: super::can::CanDirection,
    },

    /// UDS 请求
    UdsRequest {
        timestamp: u64,
        service: UdsService,
        sub_func: u8,
        data: Vec<u8>,
    },

    /// UDS 肯定响应
    UdsResponse {
        timestamp: u64,
        service: UdsService,
        data: Vec<u8>,
    },

    /// UDS 否定响应 (NRC)
    UdsErrorResponse {
        timestamp: u64,
        service: UdsService,
        nrc: UdsNrc,
    },

    /// OBD-II 请求
    ObdRequest {
        timestamp: u64,
        mode: ObdMode,
        pid: u8,
    },

    /// OBD-II PID 解码值
    ObdPidValue {
        timestamp: u64,
        mode: ObdMode,
        pid: u8,
        value: f32,
        unit: String,
    },

    /// OBD-II DTC 列表 (Mode 03/07/0A 响应)
    ObdDtcList {
        timestamp: u64,
        dtcs: Vec<Dtc>,
    },

    /// J1939 PGN 完整报文
    J1939Pgn {
        timestamp: u64,
        id: J1939Id,
        data: Vec<u8>,
    },

    /// J1939 SPN 解码值 (一条 PGN 可产出多个 SPN)
    J1939Spn {
        timestamp: u64,
        pgn: u32,
        spns: Vec<J1939Spn>,
    },
}

/// 诊断消息批次 — 一次推送多条消息 (与 CanFrameBatch 同构)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticMessageBatch {
    pub messages: Vec<DiagnosticMessage>,
}

// ============ 诊断配置 ============

/// ISO-TP 会话配置 (与 libautomotive IsoTpConfig 概念对齐)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoTpConfig {
    pub tx_id: u32,
    pub rx_id: u32,
    pub block_size: u8,
    pub st_min: u8,
    pub address_mode: IsoTpAddressMode,
    pub padding: Option<u8>,
    pub timeout_ms: u32,
}

impl Default for IsoTpConfig {
    fn default() -> Self {
        Self {
            tx_id: 0x7E0,
            rx_id: 0x7E8,
            block_size: 0,
            st_min: 0,
            address_mode: IsoTpAddressMode::Normal,
            padding: None,
            timeout_ms: 1000,
        }
    }
}

/// UDS 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdsConfig {
    pub p2_timeout_ms: u32,
    pub tester_present_interval_ms: u32,
}

impl Default for UdsConfig {
    fn default() -> Self {
        Self {
            p2_timeout_ms: 5000,
            tester_present_interval_ms: 2000,
        }
    }
}

/// OBD-II 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObdConfig {
    /// 轮询间隔 (ms)
    pub poll_interval_ms: u32,
    /// 默认请求 ID (11-bit)
    pub default_request_id: u32,
    /// 默认响应 ID (11-bit)
    pub default_response_id: u32,
}

impl Default for ObdConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 100,
            default_request_id: 0x7DF,
            default_response_id: 0x7E8,
        }
    }
}

/// J1939 解码器配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct J1939Config {
    /// 默认源地址
    pub source_address: u8,
    /// 心跳周期 (ms)
    pub heartbeat_interval_ms: u32,
}

/// 诊断配置 — 用于 ProtocolConfig::Diagnostic 变体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DiagnosticConfig {
    /// 仅 ISO-TP 透传 (调试用)
    IsoTp { config: IsoTpConfig },
    /// UDS 客户端
    Uds { isotp: IsoTpConfig, uds: UdsConfig },
    /// OBD-II 客户端
    Obd { isotp: IsoTpConfig, obd: ObdConfig },
    /// J1939 监听器 (不需要 ISO-TP,直接吃 CanFrame)
    J1939 { j1939: J1939Config },
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self::Uds {
            isotp: IsoTpConfig::default(),
            uds: UdsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uds_service_roundtrip() {
        for sid in [0x10u8, 0x27, 0x22, 0x2E, 0x19, 0x3E, 0x85] {
            let svc = UdsService::from_byte(sid);
            assert_eq!(svc.to_byte(), sid);
        }
        // 未知 SID 保留
        let other = UdsService::from_byte(0xAB);
        assert_eq!(other.to_byte(), 0xAB);
    }

    #[test]
    fn obd_mode_roundtrip() {
        for m in [0x01, 0x03, 0x04, 0x07, 0x09, 0x0A] {
            let mode = ObdMode::from_byte(m);
            assert_eq!(mode.to_byte(), m);
        }
    }

    #[test]
    fn nrc_known_codes() {
        assert_eq!(UdsNrc::from_byte(0x11), UdsNrc::ServiceNotSupported);
        assert_eq!(UdsNrc::from_byte(0x33), UdsNrc::SecurityAccessDenied);
        assert_eq!(UdsNrc::from_byte(0xEE), UdsNrc::Other(0xEE));
    }

    #[test]
    fn dtc_status_bits() {
        let active = DtcStatus(0x01);
        assert!(active.is_active());
        assert!(!active.is_pending());

        let pending = DtcStatus(0x04);
        assert!(!pending.is_active());
        assert!(pending.is_pending());

        let permanent = DtcStatus(0x08);
        assert!(permanent.is_permanent());
    }

    #[test]
    fn diagnostic_message_serializes_with_kind_tag() {
        let msg = DiagnosticMessage::UdsRequest {
            timestamp: 12345,
            service: UdsService::DiagnosticSessionControl,
            sub_func: 0x03,
            data: vec![0x01, 0x02],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"kind\":\"UdsRequest\""), "json: {json}");
        assert!(json.contains("DiagnosticSessionControl"));
    }

    #[test]
    fn diagnostic_config_default_is_uds() {
        let cfg = DiagnosticConfig::default();
        assert!(matches!(cfg, DiagnosticConfig::Uds { .. }));
    }

    #[test]
    fn isotp_config_default_ids() {
        let cfg = IsoTpConfig::default();
        assert_eq!(cfg.tx_id, 0x7E0);
        assert_eq!(cfg.rx_id, 0x7E8);
        assert_eq!(cfg.timeout_ms, 1000);
    }

    #[test]
    fn batch_serialization_roundtrip() {
        let batch = DiagnosticMessageBatch {
            messages: vec![
                DiagnosticMessage::ObdPidValue {
                    timestamp: 1,
                    mode: ObdMode::CurrentData,
                    pid: 0x0C,
                    value: 1850.5,
                    unit: "rpm".into(),
                },
                DiagnosticMessage::ObdDtcList {
                    timestamp: 2,
                    dtcs: vec![Dtc {
                        code: "P0420".into(),
                        status: DtcStatus(0x09),
                    }],
                },
            ],
        };
        let json = serde_json::to_string(&batch).unwrap();
        let back: DiagnosticMessageBatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.messages.len(), 2);
        assert!(matches!(back.messages[0], DiagnosticMessage::ObdPidValue { .. }));
        assert!(matches!(back.messages[1], DiagnosticMessage::ObdDtcList { .. }));
    }

    #[test]
    fn j1939_id_serialization() {
        let id = J1939Id {
            priority: 6,
            pgn: 0xF004,
            source: 0x00,
            destination: 0xFF,
        };
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("\"pgn\":61444"));
        let back: J1939Id = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
