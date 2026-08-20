//! ISO-TP 后台任务主循环

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{CanFrame, IsoTpConfig};
use vofa_next_transport::CanBackend;

use crate::constants::{DEFAULT_N_AR_MS, DEFAULT_N_AS_MS, DEFAULT_N_BS_MS, DEFAULT_N_CR_MS};
use crate::rx::handle_received_frame;
use crate::state::{IsoTpCmd, Pending};
use crate::tx::start_send_request;

/// ISO-TP 会话后台任务 — 同时监听命令与 CAN 帧
pub(super) async fn run_session(
    backend: Arc<dyn CanBackend>,
    config: IsoTpConfig,
    mut cmd_rx: mpsc::Receiver<IsoTpCmd>,
    mut frame_rx: broadcast::Receiver<CanFrame>,
) {
    let n_bs = Duration::from_millis(config.timeout_ms.max(DEFAULT_N_BS_MS as u32) as u64);
    let n_cr = Duration::from_millis(config.timeout_ms.max(DEFAULT_N_CR_MS as u32) as u64);
    let n_as = Duration::from_millis(DEFAULT_N_AS_MS);
    let n_ar = Duration::from_millis(DEFAULT_N_AR_MS);

    let mut pending: HashMap<u32, Pending> = HashMap::new();

    log::debug!(
        "ISO-TP 会话启动 (tx_id=0x{:X}, rx_id=0x{:X}, N_Bs={}ms, N_Cr={}ms)",
        config.tx_id,
        config.rx_id,
        n_bs.as_millis(),
        n_cr.as_millis()
    );

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(IsoTpCmd::SendRequest { tx_id, rx_id, data, response_tx }) => {
                    start_send_request(
                        &backend, &config, &mut pending,
                        tx_id, rx_id, data, response_tx, n_as,
                    ).await;
                }
                Some(IsoTpCmd::Shutdown) | None => break,
            },
            frame_result = frame_rx.recv() => {
                match frame_result {
                    Ok(frame) => {
                        if let Err(e) = handle_received_frame(
                            &backend, &mut pending,
                            &frame, n_bs, n_cr, n_as, n_ar,
                        ).await {
                            log::debug!("ISO-TP 接收帧处理: {e}");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("ISO-TP frame_rx 滞后 {n} 帧");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    log::debug!("ISO-TP 会话退出");
}