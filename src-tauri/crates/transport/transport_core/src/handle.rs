use error::TransportError;
use schema_types::TestDataLink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch, Notify};
use vofa_core::{ConnectionState, Result, TestDataConfig, TransportConfig, TransportStats};

use crate::test_data::TestDataRuntime;

/// 单连接句柄 — 一个传输节点实例的全部运行时状态
///
/// 持有写入通道 / 数据广播 / 取消标志 / 状态 / 统计 / 配置,
/// TestData 传输额外持有运行开关、恢复通知与协议热更新通道。
pub struct TransportHandle {
    write_tx: mpsc::Sender<Vec<u8>>,
    data_tx: broadcast::Sender<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    state: parking_lot::Mutex<ConnectionState>,
    stats: Arc<parking_lot::Mutex<TransportStats>>,
    /// 测试数据生成器运行状态 (仅 TestData 有效)
    test_data_running: Option<Arc<AtomicBool>>,
    /// 测试数据生成器恢复通知 (仅 TestData 有效)
    test_data_notify: Option<Arc<Notify>>,
    /// 测试数据链路配置热更新通道 (仅 TestData 有效)
    test_data_runtime: Option<watch::Sender<TestDataRuntime>>,
    /// 本连接当前实际使用的配置。TestData 热更新采样率时同步更新，供数据平面
    /// 恢复逐样本时间戳，不能只保留打开连接时的旧值。
    /// Arc 包装以便 [`LiveNodeHandle`] 免 manager 全局锁共享读取。
    config: Arc<parking_lot::RwLock<TransportConfig>>,
}

/// 读任务持有的轻量节点句柄 — 拿一次后免 manager 全局锁做每批查询
///
/// 数据平面读任务每批要查运行态配置 (采样时钟)、TestData 开关并上报 rx 统计;
/// 逐批经 manager 全局锁查询会与 open/close/其他传输串行化。此句柄与
/// [`TransportHandle`] 内部字段共享同一 Arc 存储, 克隆廉价; 生命周期上读任务
/// 随连接关闭一同结束 (detach → abort), 不存在句柄悬垂于已重建连接的问题。
pub struct LiveNodeHandle {
    node_id: Arc<str>,
    config: Arc<parking_lot::RwLock<TransportConfig>>,
    stats: Arc<parking_lot::Mutex<TransportStats>>,
    test_data_running: Option<Arc<AtomicBool>>,
}

impl LiveNodeHandle {
    /// 本节点运行态配置 — 仅当查询节点与绑定节点一致时返回。
    /// (协议转换链递归时 source 会变成 Protocol 节点 id, 此时无运行态配置,
    /// 与 `TransportManager::config` 对非传输节点返回 None 的语义一致)
    pub fn config_of(&self, node_id: &str) -> Option<TransportConfig> {
        if node_id == &*self.node_id {
            Some(self.config.read().clone())
        } else {
            None
        }
    }

    /// 更新接收统计 (由消费侧在数据被处理完成后上报)
    pub fn record_rx(&self, bytes: usize, frames: u64) {
        let mut stats = self.stats.lock();
        stats.rx_bytes += bytes as u64;
        stats.rx_frames += frames;
    }

    /// 测试数据生成器运行状态 (三分: None = 非 TestData 节点)
    pub fn test_data_running_state(&self) -> Option<bool> {
        self.test_data_running
            .as_ref()
            .map(|r| r.load(Ordering::Relaxed))
    }
}

impl TransportHandle {
    pub(crate) fn new(
        write_tx: mpsc::Sender<Vec<u8>>,
        data_tx: broadcast::Sender<Vec<u8>>,
        cancel: Arc<AtomicBool>,
        test_data_running: Option<Arc<AtomicBool>>,
        test_data_notify: Option<Arc<Notify>>,
        test_data_runtime: Option<watch::Sender<TestDataRuntime>>,
        config: TransportConfig,
    ) -> Self {
        Self {
            write_tx,
            data_tx,
            cancel,
            state: parking_lot::Mutex::new(ConnectionState::Connected),
            stats: Arc::new(parking_lot::Mutex::new(TransportStats::default())),
            test_data_running,
            test_data_notify,
            test_data_runtime,
            config: Arc::new(parking_lot::RwLock::new(config)),
        }
    }

    /// 取轻量句柄 (读任务启动时调用一次; node_id 由 manager 的表键提供)
    pub fn live(&self, node_id: &str) -> LiveNodeHandle {
        LiveNodeHandle {
            node_id: Arc::from(node_id),
            config: Arc::clone(&self.config),
            stats: Arc::clone(&self.stats),
            test_data_running: self.test_data_running.clone(),
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
    pub fn config(&self) -> TransportConfig {
        self.config.read().clone()
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

    /// 测试数据生成器运行状态 (三分: None = 非 TestData 节点)
    ///
    /// 数据平面读任务据此在"生成停止"边沿排空广播积压,
    /// 避免停止后波形继续滚动。
    pub fn test_data_running_state(&self) -> Option<bool> {
        self.test_data_running
            .as_ref()
            .map(|r| r.load(Ordering::Relaxed))
    }

    /// 运行时热更新链路配置 (图/协议变化后调用, 无需重连)
    ///
    /// 当前仅 TestData 生成器消费链路配置; 其他传输类型的字节收发与协议无关,
    /// 静默接受 (返回 false 表示未应用)。
    pub fn update_link(&self, link: TestDataLink, config: Option<TestDataConfig>) -> Result<bool> {
        if let Some(tx) = &self.test_data_runtime {
            let config = config.unwrap_or_else(|| tx.borrow().config.clone());
            tx.send(TestDataRuntime {
                config: config.clone(),
                link,
            })
            .map_err(|_| TransportError::LinkUpdate(std::io::Error::other("channel closed")))?;
            *self.config.write() = TransportConfig::TestData(config);
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
