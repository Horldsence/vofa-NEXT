//! ISO-TP 会话公开 API — `IsoTpSession` / `IsoTpSessionHandle`

use diagnostic::IsoTpConfig;
use std::sync::Arc;
use tokio::sync::mpsc;
use transport_core::CanBackend;

use crate::error::AutomotiveError;
use crate::state::IsoTpCmd;
use crate::task::run_session;

/// ISO-TP 会话句柄 (Clone-able,廉价)
#[derive(Clone)]
pub struct IsoTpSessionHandle {
    pub(crate) cmd_tx: mpsc::Sender<IsoTpCmd>,
}

impl IsoTpSessionHandle {
    /// 发送请求并等待响应
    ///
    /// `tx_id`: 请求使用的 CAN ID
    /// `rx_id`: 期望响应的 CAN ID
    /// `data`: 请求负载
    pub async fn send_request(
        &self,
        tx_id: u32,
        rx_id: u32,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, AutomotiveError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(IsoTpCmd::SendRequest {
                tx_id,
                rx_id,
                data,
                response_tx: resp_tx,
            })
            .await
            .map_err(|_| AutomotiveError::IsoTpSessionClosed)?;
        resp_rx
            .await
            .map_err(|_| AutomotiveError::IsoTpTaskCrashed)?
    }
}

/// ISO-TP 会话 — 后台任务 + 命令通道
pub struct IsoTpSession {
    handle: IsoTpSessionHandle,
    join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl IsoTpSession {
    /// 创建新会话并 spawn 后台任务
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

    /// 获取会话句柄 (可廉价克隆)
    pub fn handle(&self) -> IsoTpSessionHandle {
        self.handle.clone()
    }

    /// 显式关闭会话并等待后台任务退出
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
