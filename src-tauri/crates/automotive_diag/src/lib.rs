//! `automotive_diag` — 诊断协议层入口 (UDS / OBD-II / J1939)
//!
//! 当前仅暴露 `DiagnosticEngine` 占位,具体实现将在后续接入:
//! - `IsoTpSession` (来自 `automotive_isotp`)
//! - `UdsClient` (ISO 14229)
//! - `ObdClient` (ISO 15031 / SAE J1979)
//! - `J1939Decoder` (SAE J1939)
//!
//! 当前职责:
//! - 暴露 `libautomotive` 版本,验证 crate 链接
//! - 提供构造占位 API

/// 诊断引擎 — 包装 ISO-TP / UDS / OBD-II / J1939 状态机
///
/// 占位实现:实际字段在后续接入 `CanBackend` 时补全。
#[derive(Debug, Default)]
pub struct DiagnosticEngine {
    _priv: (),
}

impl DiagnosticEngine {
    /// 创建新的诊断引擎实例
    pub const fn new() -> Self {
        Self { _priv: () }
    }

    /// 自检:返回引擎是否就绪 (占位实现恒为 `false`)
    pub const fn is_ready(&self) -> bool {
        false
    }

    /// 占位:返回 `libautomotive` 版本字符串
    pub const fn libautomotive_version() -> &'static str {
        libautomotive::VERSION
    }
}