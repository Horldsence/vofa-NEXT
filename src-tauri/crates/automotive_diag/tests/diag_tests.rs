//! `automotive_diag` 集成测试 — Phase 1 占位 + libautomotive 链接

use automotive_diag::DiagnosticEngine;

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

#[test]
fn engine_default_matches_new() {
    let a = DiagnosticEngine::default();
    let b = DiagnosticEngine::new();
    assert_eq!(a.is_ready(), b.is_ready());
}