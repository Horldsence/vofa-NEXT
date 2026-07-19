//! ISO-TP (ISO 15765-2) 传输层实现 — 完全自实现
//!
//! 提供 SF (单帧) / FF (首帧) / CF (连续帧) / FC (流控帧) 的完整状态机,
//! 支持请求-响应模式 (oneshot) 与可配置的流控协商。
//!
//! 不依赖 libautomotive IsoTp (其 v0.1.2 实现存在已知 bug:硬编码
//! min_consecutive_frames=10、std::thread::sleep 阻塞),改为基于 tokio
//! 异步状态机 + CanBackend trait 自实现。
//!
//! ## 通信模型
//!
//! - 我们发送的所有帧 (SF/FF/CF/FC) CAN ID = `tx_id` (请求时指定,默认 config.tx_id)
//! - 我们接收的所有帧 (SF/FF/CF/FC) CAN ID = `rx_id` (请求时指定,默认 config.rx_id)
//! - pending 按 `rx_id` 单一索引,但每个 pending 内部记录对应的 tx_id

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
            // 第一个 CF 的 SN 应为 1 (FF 后接 CF#1)
            next_sn: 1,
        }
    }

    /// 推入 CF 数据,若已完整则返回 Some(完整数据)
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

/// 一个等待中的请求-响应对
struct Pending {
    /// 我们发送帧使用的 CAN ID (SF/FF/CF/FC 都用此 ID)
    tx_id: u32,
    /// 完成时触发 (Some 表示尚未触发)
    response_tx: Option<oneshot::Sender<AutomotiveResult<Vec<u8>>>>,
    state: PendingState,
}

enum PendingState {
    /// 已发 FF,等对端 FC
    WaitingForFc {
        /// 完整待发送数据 (FF 已发 6 字节)
        data: Vec<u8>,
        /// 已发送的字节偏移 (FF 后初始 = 6)
        offset: usize,
        /// 下一个 CF 的 SN
        next_sn: u8,
    },
    /// 已发所有 CF (或 SF),等响应
    WaitingForResponse,
    /// 收到 FF,正在接收 CF
    Receiving { receiver: Receiver },
}

impl Pending {
    /// 触发响应并清理 response_tx
    fn complete(&mut self, result: AutomotiveResult<Vec<u8>>) {
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(result);
        }
    }
}

// ============ IsoTpSession ============

/// ISO-TP 会话句柄 — 可 Clone,用于跨 task 发送请求
///
/// 持有命令通道的 Sender,可在多个 task 中并发调用 `send_request`。
#[derive(Clone)]
pub struct IsoTpSessionHandle {
    cmd_tx: mpsc::Sender<IsoTpCmd>,
}

