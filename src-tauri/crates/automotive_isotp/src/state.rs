//! ISO-TP 状态机内部状态 — Receiver / Pending / PendingState / IsoTpCmd

use tokio::sync::oneshot;

use crate::{AutomotiveError, AutomotiveResult};

// ============ 命令通道 ============

/// 主 task 接收的命令
pub enum IsoTpCmd {
    /// 发起请求-响应 (发送 data,等待响应)
    SendRequest {
        tx_id: u32,
        rx_id: u32,
        data: Vec<u8>,
        response_tx: oneshot::Sender<AutomotiveResult<Vec<u8>>>,
    },
    /// 关闭会话
    Shutdown,
}

// ============ 接收状态机 ============

/// 接收方状态 (收到 FF 后转入)
pub struct Receiver {
    /// 期望的总字节数 (来自 FF_DL)
    pub(super) expected_len: usize,
    /// 已累积的字节缓冲
    pub(super) buffer: Vec<u8>,
    /// 期望的下一个 CF 的 SN (0-15 循环)
    pub(super) next_sn: u8,
}

impl Receiver {
    pub(super) fn new(expected_len: usize) -> Self {
        Self {
            expected_len,
            buffer: Vec::with_capacity(expected_len),
            next_sn: 1,
        }
    }

    pub(super) fn push_cf(&mut self, sn: u8, data: &[u8]) -> AutomotiveResult<Option<Vec<u8>>> {
        if sn != self.next_sn {
            return Err(AutomotiveError::IsoTpSequenceMismatch {
                expected: self.next_sn,
                got: sn,
            });
        }
        self.next_sn = (self.next_sn + 1) & 0x0F;
        let remaining = self.expected_len - self.buffer.len();
        let take = data.len().min(remaining);
        self.buffer.extend_from_slice(&data[..take]);
        if self.buffer.len() >= self.expected_len {
            Ok(Some(std::mem::take(&mut self.buffer)))
        } else {
            Ok(None)
        }
    }
}

// ============ Pending 状态 ============

pub struct Pending {
    pub(super) tx_id: u32,
    pub(super) response_tx: Option<oneshot::Sender<AutomotiveResult<Vec<u8>>>>,
    pub(super) state: PendingState,
}

pub enum PendingState {
    WaitingForFc {
        data: Vec<u8>,
        offset: usize,
        next_sn: u8,
    },
    WaitingForResponse,
    Receiving {
        receiver: Receiver,
    },
}

impl Pending {
    pub(super) fn complete(&mut self, result: AutomotiveResult<Vec<u8>>) {
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(result);
        }
    }
}

// ISO-TP Pending/Receiver 内部状态类型,仅在 crate 内使用
