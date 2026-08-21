//! ISO-TP 集成测试 — PCI 常量验证 + Session API smoke + Handle clone

use std::sync::Arc;
use tokio::sync::broadcast;
use can_types::{CanDirection, CanFrame};
use diagnostic::{IsoTpAddressMode, IsoTpConfig};
use vofa_core::Result;
use transport_core::CanBackend;

use automotive_isotp::IsoTpSession;

/// Minimal mock — 仅用于构造 IsoTpSession,验证 API 不 panic
struct DummyBackend {
    rx: broadcast::Sender<CanFrame>,
}

#[async_trait::async_trait]
impl transport_core::CanBackend for DummyBackend {
    async fn send_frame(&self, _frame: &CanFrame) -> Result<()> {
        Ok(())
    }

    fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame> {
        self.rx.subscribe()
    }

    fn name(&self) -> &str {
        "DummyBackend"
    }
}

fn dummy_cfg() -> IsoTpConfig {
    IsoTpConfig {
        tx_id: 0x600,
        rx_id: 0x658,
        block_size: 0,
        st_min: 0,
        address_mode: IsoTpAddressMode::Normal,
        padding: None,
        timeout_ms: 1000,
    }
}

#[test]
fn pci_type_extraction_matches_iso_spec() {
    use automotive_isotp::constants::{pci_type, PCI_CF, PCI_FF, PCI_FC, PCI_SF};
    assert_eq!(PCI_SF, 0x00);
    assert_eq!(PCI_FF, 0x10);
    assert_eq!(PCI_CF, 0x20);
    assert_eq!(PCI_FC, 0x30);
    let sf_first = 0x03u8;
    assert_eq!(pci_type(sf_first), PCI_SF);
    let ff_first = 0x10 | 0x02;
    assert_eq!(pci_type(ff_first), PCI_FF);
}

#[test]
fn constants_match_iso_15765_2_spec() {
    use automotive_isotp::constants::{CF_DATA_LEN, FF_DATA_LEN, FF_DL_MAX, SF_MAX_DATA};
    assert_eq!(SF_MAX_DATA, 7);
    assert_eq!(FF_DATA_LEN, 6);
    assert_eq!(CF_DATA_LEN, 7);
    assert_eq!(FF_DL_MAX, 0xFFF);
}

#[test]
fn flow_status_constants() {
    use automotive_isotp::constants::{FC_CTS, FC_OVERFLOW, FC_WAIT};
    assert_eq!(FC_CTS, 0x00);
    assert_eq!(FC_WAIT, 0x01);
    assert_eq!(FC_OVERFLOW, 0x02);
}

#[tokio::test]
async fn handle_clone_is_cheap() {
    let (rx, _) = broadcast::channel::<CanFrame>(16);
    let backend: Arc<dyn transport_core::CanBackend> = Arc::new(DummyBackend { rx });
    let session = IsoTpSession::new(backend, dummy_cfg());
    let h1 = session.handle();
    let h2 = h1.clone();
    drop(h1);
    drop(h2);
    session.shutdown().await;
}

#[tokio::test]
async fn session_can_be_constructed_and_dropped() {
    let (rx, _) = broadcast::channel::<CanFrame>(16);
    let backend: Arc<dyn transport_core::CanBackend> = Arc::new(DummyBackend { rx });
    let session = IsoTpSession::new(backend, dummy_cfg());
    drop(session.handle());
    session.shutdown().await;
}

#[tokio::test]
async fn dummy_backend_round_trip() {
    let (rx, _) = broadcast::channel::<CanFrame>(16);
    let backend = DummyBackend { rx: rx.clone() };
    // send_frame
    let frame = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 0,
        data: vec![],
        direction: CanDirection::Tx,
    };
    backend.send_frame(&frame).await.unwrap();
    // subscribe_frames
    let mut rx_sub = backend.subscribe_frames();
    let _ = rx.send(frame.clone());
    let got = rx_sub.recv().await.unwrap();
    assert_eq!(got.id, 0x123);
    assert_eq!(backend.name(), "DummyBackend");
}

#[test]
fn automotive_error_formats_iso_variant() {
    use automotive_isotp::AutomotiveError;
    let e = AutomotiveError::IsoTp("test".into());
    let s = format!("{e}");
    assert!(s.contains("ISO-TP"));
    assert!(s.contains("test"));
}

#[test]
fn automotive_error_formats_all_variants() {
    use automotive_isotp::{AutomotiveError, AutomotiveResult};
    let cases = [
        (AutomotiveError::Uds("u".into()), "UDS"),
        (AutomotiveError::Obd("o".into()), "OBD-II"),
        (AutomotiveError::J1939("j".into()), "J1939"),
        (AutomotiveError::Backend("b".into()), "CAN 后端"),
        (AutomotiveError::Timeout("t".into()), "超时"),
        (AutomotiveError::Invalid("i".into()), "参数无效"),
    ];
    for (e, prefix) in cases {
        let s = format!("{e}");
        assert!(s.contains(prefix), "期望 '{prefix}' 出现在 '{s}'");
    }
    let _r: AutomotiveResult<()> = Err(AutomotiveError::Timeout("x".into()));
}