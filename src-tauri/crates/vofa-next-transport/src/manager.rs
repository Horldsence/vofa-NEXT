use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Notify};
use vofa_next_core::{
    ConnectionState, Error, PortInfo, ProtocolConfig, Result, TransportConfig, TransportStats,
};

/// 传输管理器 — 统一管理所有传输类型
pub struct TransportManager {
    write_tx: Option<mpsc::Sender<Vec<u8>>>,
    data_tx: Option<broadcast::Sender<Vec<u8>>>,
    cancel: Option<Arc<AtomicBool>>,
    state: parking_lot::Mutex<ConnectionState>,
    stats: parking_lot::Mutex<TransportStats>,
    /// 测试数据生成器运行状态 (仅 TestData 有效)
    test_data_running: Option<Arc<AtomicBool>>,
    /// 测试数据生成器恢复通知 (仅 TestData 有效)
    test_data_notify: Option<Arc<Notify>>,
}

impl TransportManager {
    pub fn new() -> Self {
        Self {
            write_tx: None,
            data_tx: None,
            cancel: None,
            state: parking_lot::Mutex::new(ConnectionState::Disconnected),
            stats: parking_lot::Mutex::new(TransportStats::default()),
            test_data_running: None,
            test_data_notify: None,
        }
    }

    /// 列出所有可用串口
    pub fn list_ports() -> Result<Vec<PortInfo>> {
        crate::serial::list_ports()
    }

    /// 打开连接
    ///
    /// `protocol` 仅被 TestData 用作生成数据的线缆格式参考, 其他传输类型忽略此参数
    pub async fn open(
        &mut self,
        config: TransportConfig,
        protocol: ProtocolConfig,
    ) -> Result<()> {
        self.close().await;

        self.set_state(ConnectionState::Connecting);

        let (write_tx, data_tx, cancel, test_data_running, test_data_notify) = match &config {
            TransportConfig::Serial(c) => {
                let (w, d, c) = crate::serial::spawn(c.clone())?;
                (w, d, c, None, None)
            }
            TransportConfig::Udp(c) => {
                let (w, d, c) = crate::udp::spawn(c.clone()).await?;
                (w, d, c, None, None)
            }
            TransportConfig::TcpClient(c) => {
                let (w, d, c) = crate::tcp::spawn_client(c.clone()).await?;
                (w, d, c, None, None)
            }
            TransportConfig::TcpServer(c) => {
                let (w, d, c) = crate::tcp::spawn_server(c.clone()).await?;
                (w, d, c, None, None)
            }
            TransportConfig::TestData(c) => {
                let (w, d, c, r, n) = crate::test_data::spawn(c.clone(), protocol).await?;
                (w, d, c, Some(r), Some(n))
            }
            TransportConfig::Slcan(c) => {
                let (w, d, c) = crate::slcan::spawn(c.clone())?;
                (w, d, c, None, None)
            }
            TransportConfig::CandleLight(c) => {
                let (w, d, c) = crate::candle::spawn(c.clone()).await?;
                (w, d, c, None, None)
            }
        };

        self.write_tx = Some(write_tx);
        self.data_tx = Some(data_tx);
        self.cancel = Some(cancel);
        self.test_data_running = test_data_running;
        self.test_data_notify = test_data_notify;
        self.set_state(ConnectionState::Connected);

        log::info!("连接已建立: {:?}", config);
        Ok(())
    }

    /// 关闭连接
    pub async fn close(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.write_tx = None;
        self.data_tx = None;
        self.test_data_running = None;
        self.test_data_notify = None;
        self.set_state(ConnectionState::Disconnected);
    }

    /// 设置测试数据生成器运行状态 (仅 TestData 有效)
    pub fn set_test_data_running(&self, running: bool) {
        if let Some(r) = &self.test_data_running {
            r.store(running, Ordering::Relaxed);
        }
        if running {
            if let Some(n) = &self.test_data_notify {
                n.notify_one();
            }
        }
    }

    /// 获取测试数据生成器当前运行状态
    pub fn is_test_data_running(&self) -> bool {
        self.test_data_running
            .as_ref()
            .map(|r| r.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// 发送数据
    pub async fn send(&self, data: &[u8]) -> Result<()> {
        if let Some(tx) = &self.write_tx {
            tx.try_send(data.to_vec())
                .map_err(|e| Error::Transport(format!("发送失败: {}", e)))?;
            let mut stats = self.stats.lock();
            stats.tx_bytes += data.len() as u64;
            stats.tx_frames += 1;
            Ok(())
        } else {
            Err(Error::PortNotOpen("无活动连接".into()))
        }
    }

    /// 订阅接收数据
    pub fn subscribe(&self) -> Option<broadcast::Receiver<Vec<u8>>> {
        self.data_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// 获取连接状态
    pub fn state(&self) -> ConnectionState {
        *self.state.lock()
    }

    /// 获取统计信息
    pub fn stats(&self) -> TransportStats {
        self.stats.lock().clone()
    }

    /// 更新接收统计 (由外部调用, 当数据被消费时)
    pub fn record_rx(&self, bytes: usize, frames: u64) {
        let mut stats = self.stats.lock();
        stats.rx_bytes += bytes as u64;
        stats.rx_frames += frames;
    }

    fn set_state(&self, state: ConnectionState) {
        *self.state.lock() = state;
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}
