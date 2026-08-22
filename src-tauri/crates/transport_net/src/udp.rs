use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use vofa_core::{Result, UdpConfig};
use error::TransportError;

/// 启动 UDP 传输
pub async fn spawn(
    config: UdpConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let local_addr = format!("{}:{}", config.local_addr, config.local_port);
    let socket = UdpSocket::bind(&local_addr).await.map_err(|e| TransportError::UdpBind {
        addr: local_addr.clone(),
        source: e,
    })?;

    let remote = format!("{}:{}", config.remote_addr, config.remote_port);
    socket.connect(&remote).await.map_err(|e| TransportError::UdpConnect {
        addr: remote.clone(),
        source: e,
    })?;

    // UdpSocket 的 send/recv 接受 &self, 用 Arc 共享
    let socket = Arc::new(socket);

    let (data_tx, _) = broadcast::channel(256);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读任务
    let data_tx_read = data_tx.clone();
    let cancel_read = cancel.clone();
    let socket_read = socket.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536].into_boxed_slice();
        loop {
            tokio::select! {
                result = socket_read.recv(&mut buf[..]) => {
                    match result {
                        Ok(n) => { let _ = data_tx_read.send(buf[..n].to_vec()); }
                        Err(e) => {
                            log::error!("UDP 接收错误: {e}");
                            break;
                        }
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_read.load(Ordering::Relaxed) { break; }
                }
            }
        }
        log::debug!("UDP 读任务退出");
    });

    // 写任务
    let cancel_write = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                data = write_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = socket.send(&data).await {
                                log::error!("UDP 发送错误: {e}");
                                break;
                            }
                        }
                        None => break,
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_write.load(Ordering::Relaxed) { break; }
                }
            }
        }
        log::debug!("UDP 写任务退出");
    });

    Ok((write_tx, data_tx, cancel))
}