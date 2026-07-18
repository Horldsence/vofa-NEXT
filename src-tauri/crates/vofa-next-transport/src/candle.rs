use nusb::transfer::{Bulk, In, Out};
use nusb::MaybeFuture;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{CandleConfig, CandleDeviceInfo, Error, Result};

/// candleLight USB VID/PID
const CANDLE_VID: u16 = 0x1209;
const CANDLE_PID: u16 = 0x2323;

/// 获取设备的 (bus, address) — busnum 仅 Linux 可用, 其他平台返回 0
#[cfg(target_os = "linux")]
fn dev_bus_address(dev: &nusb::DeviceInfo) -> (u8, u8) {
    (dev.busnum(), dev.device_address())
}

#[cfg(not(target_os = "linux"))]
fn dev_bus_address(dev: &nusb::DeviceInfo) -> (u8, u8) {
    (0, dev.device_address())
}

/// 列出所有 candleLight 设备
pub fn list_devices() -> Result<Vec<CandleDeviceInfo>> {
    let devices = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Transport(format!("列举 USB 设备失败: {}", e)))?;

    let mut result = Vec::new();
    for dev in devices {
        if dev.vendor_id() == CANDLE_VID && dev.product_id() == CANDLE_PID {
            let (bus, address) = dev_bus_address(&dev);
            result.push(CandleDeviceInfo {
                bus,
                address,
                vid: dev.vendor_id(),
                pid: dev.product_id(),
                manufacturer: dev.manufacturer_string().map(String::from),
                product: dev.product_string().map(String::from),
                serial_number: dev.serial_number().map(String::from),
            });
        }
    }
    Ok(result)
}

/// 启动 candleLight 传输
///
/// 用 nusb 进行原生 USB 通信。读/写任务通过 bulk 端点收发数据。
/// candleLight 帧格式的完整解析在协议层完成, 传输层只透传字节。
pub async fn spawn(
    config: CandleConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    // 列举设备并按 bus/address 找到目标
    // (Linux 上 bus 字段有效; 其他平台 bus 字段被忽略, 仅按 address 匹配)
    let device_info = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Transport(format!("列举 USB 设备失败: {}", e)))?
        .find(|d| {
            if d.vendor_id() != CANDLE_VID || d.product_id() != CANDLE_PID {
                return false;
            }
            let (bus, address) = dev_bus_address(d);
            address == config.address && bus == config.bus
        })
        .ok_or_else(|| {
            Error::Transport(format!(
                "未找到 candleLight 设备: bus={}, address={}",
                config.bus, config.address
            ))
        })?;

    let device = device_info
        .open()
        .wait()
        .map_err(|e| Error::Transport(format!("打开 candleLight 设备失败: {}", e)))?;

    let interface = device
        .claim_interface(0)
        .wait()
        .map_err(|e| Error::Transport(format!("claim interface 失败: {}", e)))?;

    // 打开 bulk 端点 (candleLight: EP1 IN=0x81, EP2 OUT=0x02)
    let mut ep_out = interface
        .endpoint::<Bulk, Out>(0x02)
        .map_err(|e| Error::Transport(format!("打开 OUT 端点失败: {}", e)))?;
    let mut ep_in = interface
        .endpoint::<Bulk, In>(0x81)
        .map_err(|e| Error::Transport(format!("打开 IN 端点失败: {}", e)))?;

    // 发送 candleLight 设置波特率命令 (16 字节命令包)
    let bitrate_cmd = build_set_bitrate_cmd(config.can_bitrate.bps());
    ep_out.submit(bitrate_cmd.into());
    let _ = ep_out.next_complete().await;

    // IN 端点最大包大小, 一次接收多个包以提高吞吐
    let in_max_packet = ep_in.max_packet_size().max(64);
    let buf_size = in_max_packet * 8;

    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读任务 — 持续提交 IN 请求并广播接收到的字节
    let data_tx_read = data_tx.clone();
    let cancel_read = cancel.clone();
    tokio::spawn(async move {
        // 初始提交读请求
        ep_in.submit(nusb::transfer::Buffer::new(buf_size));
        loop {
            tokio::select! {
                completion = ep_in.next_complete() => {
                    match completion.into_result() {
                        Ok(buf) => {
                            let data: Vec<u8> = buf[..].to_vec();
                            if !data.is_empty() {
                                let _ = data_tx_read.send(data);
                            }
                            // 重新提交读请求
                            ep_in.submit(nusb::transfer::Buffer::new(buf_size));
                        }
                        Err(e) => {
                            log::error!("candleLight 接收错误: {}", e);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_read.load(Ordering::Relaxed) { break; }
                }
            }
        }
        log::info!("candleLight 读任务退出");
    });

    // 写任务
    let cancel_write = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                data = write_rx.recv() => {
                    match data {
                        Some(data) => {
                            // TODO: 后续阶段包装为 candleLight 帧格式
                            ep_out.submit(data.into());
                            if let Err(e) = ep_out.next_complete().await.into_result() {
                                log::error!("candleLight 发送错误: {}", e);
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
        log::info!("candleLight 写任务退出");
    });

    Ok((write_tx, data_tx, cancel))
}

/// 构建 candleLight 设置波特率命令
/// candleLight 协议: 16 字节命令包
fn build_set_bitrate_cmd(bitrate: u32) -> Vec<u8> {
    let mut cmd = vec![0u8; 16];
    cmd[0] = 0x01; // SLCAN_CMD_SET_BITRATE
    cmd[4..8].copy_from_slice(&bitrate.to_le_bytes());
    cmd
}
