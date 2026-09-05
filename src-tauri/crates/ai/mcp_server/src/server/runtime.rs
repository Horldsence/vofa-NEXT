//! MCP HTTP 服务生命周期 — axum serve + 优雅关闭句柄

use std::sync::Arc;

use error::McpError;
use tauri::AppHandle;
use vofa_core::Result as VofaResult;

use super::handlers::VofaMcpServer;
use super::toolbox::{Toolbox, MCP_ENDPOINT_PATH};

/// 正在运行的 MCP server 句柄 — 显式 [`McpServerHandle::stop`] 触发优雅关闭。
pub struct McpServerHandle {
    /// 实际监听端口。
    pub port: u16,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    done_rx: tokio::sync::oneshot::Receiver<std::io::Result<()>>,
}

impl McpServerHandle {
    /// 优雅停止 (幂等)。
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// 非阻塞检查 server 是否在运行 (内部任务出错则返回错误)。
    ///
    /// # Errors
    /// axum serve 任务以错误退出时返回 [`McpError::ServerStart`]。
    pub fn check_running(&mut self) -> VofaResult<bool> {
        match self.done_rx.try_recv() {
            Ok(Ok(())) => Ok(false),
            Ok(Err(source)) => Err(McpError::ServerStart {
                port: self.port,
                source: Box::new(source),
            }
            .into()),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => Ok(true),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Ok(false),
        }
    }
}

/// 在 `127.0.0.1:{port}` 启动 MCP streamable-http server。
///
/// # Errors
/// 端口占用等 bind 失败返回 [`McpError::ServerStart`]。
pub async fn start(toolbox: Toolbox, app: AppHandle, port: u16) -> VofaResult<McpServerHandle> {
    let service_factory = move || Ok(VofaMcpServer::new(toolbox.clone(), app.clone()));
    let session_manager = Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
    );
    let service = rmcp::transport::StreamableHttpService::new(
        service_factory,
        session_manager,
        rmcp::transport::StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new().route_service(MCP_ENDPOINT_PATH, service);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|source| McpError::ServerStart {
            port,
            source: Box::new(source),
        })?;
    let actual_port = listener.local_addr().ok().map_or(port, |a| a.port());

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<std::io::Result<()>>();
    tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        let _ = done_tx.send(server.await);
    });

    log::info!("MCP server 已启动: http://127.0.0.1:{actual_port}{MCP_ENDPOINT_PATH}");
    Ok(McpServerHandle {
        port: actual_port,
        shutdown_tx: Some(shutdown_tx),
        done_rx,
    })
}
