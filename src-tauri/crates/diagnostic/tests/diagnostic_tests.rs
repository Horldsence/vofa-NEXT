//! `diagnostic` 全模块覆盖测试
//!
//! - UDS 服务/NRC 字节往返
//! - OBD-II mode/DTC 状态位
//! - J1939 ID 字段
//! - 各默认配置字段值
//! - `DiagnosticMessage` 联合类型 serde internally-tagged 序列化

use diagnostic::{
    DiagnosticConfig, DiagnosticMessage, Dtc, DtcStatus, IsoTpAddressMode, IsoTpConfig,
    J1939Config, J1939Id, J1939Spn, ObdConfig, ObdMode, UdsConfig, UdsNrc, UdsService,
};

// ============ UDS ============

#[test]
fn uds_service_byte_round_trip() {
    let cases = [
        (UdsService::DiagnosticSessionControl, 0x10u8),
        (UdsService::EcuReset, 0x11),
        (UdsService::ReadDataByIdentifier, 0x22),
        (UdsService::WriteDataByIdentifier, 0x2E),
        (UdsService::TesterPresent, 0x3E),
        (UdsService::ControlDtcSetting, 0x85),
    ];
    for (svc, byte) in cases {
        assert_eq!(UdsService::from_byte(byte), svc, "from_byte {byte:#x}");
        assert_eq!(svc.to_byte(), byte, "to_byte {byte:#x}");
    }
}

#[test]
fn uds_service_other_preserves_payload() {
    let other = UdsService::Other(0x99);
    assert_eq!(other.to_byte(), 0x99);
    assert_eq!(UdsService::from_byte(0x99), other);
}

#[test]
fn uds_nrc_from_byte_known_values() {
    let pairs = [
        (0x10, UdsNrc::GeneralReject),
        (0x11, UdsNrc::ServiceNotSupported),
        (0x22, UdsNrc::ConditionsNotCorrect),
        (0x72, UdsNrc::GeneralProgrammingFailure),
    ];
    for (b, expected) in pairs {
        assert_eq!(UdsNrc::from_byte(b), expected);
    }
}

#[test]
fn uds_nrc_unknown_falls_to_other() {
    assert_eq!(UdsNrc::from_byte(0xFF), UdsNrc::Other(0xFF));
}

// ============ OBD-II ============

#[test]
fn obd_mode_byte_round_trip() {
    let cases = [
        (ObdMode::CurrentData, 0x01u8),
        (ObdMode::FreezeFrame, 0x02),
        (ObdMode::ReadDtc, 0x03),
        (ObdMode::PermanentDtc, 0x0A),
    ];
    for (mode, byte) in cases {
        assert_eq!(ObdMode::from_byte(byte), mode);
        assert_eq!(mode.to_byte(), byte);
    }
}

#[test]
fn obd_mode_unknown_preserves_payload() {
    assert_eq!(ObdMode::from_byte(0xCC), ObdMode::Other(0xCC));
    assert_eq!(ObdMode::Other(0xCC).to_byte(), 0xCC);
}

#[test]
fn dtc_status_bits() {
    // bit 0: active
    assert!(DtcStatus::new(0b0000_0001).is_active());
    assert!(!DtcStatus::new(0b0000_0010).is_active());
    // bit 2: pending
    assert!(DtcStatus::new(0b0000_0100).is_pending());
    // bit 3: confirmed/permanent
    assert!(DtcStatus::new(0b0000_1000).is_confirmed());
    assert!(DtcStatus::new(0b0000_1000).is_permanent());
}

#[test]
fn dtc_struct_serializes_code_and_status() {
    let d = Dtc {
        code: "P0420".into(),
        status: DtcStatus::new(0b0000_1001),
    };
    let j = serde_json::to_string(&d).unwrap();
    let r: Dtc = serde_json::from_str(&j).unwrap();
    assert_eq!(r.code, "P0420");
    assert!(r.status.is_confirmed());
    assert!(r.status.is_active());
}

