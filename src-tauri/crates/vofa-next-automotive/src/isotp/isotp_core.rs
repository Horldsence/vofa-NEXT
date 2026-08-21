//! ISO-TP 核心状态机与常量

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{sleep, timeout};
use vofa_next_core::{CanDirection, CanFrame, IsoTpConfig};
use vofa_next_transport::CanBackend;

use crate::{AutomotiveError, AutomotiveResult};

// ============ PCI 常量 ============

const PCI_TYPE_MASK: u8 = 0xF0;
const PCI_SF: u8 = 0x00; // Single Frame
const PCI_FF: u8 = 0x10; // First Frame
const PCI_CF: u8 = 0x20; // Consecutive Frame
const PCI_FC: u8 = 0x30; // Flow Control

/// FC 帧的 FlowStatus 字段
const FC_CTS: u8 = 0x00; // Continue To Send
const FC_WAIT: u8 = 0x01; // Wait
const FC_OVERFLOW: u8 = 0x02; // Overflow

/// SF 最大数据长度 (经典 CAN 8 字节 - 1 PCI - 1 SF_DL = 7)
const SF_MAX_DATA: usize = 7;
/// FF 一次携带的数据长度 (8 - 2 字节 PCI = 6)
const FF_DATA_LEN: usize = 6;
/// CF 一次携带的数据长度 (8 - 1 字节 PCI = 7)
const CF_DATA_LEN: usize = 7;
/// FF_DL 最大值 (12 位)
const FF_DL_MAX: usize = 0xFFF;

/// 默认超时 (ISO 15765-2 推荐,单位 ms)
const DEFAULT_N_BS_MS: u64 = 1000; // 发送方等 FC
const DEFAULT_N_CR_MS: u64 = 1000; // 接收方等 CF
const DEFAULT_N_AS_MS: u64 = 100; // 发送方等 CAN ACK
const DEFAULT_N_AR_MS: u64 = 100; // 接收方等 CAN ACK

// ============ 命令通道 ============

/// 主 task 接收的命令
enum IsoTpCmd {
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
struct Receiver {
    /// 期望的总字节数 (来自 FF_DL)
    expected_len: usize,
    /// 已累积的字节缓冲
    buffer: Vec<u8>,
    /// 期望的下一个 CF 的 SN (0-15 循环)
    next_sn: u8,
}

impl Receiver {
    fn new(expected_len: usize) -> Self {
        Self {
            expected_len,
            buffer: Vec::with_capacity(expected_len),
            next_sn: 1,
        }
    }

