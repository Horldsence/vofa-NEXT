//! ISO-TP 接收流程 — handle_received_frame + 各 PCI handler

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use can_types::{CanDirection, CanFrame};
use transport_core::CanBackend;

use crate::constants::{
    pci_type, FC_CTS, FC_OVERFLOW, FC_WAIT, FF_DATA_LEN, FF_DL_MAX, PCI_CF, PCI_FF, PCI_FC,
    PCI_SF, SF_MAX_DATA,
};
use crate::error::{AutomotiveError, AutomotiveResult};
use crate::state::{Pending, PendingState, Receiver};
use crate::tx::{send_can_frame, send_consecutive_frames};

/// 接收帧处理 — 根据 PCI 类型分派
pub async fn handle_received_frame(
    backend: &Arc<dyn CanBackend>,
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
    _n_bs: Duration,
    _n_cr: Duration,
    n_as: Duration,
    _n_ar: Duration,
) -> AutomotiveResult<()> {
    if frame.direction == CanDirection::Tx {
        return Ok(());
    }
    if frame.data.is_empty() {
        return Ok(());
    }

    match pci_type(frame.data[0]) {
        PCI_FC => handle_fc_frame(backend, pending, frame, n_as).await,
        PCI_SF => {
            handle_sf_frame(pending, frame);
            Ok(())
        }
        PCI_FF => handle_ff_frame(backend, pending, frame, n_as).await,
        PCI_CF => {
            handle_cf_frame(pending, frame);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 处理 FC (Flow Control) 帧 — 触发后续 CF 发送
async fn handle_fc_frame(
    backend: &Arc<dyn CanBackend>,
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
    n_as: Duration,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let Some(pending_entry) = pending.get_mut(&rx_id) else {
        return Ok(());
    };

    let tx_id = pending_entry.tx_id;
    let (data, mut offset, mut next_sn) = match &pending_entry.state {
        PendingState::WaitingForFc {
            data,
            offset,
            next_sn,
        } => (data.clone(), *offset, *next_sn),
        _ => return Ok(()),
    };

    let fs = frame.data.get(1).copied().unwrap_or(0);
    let bs = frame.data.get(2).copied().unwrap_or(0);
    let st_min = frame.data.get(3).copied().unwrap_or(0);

    if fs == FC_OVERFLOW {
        pending_entry.complete(Err(AutomotiveError::IsoTp("对端 FC OVERFLOW".into())));
        pending.remove(&rx_id);
        return Ok(());
    }
    if fs == FC_WAIT {
        pending_entry.state = PendingState::WaitingForFc {
            data,
            offset,
            next_sn,
        };
        return Ok(());
    }

    let result = send_consecutive_frames(
        backend,
        tx_id,
        &data,
        &mut offset,
        &mut next_sn,
        bs,
        st_min,
        n_as,
    )
    .await;

    match result {
        Ok(true) => {
            pending_entry.state = PendingState::WaitingForResponse;
        }
        Ok(false) => {
            pending_entry.state = PendingState::WaitingForFc {
                data,
                offset,
                next_sn,
            };
        }
        Err(e) => {
            pending_entry.complete(Err(e));
            pending.remove(&rx_id);
        }
    }
    Ok(())
}

/// 处理 SF (Single Frame) — 直接交付响应
fn handle_sf_frame(pending: &mut HashMap<u32, Pending>, frame: &CanFrame) {
    let rx_id = frame.id;
    let sf_dl = (frame.data[0] & 0x0F) as usize;
    if sf_dl == 0 || sf_dl > SF_MAX_DATA {
        return;
    }
    let data = frame.data[1..=sf_dl].to_vec();

    let Some(pending_entry) = pending.get_mut(&rx_id) else {
        return;
    };
    pending_entry.complete(Ok(data));
    pending.remove(&rx_id);
}

/// 处理 FF (First Frame) — 回复 FC + 进入接收状态机
async fn handle_ff_frame(
    backend: &Arc<dyn CanBackend>,
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
    n_as: Duration,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let ff_dl = (((frame.data[0] & 0x0F) as usize) << 8) | (frame.data[1] as usize);
    if ff_dl == 0 || ff_dl > FF_DL_MAX {
        return Ok(());
    }

    let Some(pending_entry) = pending.get_mut(&rx_id) else {
        return Ok(());
    };

    let tx_id = pending_entry.tx_id;
    let fc = [PCI_FC, FC_CTS, 0, 0, 0, 0, 0, 0];
    if let Err(e) = send_can_frame(backend, tx_id, &fc, n_as).await {
        pending_entry.complete(Err(e));
        pending.remove(&rx_id);
        return Ok(());
    }

    let mut receiver = Receiver::new(ff_dl);
    let take = ff_dl.min(FF_DATA_LEN);
    receiver.buffer.extend_from_slice(&frame.data[2..2 + take]);
    if receiver.buffer.len() >= ff_dl {
        pending_entry.complete(Ok(std::mem::take(&mut receiver.buffer)));
        pending.remove(&rx_id);
        return Ok(());
    }
    pending_entry.state = PendingState::Receiving { receiver };
    Ok(())
}

/// 处理 CF (Consecutive Frame) — 累积数据,SN 不匹配则报错
fn handle_cf_frame(pending: &mut HashMap<u32, Pending>, frame: &CanFrame) {
    let rx_id = frame.id;
    let sn = frame.data[0] & 0x0F;
    let data = &frame.data[1..8.min(frame.data.len())];

    let Some(pending_entry) = pending.get_mut(&rx_id) else {
        return;
    };

    let PendingState::Receiving { receiver } = &mut pending_entry.state else {
        return;
    };

    match receiver.push_cf(sn, data) {
        Ok(Some(complete)) => {
            pending_entry.complete(Ok(complete));
            pending.remove(&rx_id);
        }
        Ok(None) => {}
        Err(e) => {
            pending_entry.complete(Err(e));
            pending.remove(&rx_id);
        }
    }
}