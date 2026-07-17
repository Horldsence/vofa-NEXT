use serialport::{self, DataBits, FlowControl, Parity, SerialPortType, StopBits};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{Error, PortInfo, Result, SerialConfig};

/// 列出所有可用串口
pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports().map_err(|e| Error::Transport(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| {
            let (port_type, vid, pid, serial_number, manufacturer, product) = match p.port_type {
                SerialPortType::UsbPort(info) => (
                    "USB".to_string(),
                    Some(info.vid),
                    Some(info.pid),
                    info.serial_number,
                    info.manufacturer,
                    info.product,
                ),
                SerialPortType::PciPort => ("PCI".to_string(), None, None, None, None, None),
                SerialPortType::BluetoothPort => {
                    ("Bluetooth".to_string(), None, None, None, None, None)
                }
                SerialPortType::Unknown => ("Unknown".to_string(), None, None, None, None, None),
            };
            PortInfo {
                name: p.port_name,
                port_type,
                vid,
                pid,
                serial_number,
                manufacturer,
                product,
            }
        })
        .collect())
}

/// 启动串口传输
///
/// 返回 (写入端, 数据广播端, 取消标志)
pub fn spawn(
    config: SerialConfig,
) -> Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let mut port = serialport::new(&config.port_name, config.baud_rate)
        .data_bits(match config.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        })
        .parity(match config.parity {
            vofa_next_core::Parity::Odd => Parity::Odd,
            vofa_next_core::Parity::Even => Parity::Even,
            vofa_next_core::Parity::None => Parity::None,
        })
        .stop_bits(match config.stop_bits {
            vofa_next_core::StopBits::Two => StopBits::Two,
            vofa_next_core::StopBits::One => StopBits::One,
        })
        .flow_control(match config.flow_control {
            vofa_next_core::FlowControl::Software => FlowControl::Software,
            vofa_next_core::FlowControl::Hardware => FlowControl::Hardware,
            vofa_next_core::FlowControl::None => FlowControl::None,
        })
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| Error::Transport(format!("打开串口失败: {}", e)))?;

    let mut write_port = port
        .try_clone()
        .map_err(|e| Error::Transport(format!("克隆串口失败: {}", e)))?;

    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读线程
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
        log::info!("串口读线程退出");
    });

    // 写线程
    let cancel_write = cancel.clone();
    std::thread::spawn(move || {
        while !cancel_write.load(Ordering::Relaxed) {
            match write_rx.blocking_recv() {
                Some(data) => {
                    if let Err(e) = write_port.write_all(&data) {
                        log::error!("串口写入失败: {}", e);
                        break;
                    }
                }
                None => break,
            }
        }
        log::info!("串口写线程退出");
    });

    Ok((write_tx, data_tx, cancel))
}