    fn push_cf(&mut self, sn: u8, data: &[u8]) -> AutomotiveResult<Option<Vec<u8>>> {
        if sn != self.next_sn {
            return Err(AutomotiveError::IsoTp(format!(
                "SN 不匹配: 期望 0x{:X} 收到 0x{:X}",
                self.next_sn, sn
            )));
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

struct Pending {
    tx_id: u32,
    response_tx: Option<oneshot::Sender<AutomotiveResult<Vec<u8>>>>,
    state: PendingState,
}

enum PendingState {
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
    fn complete(&mut self, result: AutomotiveResult<Vec<u8>>) {
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(result);
        }
    }
}

// ============ IsoTpSession ============

#[derive(Clone)]
pub struct IsoTpSessionHandle {
    cmd_tx: mpsc::Sender<IsoTpCmd>,
}

impl IsoTpSessionHandle {
    pub async fn send_request(
        &self,
        tx_id: u32,
        rx_id: u32,
        data: Vec<u8>,
    ) -> AutomotiveResult<Vec<u8>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx
            .send(IsoTpCmd::SendRequest {
                tx_id,
                rx_id,
                data,
                response_tx: resp_tx,
            })
            .await
            .map_err(|_| AutomotiveError::IsoTp("会话已关闭".into()))?;
        resp_rx
            .await
            .map_err(|_| AutomotiveError::IsoTp("会话任务崩溃".into()))?
    }
}

pub struct IsoTpSession {
    handle: IsoTpSessionHandle,
    join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl IsoTpSession {
    pub fn new(backend: Arc<dyn CanBackend>, config: IsoTpConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let frame_rx = backend.subscribe_frames();
        let join_handle = tokio::spawn(async move {
            run_session(backend, config, cmd_rx, frame_rx).await;
        });
        Self {
            handle: IsoTpSessionHandle { cmd_tx },
            join_handle: Some(join_handle),
        }
    }

    pub fn handle(&self) -> IsoTpSessionHandle {
        self.handle.clone()
    }

    pub async fn shutdown(mut self) {
        let _ = self.handle.cmd_tx.send(IsoTpCmd::Shutdown).await;
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.await;
        }
    }
}

impl Drop for IsoTpSession {
    fn drop(&mut self) {
        let _ = self.handle.cmd_tx.try_send(IsoTpCmd::Shutdown);
    }
}

// ============ 后台任务 ============

// n_as/n_bs/n_cr/n_ar 为 ISO 15765-2 标准定时参数名, 按规范命名故允许相似
#[allow(clippy::similar_names)]
async fn run_session(
    backend: Arc<dyn CanBackend>,
    config: IsoTpConfig,
    mut cmd_rx: mpsc::Receiver<IsoTpCmd>,
    mut frame_rx: broadcast::Receiver<CanFrame>,
) {
    let n_bs = Duration::from_millis(u64::from(config.timeout_ms).max(DEFAULT_N_BS_MS));
    let n_cr = Duration::from_millis(u64::from(config.timeout_ms).max(DEFAULT_N_CR_MS));
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
                            &backend, &config, &mut pending,
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

// ============ 发送流程 ============

async fn start_send_request(
    backend: &Arc<dyn CanBackend>,
    config: &IsoTpConfig,
    pending: &mut HashMap<u32, Pending>,
    tx_id: u32,
    rx_id: u32,
    data: Vec<u8>,
    response_tx: oneshot::Sender<AutomotiveResult<Vec<u8>>>,
    n_as: Duration,
) {
    if data.len() > FF_DL_MAX {
        let _ = response_tx.send(Err(AutomotiveError::IsoTp(format!(
            "数据超长: {} > {FF_DL_MAX}",
            data.len()
        ))));
        return;
    }

    if data.len() <= SF_MAX_DATA {
        let mut frame_data = vec![0u8; 8];
        frame_data[0] = PCI_SF | u8::try_from(data.len()).expect("SF 数据长度不超过 7");
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
        ff[1] = u8::try_from((data.len() >> 8) & 0x0F).expect("已掩码为 4 位");
        ff[2] = u8::try_from(data.len() & 0xFF).expect("已掩码为 8 位");
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

async fn send_consecutive_frames(
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

// ============ 接收流程 ============

async fn handle_received_frame(
    backend: &Arc<dyn CanBackend>,
    config: &IsoTpConfig,
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

    let pci_type = frame.data[0] & PCI_TYPE_MASK;

    match pci_type {
        PCI_FC => handle_fc_frame(backend, pending, frame, n_as).await,
        PCI_SF => {
            handle_sf_frame(pending, frame);
            Ok(())
        }
        PCI_FF => handle_ff_frame(backend, config, pending, frame, n_as).await,
        PCI_CF => {
            handle_cf_frame(pending, frame);
            Ok(())
        }
        _ => Ok(()),
    }
}

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

async fn handle_ff_frame(
    backend: &Arc<dyn CanBackend>,
    _config: &IsoTpConfig,
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

// ============ 辅助函数 ============

async fn send_can_frame(
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
        .map_err(|_| AutomotiveError::Timeout(format!("N_As 超时 (发送 CAN 帧 id=0x{tx_id:X})")))?;
    result.map_err(|e| {
        AutomotiveError::Timeout(format!("N_As 超时 (发送 CAN 帧 id=0x{tx_id:X}): {e}"))
    })
}

// const fn 中无法使用 From trait (非常量), 只能用 as 转换
#[allow(clippy::cast_lossless)]
const fn st_min_to_duration(st_min: u8) -> Duration {
    match st_min {
        0..=127 => Duration::from_millis(st_min as u64),
        241..=249 => Duration::from_micros((st_min as u64 - 240) * 100),
        _ => Duration::ZERO,
    }
}