// ============ J1939 ============

#[test]
fn j1939_id_struct_field_serde() {
    let id = J1939Id {
        priority: 6,
        pgn: 0xF004,
        source: 0x0A,
        destination: 0xFF,
    };
    let j = serde_json::to_string(&id).unwrap();
    let r: J1939Id = serde_json::from_str(&j).unwrap();
    assert_eq!(r, id);
}

#[test]
fn j1939_spn_struct_serializes_fields() {
    let s = J1939Spn {
        spn: 110,
        name: "Engine Coolant Temperature".into(),
        value: 85.0,
        unit: "°C".into(),
    };
    let j = serde_json::to_string(&s).unwrap();
    assert!(j.contains("\"spn\":110"));
    assert!(j.contains("\"name\":\"Engine Coolant Temperature\""));
}

// ============ 配置默认值 ============

#[test]
fn isotp_config_default_values() {
    let c = IsoTpConfig::default();
    assert_eq!(c.tx_id, 0x7E0);
    assert_eq!(c.rx_id, 0x7E8);
    assert_eq!(c.block_size, 0);
    assert_eq!(c.st_min, 0);
    assert_eq!(c.address_mode, IsoTpAddressMode::Normal);
    assert_eq!(c.padding, None);
    assert_eq!(c.timeout_ms, 1000);
}

#[test]
fn uds_config_default_values() {
    let c = UdsConfig::default();
    assert_eq!(c.p2_timeout_ms, 5000);
    assert_eq!(c.tester_present_interval_ms, 2000);
}

#[test]
fn obd_config_default_values() {
    let c = ObdConfig::default();
    assert_eq!(c.poll_interval_ms, 100);
    assert_eq!(c.default_request_id, 0x7DF);
    assert_eq!(c.default_response_id, 0x7E8);
}

#[test]
fn j1939_config_default_zero() {
    let c = J1939Config::default();
    assert_eq!(c.source_address, 0);
    assert_eq!(c.heartbeat_interval_ms, 0);
}

#[test]
fn diagnostic_config_default_is_uds() {
    assert!(matches!(
        DiagnosticConfig::default(),
        DiagnosticConfig::Uds { .. }
    ));
}

// ============ DiagnosticMessage ============

#[test]
fn diagnostic_message_serializes_with_internally_tagged_kind() {
    let msg = DiagnosticMessage::UdsRequest {
        timestamp: 100,
        service: UdsService::DiagnosticSessionControl,
        sub_func: 3,
        data: vec![0xDE, 0xAD],
    };
    let v = serde_json::to_value(&msg).unwrap();
    assert_eq!(v["kind"], "UdsRequest");
    assert_eq!(v["service"], "DiagnosticSessionControl");
    assert_eq!(v["sub_func"], 3);
    assert_eq!(v["data"][0], 0xDE);
}

#[test]
fn diagnostic_message_timestamp_unified() {
    let m = DiagnosticMessage::UdsErrorResponse {
        timestamp: 12345,
        service: UdsService::ReadDtcInformation,
        nrc: UdsNrc::SecurityAccessDenied,
    };
    assert_eq!(m.timestamp(), 12345);

    let m2 = DiagnosticMessage::ObdDtcList {
        timestamp: 7777,
        dtcs: vec![Dtc {
            code: "P0100".into(),
            status: DtcStatus::new(0x01),
        }],
    };
    assert_eq!(m2.timestamp(), 7777);
}

#[test]
fn diagnostic_message_batch_default_serializes() {
    let mut b = diagnostic::DiagnosticMessageBatch::new();
    assert!(b.is_empty());
    b.push(DiagnosticMessage::UdsRequest {
        timestamp: 0,
        service: UdsService::TesterPresent,
        sub_func: 0,
        data: vec![],
    });
    assert_eq!(b.len(), 1);
}
