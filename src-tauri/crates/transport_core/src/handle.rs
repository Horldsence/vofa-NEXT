use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch, Notify};
use schema_types::TestDataLink;
use vofa_core::{ConnectionState, Result, TransportConfig, TransportStats};
use error::TransportError;

/// 单连接句柄 — 一个传输节点实例的全部运行时状态
///
/// 持有写入通道 / 数据广播 / 取消标志 / 状态 / 统计 / 配置,
/// TestData 传输额外持有运行开关、恢复通知与协议热更新通道。
pub struct TransportHandle {
    write_tx: mpsc::Sender<Vec<u8>>,
    data_tx: broadcast::Sender<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    state: parking_lot::Mutex<ConnectionState>,
    stats: parking_lot::Mutex<TransportStats>,
    /// 测试数据生成器运行状态 (仅 TestData 有效)
    test_data_running: Option<Arc<AtomicBool>>,
    /// 测试数据生成器恢复通知 (仅 TestData 有效)
    test_data_notify: Option<Arc<Notify>>,
    /// 测试数据链路配置热更新通道 (仅 TestData 有效)
    test_data_link: Option<watch::Sender<TestDataLink>>,
    /// 本连接的配置 — 供外部查询 (如 CAN 波特率)
    config: TransportConfig,
}

impl TransportHandle {
    pub(crate) fn new(
        write_tx: mpsc::Sender<Vec<u8>>,
        data_tx: broadcast::Sender<Vec<u8>>,
        cancel: Arc<AtomicBool>,
        test_data_running: Option<Arc<AtomicBool>>,
        test_data_notify: Option<Arc<Notify>>,
        test_data_link: Option<watch::Sender<TestDataLink>>,
        config: TransportConfig,
    ) -> Self {
        Self {
            write_tx,
            data_tx,
            cancel,
            state: parking_lot::Mutex::new(ConnectionState::Connected),
            stats: parking_lot::Mutex::new(TransportStats::default()),
            test_data_running,
            test_data_notify,
            test_data_link,
            config,
        }
    }

    /// 发送数据 (try_send, 队列满时立即报错) 并更新 tx 统计
    ///
    /// 统一回环: 写入任一 Transport 的字节同时发布到本节点的接收广播,
    /// 使 transport→transport 路由链在写入点不断裂 (发送内容对下游
    /// Protocol/RawData 等订阅者可见)。无订阅者时广播失败静默忽略。
    pub fn send(&self, data: &[u8]) -> Result<()> {
        self.write_tx
            .try_send(data.to_vec())
            .map_err(|_| TransportError::Send(std::io::Error::other("channel closed")))?;
        let _ = self.data_tx.send(data.to_vec());
        {
            let mut stats = self.stats.lock();
            stats.tx_bytes += data.len() as u64;
            stats.tx_frames += 1;
        }
        Ok(())
    }

    /// 订阅接收数据
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.data_tx.subscribe()
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

    /// 本连接的配置 — 供外部查询 CAN 波特率等
    pub const fn config(&self) -> &TransportConfig {
        &self.config
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
            .is_some_and(|r| r.load(Ordering::Relaxed))
    }

    /// 运行时热更新链路配置 (图/协议变化后调用, 无需重连)
    ///
    /// 当前仅 TestData 生成器消费链路配置; 其他传输类型的字节收发与协议无关,
    /// 静默接受 (返回 false 表示未应用)。
    pub fn update_link(&self, link: TestDataLink) -> Result<bool> {
        if let Some(tx) = &self.test_data_link {
            tx.send(link)
                .map_err(|_| TransportError::LinkUpdate(std::io::Error::other("channel closed")))?;
            return Ok(true);
        }
        Ok(false)
    }

    /// 关闭连接: 通知后台任务退出并将状态置为 Disconnected
    pub fn close(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        *self.state.lock() = ConnectionState::Disconnected;
    }
}

impl Drop for TransportHandle {
    fn drop(&mut self) {
        // 句柄被移除 (close / 重复 open 替换) 时确保后台任务收到取消信号
        self.cancel.store(true, Ordering::Relaxed);
    }
}