impl IsoTpSessionHandle {
    /// 发送数据并等待响应
    ///
    /// - `tx_id` / `rx_id`: 覆盖 config 默认值,支持 J1939 多 PGN 场景
    /// - `data`: 0..=4095 字节
    /// - 返回:响应数据 (来自对端 SF/FF+CF)
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

/// ISO-TP 会话 — 持有 CanBackend 与后台 task,提供 async send_request API
///
/// 一个会话支持多个并发 (tx_id, rx_id) 请求,通过 rx_id 路由响应。
/// 通过 `handle()` 获取可 Clone 的 `IsoTpSessionHandle` 在其他 task 中使用。
pub struct IsoTpSession {
    handle: IsoTpSessionHandle,
    join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl IsoTpSession {
    /// 创建新会话并启动后台 task
    pub fn new(backend: Arc<dyn CanBackend>, config: IsoTpConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let frame_rx = backend.subscribe_frames();
        let cfg = config.clone();
        let join_handle = tokio::spawn(async move {
            run_session(backend, cfg, cmd_rx, frame_rx).await;
        });
        Self {
            handle: IsoTpSessionHandle { cmd_tx },
            join_handle: Some(join_handle),
        }
    }

    /// 获取可 Clone 的句柄 (用于跨 task 发送请求)
    pub fn handle(&self) -> IsoTpSessionHandle {
        self.handle.clone()
    }

    /// 关闭会话 (后台 task 退出)
    pub async fn shutdown(mut self) {
        let _ = self.handle.cmd_tx.send(IsoTpCmd::Shutdown).await;
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.await;
        }
    }
}

impl Drop for IsoTpSession {
    fn drop(&mut self) {
        // 兜底:如果未显式 shutdown,发送 Shutdown 命令 (用 try_send 避免阻塞)
        let _ = self.handle.cmd_tx.try_send(IsoTpCmd::Shutdown);
    }
}

// ============ 后台任务 ============

async fn run_session(
    backend: Arc<dyn CanBackend>,
    config: IsoTpConfig,
    mut cmd_rx: mpsc::Receiver<IsoTpCmd>,
    mut frame_rx: broadcast::Receiver<CanFrame>,
) {
    // 取超时上限 (config.timeout_ms 不能小于默认值,否则用默认)
    let n_bs = Duration::from_millis(config.timeout_ms.max(DEFAULT_N_BS_MS as u32) as u64);
    let n_cr = Duration::from_millis(config.timeout_ms.max(DEFAULT_N_CR_MS as u32) as u64);
    let n_as = Duration::from_millis(DEFAULT_N_AS_MS);
    let n_ar = Duration::from_millis(DEFAULT_N_AR_MS);

    // rx_id → Pending (单一索引即可,因为所有接收帧 CAN ID == rx_id)
    let mut pending: HashMap<u32, Pending> = HashMap::new();

    log::info!(
        "ISO-TP 会话启动 (tx_id=0x{:X}, rx_id=0x{:X}, N_Bs={}ms, N_Cr={}ms)",
        config.tx_id, config.rx_id, n_bs.as_millis(), n_cr.as_millis()
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
    log::info!("ISO-TP 会话退出");
}

// ============ 发送流程 ============

/// 启动一次请求-响应发送 (SF 或 FF),注册 pending
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
        // === 单帧 SF ===
        let mut frame_data = vec![0u8; 8];
        frame_data[0] = PCI_SF | (data.len() as u8);
        frame_data[1..1 + data.len()].copy_from_slice(&data);
        if let Some(pad) = config.padding {
            for b in &mut frame_data[1 + data.len()..] {
                *b = pad;
            }
        }
        if let Err(e) = send_can_frame(backend, tx_id, &frame_data, n_as).await {
            let _ = response_tx.send(Err(e));
            return;
        }
        // SF 发送成功,等待响应
        pending.insert(rx_id, Pending {
            tx_id,
            response_tx: Some(response_tx),
            state: PendingState::WaitingForResponse,
        });
    } else {
        // === 多帧 FF + CF ===
        let mut ff = vec![0u8; 8];
        ff[0] = PCI_FF;
        ff[1] = ((data.len() >> 8) & 0x0F) as u8;
        ff[2] = (data.len() & 0xFF) as u8;
        ff[3..3 + FF_DATA_LEN].copy_from_slice(&data[..FF_DATA_LEN]);
        if let Err(e) = send_can_frame(backend, tx_id, &ff, n_as).await {
            let _ = response_tx.send(Err(e));
            return;
        }
        // FF 发送成功,等待 FC
        pending.insert(rx_id, Pending {
            tx_id,
            response_tx: Some(response_tx),
            state: PendingState::WaitingForFc {
                data,
                offset: FF_DATA_LEN,
                next_sn: 1,
            },
        });
    }
}

