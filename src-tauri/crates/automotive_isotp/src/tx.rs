//! ISO-TP 发送流程 — start_send_request + send_consecutive_frames + 辅助

use can_types::{CanDirection, CanFrame};
use diagnostic::IsoTpConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};
use transport_core::CanBackend;

use crate::constants::{CF_DATA_LEN, FF_DATA_LEN, FF_DL_MAX, PCI_CF, PCI_FF, PCI_SF, SF_MAX_DATA};
use crate::error::{AutomotiveError, AutomotiveResult};
use crate::state::{Pending, PendingState};

/// 启动一次发送请求 (SF 或 FF 首帧)
pub async fn start_send_request(
    backend: &Arc<dyn CanBackend>,
    config: &IsoTpConfig,
    pending: &mut std::collections::HashMap<u32, Pending>,
    tx_id: u32,
    rx_id: u32,
    data: Vec<u8>,
    response_tx: oneshot::Sender<AutomotiveResult<Vec<u8>>>,
    n_as: Duration,
) {
    if data.len() > FF_DL_MAX {
        let _ = response_tx.send(Err(AutomotiveError::IsoTpDataTooLong {
            length: data.len(),
            max: FF_DL_MAX,
        }));
        return;
    }

    if data.len() <= SF_MAX_DATA {
        let mut frame_data = vec![0u8; 8];
        frame_data[0] = PCI_SF | u8::try_from(data.len()).unwrap_or(0);
        frame_data[1..=data.len()].copy_from_slice(&data);
        if let Some(pad) = config.padding {
            for b in &mut frame_data[1 + data.len()..] {
                *b = pad;
            }
        }
        if let Err(e) = send_can_frame(backend, tx_id, &frame_data, n_as).await {
            let _ = response_tx.send(Err(e));
            return;
        }
        pending.insert(
            rx_id,
            Pending {
                tx_id,
                response_tx: Some(response_tx),
                state: PendingState::WaitingForResponse,
            },
        );
    } else {
        let mut ff = vec![0u8; 8];
        ff[0] = PCI_FF;
        ff[1] = u8::try_from((data.len() >> 8) & 0x0F).unwrap_or(0);
        ff[2] = u8::try_from(data.len() & 0xFF).unwrap_or(0);
        ff[3..3 + FF_DATA_LEN].copy_from_slice(&data[..FF_DATA_LEN]);
        if let Err(e) = send_can_frame(backend, tx_id, &ff, n_as).await {
            let _ = response_tx.send(Err(e));
            return;
        }
        pending.insert(
            rx_id,
            Pending {
                tx_id,
                response_tx: Some(response_tx),
                state: PendingState::WaitingForFc {
                    data,
                    offset: FF_DATA_LEN,
                    next_sn: 1,
                },
            },
        );
    }
}

/// 发送连续帧 (CF),遵守 block size 与 STmin
///
/// 返回 `Ok(true)` 表示全部 CF 已发完,可等待响应;
/// 返回 `Ok(false)` 表示 block size 到顶,需等待下一个 FC。
pub async fn send_consecutive_frames(
    backend: &Arc<dyn CanBackend>,
    tx_id: u32,
    data: &[u8],
    offset: &mut usize,
    next_sn: &mut u8,
    bs: u8,
    st_min: u8,
    n_as: Duration,
) -> AutomotiveResult<bool> {
    let mut block_remaining = bs;
    let st_min_dur = st_min_to_duration(st_min);

    while *offset < data.len() {
        let take = (data.len() - *offset).min(CF_DATA_LEN);
        let mut cf = vec![0u8; 8];
        cf[0] = PCI_CF | (*next_sn & 0x0F);
        cf[1..=take].copy_from_slice(&data[*offset..*offset + take]);
        send_can_frame(backend, tx_id, &cf, n_as).await?;
        *offset += take;
        *next_sn = (*next_sn + 1) & 0x0F;

        if *offset >= data.len() {
            return Ok(true);
        }

        if !st_min_dur.is_zero() {
            sleep(st_min_dur).await;
        }

        if bs > 0 {
            block_remaining = block_remaining.saturating_sub(1);
            if block_remaining == 0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// 发送 CAN 帧并应用 N_As 超时
pub async fn send_can_frame(
    backend: &Arc<dyn CanBackend>,
    tx_id: u32,
    data: &[u8],
    n_as: Duration,
) -> AutomotiveResult<()> {
    let mut data_vec = data.to_vec();
    data_vec.resize(8, 0);
    let frame = CanFrame {
        timestamp: 0,
        id: tx_id,
        extended: false,
        rtr: false,
        dlc: 8,
        data: data_vec,
        direction: CanDirection::Tx,
    };
    let result = timeout(n_as, backend.send_frame(&frame))
        .await
        .map_err(|_| AutomotiveError::IsoTpTimeout { tx_id })?;
    result.map_err(|e| {
        // backend.send_frame 返回 AppError, 还原为 std::io::Error
        let io = std::io::Error::other(e.to_string());
        AutomotiveError::BackendSend(io)
    })
}

/// STmin 字节到 Duration
/// - 0..=127: 毫秒
/// - 241..=249: 100µs 单位
/// - 其它: 0
#[allow(clippy::cast_lossless)]
const fn st_min_to_duration(st_min: u8) -> Duration {
    match st_min {
        0..=127 => Duration::from_millis(st_min as u64),
        241..=249 => Duration::from_micros((st_min as u64 - 240) * 100),
        _ => Duration::ZERO,
    }
}
