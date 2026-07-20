use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{Error, Result, TcpClientConfig, TcpServerConfig};

/// 启动 TCP 客户端
pub async fn spawn_client(
    config: TcpClientConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let addr = format!("{}:{}", config.host, config.port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| Error::Transport(format!("TCP 连接失败: {}", e)))?;

    let (read_half, write_half) = stream.into_split();

    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读任务
    let data_tx_read = data_tx.clone();
    let cancel_read = cancel.clone();
    tokio::spawn(async move {
        let mut read_half = read_half;
        let mut buf = [0u8; 2048];
        loop {
            tokio::select! {
                result = read_half.read(&mut buf) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => { let _ = data_tx_read.send(buf[..n].to_vec()); }
                        Err(e) => {
                            log::error!("TCP 接收错误: {}", e);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if cancel_read.load(Ordering::Relaxed) { break; }
                }
            }
        }
        log::info!("TCP 客户端读任务退出");
    });

    // 写任务
    let cancel_write = cancel.clone();
    tokio::spawn(async move {
        let mut write_half = write_half;
        loop {
            tokio::select! {
                data = write_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = write_half.write_all(&data).await {
                                log::error!("TCP 发送错误: {}", e);
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if cancel_write.load(Ordering::Relaxed) { break; }
                }
            }
        }
        log::info!("TCP 客户端写任务退出");
    });

    Ok((write_tx, data_tx, cancel))
}

/// 启动 TCP 服务端 (接受第一个连接)
pub async fn spawn_server(
    config: TcpServerConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let addr = format!("{}:{}", config.listen_addr, config.listen_port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| Error::Transport(format!("TCP 监听失败: {}", e)))?;

    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 接受连接 + 读写任务
    let data_tx_accept = data_tx.clone();
    let cancel_accept = cancel.clone();
    tokio::spawn(async move {
        log::info!("TCP 服务端等待连接: {}", addr);

        // 等待连接 (带取消)
        let stream = loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            log::info!("TCP 客户端已连接: {}", addr);
                            break stream;
                        }
                        Err(e) => {
                            log::error!("TCP 接受连接失败: {}", e);
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if cancel_accept.load(Ordering::Relaxed) { return; }
                }
            }
        };

        let (mut read_half, mut write_half) = stream.into_split();
        let data_tx_read = data_tx_accept.clone();
        let cancel_read = cancel_accept.clone();

        // 读任务
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                tokio::select! {
                    result = read_half.read(&mut buf) => {
                        match result {
                            Ok(0) => break,
                            Ok(n) => { let _ = data_tx_read.send(buf[..n].to_vec()); }
                            Err(e) => {
                                log::error!("TCP 服务端接收错误: {}", e);
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if cancel_read.load(Ordering::Relaxed) { break; }
                    }
                }
            }
        });

        // 写任务
        loop {
            tokio::select! {
                data = write_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = write_half.write_all(&data).await {
                                log::error!("TCP 服务端发送错误: {}", e);
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if cancel_accept.load(Ordering::Relaxed) { break; }
                }
            }
        }
    });

    Ok((write_tx, data_tx, cancel))
}
