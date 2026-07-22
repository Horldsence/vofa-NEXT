//! 诊断引擎入口 — Phase 1 仅创建骨架,具体实现在 Phase 2-5 补全。
//!
//! 当前职责:
//! - 暴露 crate 公共 API 表面
//! - 后续将持有 IsoTpSession / UdsClient / ObdClient / J1939Decoder

/// 诊断引擎 — 包装 ISO-TP / UDS / OBD-II / J1939 状态机
///
/// Phase 1 占位:实际字段在 Phase 2 接入 CanBackend 时补全
pub struct DiagnosticEngine {
    _priv: (),
}

impl DiagnosticEngine {
    /// 创建新的诊断引擎实例
    ///
    /// Phase 1 占位:Phase 2 将接受 CanBackend 参数
    pub const fn new() -> Self {
        Self { _priv: () }
    }

    /// Phase 1 自检:返回引擎是否就绪
    pub const fn is_ready(&self) -> bool {
        false
    }

    /// 占位:返回 libautomotive 版本字符串
    pub const fn libautomotive_version() -> &'static str {
        libautomotive::VERSION
    }
}

impl Default for DiagnosticEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 简单冒烟测试:验证 libautomotive crate 链接成功且 VERSION 常量可访问
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libautomotive_links_and_exposes_version() {
        let v = DiagnosticEngine::libautomotive_version();
        assert!(!v.is_empty(), "libautomotive VERSION 不应为空");
    }

    #[test]
    fn engine_can_be_constructed() {
        let eng = DiagnosticEngine::new();
        assert!(!eng.is_ready(), "Phase 1 占位引擎不应就绪");
    }

    /// Phase 1 末自检:确认 AutomotiveError 可被构造与格式化
    #[test]
    fn error_formats_correctly() {
        let e = crate::AutomotiveError::IsoTp("test".into());
        let s = format!("{e}");
        assert!(s.contains("ISO-TP"));
        assert!(s.contains("test"));
        let _r: crate::AutomotiveResult<()> = Err(crate::AutomotiveError::Timeout("x".into()));
    }
}