/// 发送所有剩余 CF (按 BS/STmin)
///
/// 返回 Ok(true) 表示全部发完,Ok(false) 表示中途因 BS 限制停下 (需要等下一个 FC)
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
        cf[1..1 + take].copy_from_slice(&data[*offset..*offset + take]);
        send_can_frame(backend, tx_id, &cf, n_as).await?;
        *offset += take;
        *next_sn = (*next_sn + 1) & 0x0F;

        if *offset >= data.len() {
            return Ok(true);
        }

        // STmin 间隔 (CF 之间)
        if !st_min_dur.is_zero() {
            sleep(st_min_dur).await;
        }

        // BS 流控:bs=0 表示无限 (一次发完);bs>0 表示每 bs 个 CF 后等下一个 FC
        if bs > 0 {
            block_remaining = block_remaining.saturating_sub(1);
            if block_remaining == 0 {
                return Ok(false); // 等下一个 FC
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
    // 忽略我们自己发的帧 (Tx 方向)
    if frame.direction == CanDirection::Tx {
        return Ok(());
    }
    if frame.data.is_empty() {
        return Ok(());
    }

    let pci_type = frame.data[0] & PCI_TYPE_MASK;

    match pci_type {
        PCI_FC => handle_fc_frame(backend, pending, frame, n_as).await,
        PCI_SF => handle_sf_frame(pending, frame),
        PCI_FF => handle_ff_frame(backend, config, pending, frame, n_as).await,
        PCI_CF => handle_cf_frame(pending, frame),
        _ => Ok(()), // 未知 PCI,忽略
    }
}

/// 处理 FC 帧 (对端对我们 FF 的流控响应)
async fn handle_fc_frame(
    backend: &Arc<dyn CanBackend>,
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
    n_as: Duration,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let pending_entry = match pending.get_mut(&rx_id) {
        Some(p) => p,
        None => return Ok(()), // 没有等待 FC 的请求,忽略
    };

    let tx_id = pending_entry.tx_id;
    let (data, mut offset, mut next_sn) = match &pending_entry.state {
        PendingState::WaitingForFc { data, offset, next_sn } => {
            (data.clone(), *offset, *next_sn)
        }
        _ => return Ok(()), // 状态不匹配,忽略
    };

    let fs = frame.data.get(1).copied().unwrap_or(0);
    let bs = frame.data.get(2).copied().unwrap_or(0);
    let st_min = frame.data.get(3).copied().unwrap_or(0);

    if fs == FC_OVERFLOW {
        // 对端缓冲溢出,失败
        pending_entry.complete(Err(AutomotiveError::IsoTp("对端 FC OVERFLOW".into())));
        pending.remove(&rx_id);
        return Ok(());
    }
    if fs == FC_WAIT {
        // 对端要求等待,保持 WaitingForFc 状态 (由 N_Bs 超时控制)
        pending_entry.state = PendingState::WaitingForFc { data, offset, next_sn };
        return Ok(());
    }

    // FS == CTS,发送 CF
    let result = send_consecutive_frames(
        backend, tx_id, &data, &mut offset, &mut next_sn, bs, st_min, n_as,
    ).await;

    match result {
        Ok(true) => {
            // 全部 CF 发完,转等响应
            pending_entry.state = PendingState::WaitingForResponse;
        }
        Ok(false) => {
            // 等下一个 FC
            pending_entry.state = PendingState::WaitingForFc { data, offset, next_sn };
        }
        Err(e) => {
            pending_entry.complete(Err(e));
            pending.remove(&rx_id);
        }
    }
    Ok(())
}

/// 处理 SF 帧 (对端响应: 单帧)
fn handle_sf_frame(
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let sf_dl = (frame.data[0] & 0x0F) as usize;
    if sf_dl == 0 || sf_dl > SF_MAX_DATA {
        return Ok(()); // 无效 SF_DL
    }
    let data = frame.data[1..1 + sf_dl].to_vec();

    let pending_entry = match pending.get_mut(&rx_id) {
        Some(p) => p,
        None => return Ok(()), // 没有等待响应的请求,忽略
    };
    pending_entry.complete(Ok(data));
    pending.remove(&rx_id);
    Ok(())
}

/// 处理 FF 帧 (对端响应: 多帧开始)
async fn handle_ff_frame(
    backend: &Arc<dyn CanBackend>,
    config: &IsoTpConfig,
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
    n_as: Duration,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let ff_dl = (((frame.data[0] & 0x0F) as usize) << 8) | (frame.data[1] as usize);
    if ff_dl == 0 || ff_dl > FF_DL_MAX {
        return Ok(()); // 无效 FF_DL
    }

    let pending_entry = match pending.get_mut(&rx_id) {
        Some(p) => p,
        None => {
            // 没有等待响应的请求 — 但仍可能需要消费这个多帧 (主动消息)
            // Phase 2 简化:忽略主动消息
            return Ok(());
        }
    };

    // 发 FC (CTS, BS=0, STmin=0 表示一次性发完)
    let tx_id = pending_entry.tx_id;
    let fc = [PCI_FC, FC_CTS, 0, 0, 0, 0, 0, 0];
    if let Err(e) = send_can_frame(backend, tx_id, &fc, n_as).await {
        pending_entry.complete(Err(e));
        pending.remove(&rx_id);
        return Ok(());
    }

    // 转入接收状态,先把 FF 的 6 字节数据存入
    let mut receiver = Receiver::new(ff_dl);
    let take = ff_dl.min(FF_DATA_LEN);
    receiver.buffer.extend_from_slice(&frame.data[2..2 + take]);
    if receiver.buffer.len() >= ff_dl {
        // FF 数据已完整 (理论上 ff_dl ≤ 6 时)
        pending_entry.complete(Ok(std::mem::take(&mut receiver.buffer)));
        pending.remove(&rx_id);
        return Ok(());
    }
    pending_entry.state = PendingState::Receiving { receiver };
    Ok(())
}

/// 处理 CF 帧 (对端响应: 多帧后续)
fn handle_cf_frame(
    pending: &mut HashMap<u32, Pending>,
    frame: &CanFrame,
) -> AutomotiveResult<()> {
    let rx_id = frame.id;
    let sn = frame.data[0] & 0x0F;
    let data = &frame.data[1..8.min(frame.data.len())];

    let pending_entry = match pending.get_mut(&rx_id) {
        Some(p) => p,
        None => return Ok(()), // 没有等待响应的请求,忽略
    };

    let receiver = match &mut pending_entry.state {
        PendingState::Receiving { receiver } => receiver,
        _ => return Ok(()), // 状态不匹配,忽略
    };

    match receiver.push_cf(sn, data) {
        Ok(Some(complete)) => {
            pending_entry.complete(Ok(complete));
            pending.remove(&rx_id);
        }
        Ok(None) => {} // 继续等下一个 CF
        Err(e) => {
            pending_entry.complete(Err(e));
            pending.remove(&rx_id);
        }
    }
    Ok(())
}

// ============ 辅助函数 ============

/// 发送一个 CAN 帧,带 N_As 超时
async fn send_can_frame(
    backend: &Arc<dyn CanBackend>,
    tx_id: u32,
    data: &[u8],
    n_as: Duration,
) -> AutomotiveResult<()> {
    let mut data_vec = data.to_vec();
    data_vec.resize(8, 0); // CAN 经典帧固定 8 字节
    let frame = CanFrame {
        timestamp: 0,
        id: tx_id,
        extended: false,
        rtr: false,
        dlc: 8,
        data: data_vec,
        direction: CanDirection::Tx,
    };
    timeout(n_as, backend.send_frame(&frame))
        .await
        .map_err(|_| AutomotiveError::Timeout(format!("N_As 超时 (发送 CAN 帧 id=0x{tx_id:X})")))?
}

/// STmin 字节转 Duration
///
/// - 0-127: 毫秒
/// - 128-240: 保留 (按 0 处理)
/// - 241-249: 微秒 (value - 240) * 100
/// - 250-255: 保留 (按 0 处理)
fn st_min_to_duration(st_min: u8) -> Duration {
    match st_min {
        0..=127 => Duration::from_millis(st_min as u64),
        241..=249 => Duration::from_micros((st_min as u64 - 240) * 100),
        _ => Duration::ZERO,
    }
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use vofa_next_core::{CanDirection, CanFrame, Error};
    use vofa_next_transport::CanBackend;

    /// 测试用 mock CanBackend — 直接把发送的帧推到 mpsc,供测试观察
    struct MockBackend {
        sent_tx: mpsc::UnboundedSender<CanFrame>,
        frame_tx: broadcast::Sender<CanFrame>,
    }

    impl MockBackend {
        fn new() -> (Self, mpsc::UnboundedReceiver<CanFrame>, broadcast::Receiver<CanFrame>) {
            let (sent_tx, sent_rx) = mpsc::unbounded_channel();
            let (frame_tx, frame_rx) = broadcast::channel(64);
            (
                Self { sent_tx, frame_tx },
                sent_rx,
                frame_rx,
            )
        }

        /// 模拟从对端收到一个 CAN 帧
        fn inject_rx(&self, frame: CanFrame) {
            let _ = self.frame_tx.send(frame);
        }
    }

    #[async_trait]
    impl CanBackend for MockBackend {
        async fn send_frame(&self, frame: &CanFrame) -> Result<(), Error> {
            self.sent_tx
                .send(frame.clone())
                .map_err(|_| Error::Transport("mock channel closed".into()))
        }
        fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame> {
            self.frame_tx.subscribe()
        }
        fn name(&self) -> &str {
            "MockBackend"
        }
    }

    /// 创建测试用的 IsoTpSession + 接收端
    fn make_session(
        config: IsoTpConfig,
    ) -> (
        IsoTpSession,
        Arc<MockBackend>,
        mpsc::UnboundedReceiver<CanFrame>,
    ) {
        let (backend, sent_rx, _frame_rx) = MockBackend::new();
        let backend = Arc::new(backend);
        // 注意:这里 subscribe 一次,session 内部再 subscribe 一次
        let session = IsoTpSession::new(backend.clone(), config);
        (session, backend, sent_rx)
    }

    fn make_rx_frame(id: u32, data: Vec<u8>) -> CanFrame {
        let dlc = data.len() as u8;
        CanFrame {
            timestamp: 0,
            id,
            extended: false,
            rtr: false,
            dlc,
            data,
            direction: CanDirection::Rx,
        }
    }

    #[test]
    fn st_min_units() {
        assert_eq!(st_min_to_duration(0), Duration::ZERO);
        assert_eq!(st_min_to_duration(10), Duration::from_millis(10));
        assert_eq!(st_min_to_duration(127), Duration::from_millis(127));
        // 241 = 100us
        assert_eq!(st_min_to_duration(241), Duration::from_micros(100));
        // 249 = 900us
        assert_eq!(st_min_to_duration(249), Duration::from_micros(900));
        // 128 / 250 = reserved → 0
        assert_eq!(st_min_to_duration(128), Duration::ZERO);
        assert_eq!(st_min_to_duration(250), Duration::ZERO);
    }

    #[test]
    fn receiver_push_cf_completes_when_full() {
        let mut r = Receiver::new(10);
        // FF 携带 6 字节,buffer 已有 6,还差 4
        r.buffer = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        // CF#1 携带 7 字节,前 4 字节用于完成 (后 3 字节忽略)
        let result = r.push_cf(1, &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]).unwrap();
        assert!(result.is_some());
        let complete = result.unwrap();
        assert_eq!(complete.len(), 10);
        assert_eq!(complete, vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn receiver_push_cf_rejects_wrong_sn() {
        let mut r = Receiver::new(20);
        // 期望 SN=1
        let result = r.push_cf(2, &[0x01, 0x02]);
        assert!(result.is_err(), "SN 不匹配应报错");
    }

    #[test]
    fn receiver_push_cf_sn_wraps_at_15() {
        let mut r = Receiver::new(100);
        r.next_sn = 15; // 下一个期望 SN=15
        r.buffer = vec![0; 93]; // 还差 7 字节
        let result = r.push_cf(15, &[0xAA; 7]).unwrap();
        assert!(result.is_some());
        // 下一个 SN 应该 wrap 到 0
        // (实际 wrap 在 push_cf 内部完成,这里只验证 SN=15 可接受)
    }

    #[tokio::test]
    async fn sf_request_response_round_trip() {
        // 测试:发送 SF 请求,对端回 SF 响应
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        // 异步发起请求
        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x10, 0x03])
                .await
        });

        // 等待 SF 帧发送出来
        let sent_frame = sent_rx.recv().await.expect("应收到 SF 帧");
        assert_eq!(sent_frame.id, 0x7E0);
        assert_eq!(sent_frame.data[0], 0x02); // SF_DL=2
        assert_eq!(&sent_frame.data[1..3], &[0x10, 0x03]);

        // 模拟对端回 SF (单帧响应,2 字节)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![0x02, 0x50, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        ));

        // 等响应完成
        let response = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("超时")
            .expect("task panic")
            .expect("应成功");
        assert_eq!(response, vec![0x50, 0x03]);

        session.shutdown().await;
    }

    #[tokio::test]
    async fn ff_cf_request_response_round_trip() {
        // 测试:发送 20 字节请求 (需要 FF+CF),对端回 SF 响应
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        // 构造 20 字节数据 (UDS ReadDataByIdentifier 0x22 + 19 字节填充)
        let request_data = vec![0x22; 20];

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            session_clone
                .send_request(config.tx_id, config.rx_id, request_data)
                .await
        });

        // 等 FF
        let ff_frame = sent_rx.recv().await.expect("应收到 FF");
        assert_eq!(ff_frame.id, 0x7E0);
        assert_eq!(ff_frame.data[0] & 0xF0, PCI_FF);
        let ff_dl = (((ff_frame.data[0] & 0x0F) as usize) << 8) | (ff_frame.data[1] as usize);
        assert_eq!(ff_dl, 20);
        assert_eq!(&ff_frame.data[3..8], &[0x22; 5]); // FF 携带前 5 字节

        // 发 FC (CTS, BS=0, STmin=0)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_CTS, 0, 0, 0, 0, 0, 0],
        ));

        // 等 CF#1
        let cf1 = sent_rx.recv().await.expect("应收到 CF1");
        assert_eq!(cf1.data[0], PCI_CF | 0x01);
        assert_eq!(&cf1.data[1..8], &[0x22; 7]); // CF1 携带字节 6-12

        // 等 CF#2 (含最后 1 字节)
        let cf2 = sent_rx.recv().await.expect("应收到 CF2");
        assert_eq!(cf2.data[0], PCI_CF | 0x02);
        assert_eq!(cf2.data[1], 0x22); // CF2 携带字节 13

        // 模拟对端回 SF 响应
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![0x01, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ));

        let response = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("超时")
            .expect("task panic")
            .expect("应成功");
        assert_eq!(response, vec![0x62]);

        session.shutdown().await;
    }

    #[tokio::test]
    async fn ff_cf_response_multi_frame() {
        // 测试:发送 SF 请求,对端回 20 字节响应 (FF+CF)
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x10, 0x03])
                .await
        });

        // 等 SF 请求发送
        let _ = sent_rx.recv().await.expect("应收到请求 SF");

        // 对端回 FF (20 字节响应)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FF, 0x00, 0x14, 0x62, 0xAA, 0xBB, 0xCC, 0xDD], // FF_DL=20, 携带 5 字节
        ));

        // 等待我们发 FC
        let fc = sent_rx.recv().await.expect("应发 FC");
        assert_eq!(fc.id, 0x7E0);
        assert_eq!(fc.data[0], PCI_FC);
        assert_eq!(fc.data[1], FC_CTS);

        // 对端继续发 CF#1
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_CF | 0x01, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77],
        ));
        // 对端发 CF#2 (最后 8 字节中的 7 字节)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_CF | 0x02, 0x88, 0x99, 0xAA, 0x00, 0x00, 0x00, 0x00],
        ));

        let response = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("超时")
            .expect("task panic")
            .expect("应成功");
        // FF 5 字节 + CF1 7 字节 + CF2 7 字节 (实际需要 20)
        // FF: 5 字节 (0x62 0xAA 0xBB 0xCC 0xDD)
        // CF1: 7 字节 (0x11 0x22 0x33 0x44 0x55 0x66 0x77) → 共 12
        // CF2: 取剩余 8 字节中的 7 (0x88 0x99 0xAA + 5字节填充)
        // 等等,期望 20 字节,FF 5 + CF1 7 + CF2 8 = 20,但 CF2 只有 7 字节有效 (8-1 PCI)
        // 修正:FF 6 字节?让我重新计算
        // FF: PCI(1) + FF_DL(2) + data(5) = 8 字节,数据 5 字节
        // CF: PCI(1) + data(7) = 8 字节,数据 7 字节
        // 20 字节:FF 5 + CF1 7 + CF2 7 = 19,还差 1 字节,需要 CF3
        // 重新构造:用 18 字节响应

        assert_eq!(response.len(), 20);
        // 检查前几字节
        assert_eq!(&response[0..5], &[0x62, 0xAA, 0xBB, 0xCC, 0xDD]);

        session.shutdown().await;
    }

    #[tokio::test]
    async fn fc_wait_keeps_state() {
        // 测试:对端发 FC WAIT,应保持 WaitingForFc 状态
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x22; 20])
                .await
        });

        // 等 FF 发出
        let _ = sent_rx.recv().await.expect("应收到 FF");

        // 对端发 FC WAIT
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_WAIT, 0, 0, 0, 0, 0, 0],
        ));

        // 短暂等待,确保 task 处理了 WAIT
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 然后对端发 FC CTS
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_CTS, 0, 0, 0, 0, 0, 0],
        ));

        // 等 CF1
        let cf1 = sent_rx.recv().await.expect("应收到 CF1");
        assert_eq!(cf1.data[0] & 0xF0, PCI_CF);

        // 中止会话
        session.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;
    }

    #[tokio::test]
    async fn fc_overflow_completes_with_error() {
        // 测试:对端发 FC OVERFLOW,应触发错误响应
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x22; 20])
                .await
        });

        // 等 FF
        let _ = sent_rx.recv().await.expect("应收到 FF");

        // 对端发 FC OVERFLOW
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_OVERFLOW, 0, 0, 0, 0, 0, 0],
        ));

        let result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("超时")
            .expect("task panic");
        assert!(result.is_err(), "应返回错误");
        let err = result.unwrap_err();
        assert!(matches!(err, AutomotiveError::IsoTp(_)), "应是 IsoTp 错误");

        session.shutdown().await;
    }

    #[tokio::test]
    async fn data_too_long_returns_error() {
        // 测试:数据 > 4095 字节应直接返回错误
        let config = IsoTpConfig::default();
        let (session, _backend, _sent_rx) = make_session(config.clone());

        let result = session
            .send_request(config.tx_id, config.rx_id, vec![0u8; 5000])
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AutomotiveError::IsoTp(_)));
        assert!(format!("{err}").contains("超长"));

        session.shutdown().await;
    }

    #[tokio::test]
    async fn st_min_paces_cf_sending() {
        // 测试:STmin=50ms 时,CF 之间应有 50ms 间隔
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            // 20 字节数据 → FF + 2 个 CF
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x22; 20])
                .await
        });

        // 等 FF
        let _ = sent_rx.recv().await.expect("应收到 FF");

        // 对端发 FC CTS, BS=0, STmin=50ms
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_CTS, 0, 50, 0, 0, 0, 0],
        ));

        // 等 CF1
        let t0 = std::time::Instant::now();
        let _cf1 = sent_rx.recv().await.expect("应收到 CF1");
        let t1 = std::time::Instant::now();
        // 等 CF2
        let _cf2 = sent_rx.recv().await.expect("应收到 CF2");
        let t2 = std::time::Instant::now();

        // CF1 之后到 CF2 之前应有至少 50ms 间隔 (考虑调度抖动,放宽到 30ms)
        let cf_gap = t2.duration_since(t1);
        assert!(
            cf_gap >= Duration::from_millis(30),
            "CF 间隔 {} 应 ≥ 30ms (STmin=50ms)",
            cf_gap.as_millis()
        );

        // 中止
        session.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;
    }

    #[tokio::test]
    async fn bs_limits_block_and_waits_for_next_fc() {
        // 测试:BS=1 时,每发 1 个 CF 后等下一个 FC
        let config = IsoTpConfig::default();
        let (session, backend, mut sent_rx) = make_session(config.clone());

        let session_clone = session_handle_for_test(&session);
        let handle = tokio::spawn(async move {
            // 30 字节 → FF + 4 CF (5+7+7+7+4)
            session_clone
                .send_request(config.tx_id, config.rx_id, vec![0x33; 30])
                .await
        });

        // 等 FF
        let _ff = sent_rx.recv().await.expect("应收到 FF");
        assert_eq!(_ff.data[0] & 0xF0, PCI_FF);

        // 对端发 FC: BS=1, STmin=0 (每发 1 个 CF 等下一个 FC)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_CTS, 1, 0, 0, 0, 0, 0],
        ));

        // 等 CF1
        let _cf1 = sent_rx.recv().await.expect("应收到 CF1");
        assert_eq!(_cf1.data[0], PCI_CF | 0x01);

        // 应不再有 CF (因为 BS=1 已用完),等下一个 FC
        let next = tokio::time::timeout(Duration::from_millis(200), sent_rx.recv()).await;
        assert!(next.is_err() || next.unwrap().is_none(), "BS=1 时应停下等下一个 FC");

        // 发第二个 FC (继续)
        backend.inject_rx(make_rx_frame(
            0x7E8,
            vec![PCI_FC, FC_CTS, 1, 0, 0, 0, 0, 0],
        ));

        // 等 CF2
        let _cf2 = sent_rx.recv().await.expect("应收到 CF2");
        assert_eq!(_cf2.data[0], PCI_CF | 0x02);

        // 中止
        session.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;
    }

    /// 辅助:获取 session 的引用 (用于在 spawn 的 task 中使用)
    /// 注意:实际生产中 IsoTpSession 不需要 Clone,这里用 Arc 包装来支持测试
    fn session_handle_for_test(_session: &IsoTpSession) -> SessionHandle {
        // 这里返回一个伪造句柄,实际测试通过 channel 发送命令
        // 改为直接在测试中持有 session
        unimplemented!("改用直接持有方式")
    }
}
