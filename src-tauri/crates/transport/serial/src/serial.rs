use error::TransportError;
use serialport::{DataBits, FlowControl, Parity, SerialPortType, StopBits};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use vofa_core::{PortInfo, Result, SerialConfig};

#[cfg(windows)]
use crate::windows_ports::port_descriptions;

#[cfg(not(windows))]
fn port_descriptions() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
}

/// 列出所有可用串口
pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports =
        serialport::available_ports().map_err(|e| TransportError::SerialEnumeration(e.into()))?;
    let descriptions = port_descriptions();
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
            let description = descriptions.get(&p.port_name).cloned();
            PortInfo {
                name: p.port_name,
                port_type,
                vid,
                pid,
                serial_number,
                manufacturer,
                product,
                description,
            }
        })
        .collect())
}

/// 启动串口传输
///
/// 返回 (写入端, 数据广播端, 取消标志)
#[allow(clippy::type_complexity)]
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
            vofa_core::Parity::Odd => Parity::Odd,
            vofa_core::Parity::Even => Parity::Even,
            vofa_core::Parity::None => Parity::None,
        })
        .stop_bits(match config.stop_bits {
            vofa_core::StopBits::Two => StopBits::Two,
            vofa_core::StopBits::One => StopBits::One,
        })
        .flow_control(match config.flow_control {
            vofa_core::FlowControl::Software => FlowControl::Software,
            vofa_core::FlowControl::Hardware => FlowControl::Hardware,
            vofa_core::FlowControl::None => FlowControl::None,
        })
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| TransportError::SerialOpen {
            port: config.port_name.clone(),
            source: e.into(),
        })?;

    let mut write_port = port
        .try_clone()
        .map_err(|e| TransportError::SerialClone(e.into()))?;

    let (data_tx, _) = broadcast::channel(256);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    // 读线程
    let data_tx_read = data_tx.clone();
    let cancel_read = cancel.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536].into_boxed_slice();
        while !cancel_read.load(Ordering::Relaxed) {
            match port.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = data_tx_read.send(buf[..n].to_vec());
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
        log::debug!("串口读线程退出");
    });

    // 写线程
    let cancel_write = cancel.clone();
    std::thread::spawn(move || {
        while !cancel_write.load(Ordering::Relaxed) {
            match write_rx.blocking_recv() {
                Some(data) => {
                    if let Err(e) = write_port.write_all(&data) {
                        log::error!("串口写入失败: {e}");
                        break;
                    }
                }
                None => break,
            }
        }
        log::debug!("串口写线程退出");
    });

    Ok((write_tx, data_tx, cancel))
}
