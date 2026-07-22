use serialport::{self, DataBits, FlowControl, Parity, StopBits};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{Error, Result, SlcanConfig};

/// 启动 slcan 传输
///
/// 内部用 serialport 打开串口, 启动时发送 slcan 初始化命令。
/// 读线程广播原始字节 (包含 slcan ASCII 命令), 由 SlcanEngine 解析。
#[allow(clippy::type_complexity)]
pub fn spawn(
    config: SlcanConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let mut port = serialport::new(&config.port_name, config.baud_rate)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| Error::Transport(format!("打开 slcan 串口失败: {}", e)))?;

    // 发送 slcan 初始化命令: 设置波特率 + 打开 CAN
    let bitrate_cmd = format!("{}\r", config.can_bitrate.slcan_cmd());
    port.write_all(bitrate_cmd.as_bytes())
        .map_err(|e| Error::Transport(format!("设置 CAN 波特率失败: {}", e)))?;
    std::thread::sleep(Duration::from_millis(50));
    port.write_all(b"O\r")
        .map_err(|e| Error::Transport(format!("打开 CAN 失败: {}", e)))?;
    std::thread::sleep(Duration::from_millis(50));

    let mut write_port = port
        .try_clone()
        .map_err(|e| Error::Transport(format!("克隆 slcan 串口失败: {}", e)))?;

    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读线程 — 透传原始字节给协议层解析
    let data_tx_read = data_tx.clone();
    let cancel_read = cancel.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        while !cancel_read.load(Ordering::Relaxed) {
            match port.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = data_tx_read.send(buf[..n].to_vec());
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            }
        }
        // 关闭 CAN
        let _ = port.write_all(b"C\r");
        log::info!("slcan 读线程退出");
    });

    // 写线程
    let cancel_write = cancel.clone();
    std::thread::spawn(move || {
        while !cancel_write.load(Ordering::Relaxed) {
            match write_rx.blocking_recv() {
                Some(data) => {
                    if let Err(e) = write_port.write_all(&data) {
                        log::error!("slcan 写入失败: {}", e);
                        break;
                    }
                }
                None => break,
            }
        }
        log::info!("slcan 写线程退出");
    });

    Ok((write_tx, data_tx, cancel))
}
