use serial_core::{Error, Result, UdpConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};

/// 启动 UDP 传输
pub async fn spawn(
    config: UdpConfig,
) -> Result<(mpsc::Sender<Vec<u8>>, broadcast::Sender<Vec<u8>>, Arc<AtomicBool>)> {
    let local_addr = format!("{}:{}", config.local_addr, config.local_port);
    let socket = UdpSocket::bind(&local_addr)
        .await
        .map_err(|e| Error::Transport(format!("UDP 绑定失败: {}", e)))?;

    let remote = format!("{}:{}", config.remote_addr, config.remote_port);
    socket
        .connect(&remote)
        .await
        .map_err(|e| Error::Transport(format!("UDP 连接失败: {}", e)))?;

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
        let mut buf = [0u8; 2048];
        loop {
            tokio::select! {
                result = socket_read.recv(&mut buf) => {
                    match result {
                        Ok(n) => { let _ = data_tx_read.send(buf[..n].to_vec()); }
                        Err(e) => {
                            tracing::error!("UDP 接收错误: {}", e);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_read.load(Ordering::Relaxed) { break; }
                }
            }
        }
        tracing::info!("UDP 读任务退出");
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
                                tracing::error!("UDP 发送错误: {}", e);
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_write.load(Ordering::Relaxed) { break; }
                }
            }
        }
        tracing::info!("UDP 写任务退出");
    });

    Ok((write_tx, data_tx, cancel))
}
