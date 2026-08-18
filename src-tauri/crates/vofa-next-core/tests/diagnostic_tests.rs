//! 诊断模块集成测试

use vofa_next_core::diagnostic::{
    DiagnosticConfig, DiagnosticMessage, DiagnosticMessageBatch, Dtc, DtcStatus, IsoTpConfig,
    ObdMode, UdsNrc, UdsService,
};

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
    assert!(matches!(
        back.messages[0],
        DiagnosticMessage::ObdPidValue { .. }
    ));
    assert!(matches!(
        back.messages[1],
        DiagnosticMessage::ObdDtcList { .. }
    ));
}

#[test]
fn j1939_id_serialization() {
    use vofa_next_core::diagnostic::J1939Id;
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
