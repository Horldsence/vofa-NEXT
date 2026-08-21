use std::collections::HashMap;
use tokio::sync::broadcast;
use schema_types::TestDataLink;
use vofa_core::{ConnectionState, Error, PortInfo, Result, TransportConfig, TransportStats};

use crate::handle::TransportHandle;

/// 传输管理器 — 按节点 ID 的多实例注册表
///
/// 节点图中可同时存在多个传输节点 (串口/TCP/UDP/TestData…),
/// 每个节点一个连接实例, 独立收发。同一 node_id 重复 open 会先关闭旧连接,
/// 不影响其他节点。
pub struct TransportManager {
    handles: HashMap<String, TransportHandle>,
}

impl TransportManager {
    pub fn new() -> Self {
        Self {
            handles: HashMap::new(),
        }
    }

    /// 列出所有可用串口
    pub fn list_ports() -> Result<Vec<PortInfo>> {
        transport_serial::serial::list_ports()
    }

    /// 打开连接 (node_id 标识图中的一个传输节点)
    ///
    /// 同一 node_id 重复 open 会先关闭该 id 的旧连接 (不允许同 id 双连接),
    /// 其他 id 的连接不受影响。
    ///
    /// `link` 仅被 TestData 用作生成数据的线缆格式参考 (protocol 为 legacy 配置,
    /// schema 为可选帧 schema), 其他传输类型忽略此参数。
    /// 连接建立后可经 `update_link` 热更新, 无需重连。
    pub async fn open(
        &mut self,
        node_id: &str,
        config: TransportConfig,
        link: TestDataLink,
    ) -> Result<()> {
        // 同 id 重复 open: 先关闭旧连接 (Drop 会置 cancel 标志)
        self.handles.remove(node_id);

        let (write_tx, data_tx, cancel, test_data_running, test_data_notify, test_data_protocol) =
            match &config {
                TransportConfig::Serial(c) => {
                    let (w, d, c) = transport_serial::serial::spawn(c.clone())?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::Udp(c) => {
                    let (w, d, c) = transport_net::udp::spawn(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TcpClient(c) => {
                    let (w, d, c) = transport_net::tcp::spawn_client(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TcpServer(c) => {
                    let (w, d, c) = transport_net::tcp::spawn_server(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TestData(c) => {
                    let (write_tx, data_tx, cancel, running, notify, protocol) =
                        crate::test_data::spawn(c.clone(), link)?;
                    (write_tx, data_tx, cancel, Some(running), Some(notify), Some(protocol))
                }
                TransportConfig::Slcan(c) => {
                    let (w, d, c) = transport_can_bridge::slcan::spawn(c.clone())?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::CandleLight(c) => {
                    let (w, d, c) = transport_can_bridge::candle::spawn(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
            };

        self.handles.insert(
            node_id.to_string(),
            TransportHandle::new(
                write_tx,
                data_tx,
                cancel,
                test_data_running,
                test_data_notify,
                test_data_protocol,
                config.clone(),
            ),
        );

        log::info!("连接已建立: 节点 {node_id} -> {config:?}");
        Ok(())
    }

    /// 关闭指定节点的连接 (不存在的 id 静默忽略)
    pub fn close(&mut self, node_id: &str) {
        // 移除即触发 TransportHandle::Drop, 后台任务收到取消信号
        self.handles.remove(node_id);
    }

    /// 关闭所有节点的连接
    pub fn close_all(&mut self) {
        self.handles.clear();
    }

    /// 发送数据 — 未知 id 返回 Error::PortNotFound
    pub fn send(&self, node_id: &str, data: &[u8]) -> Result<()> {
        self.get(node_id)?.send(data)
    }

    /// 订阅指定节点的接收数据 (未知 id 返回 None)
    pub fn subscribe(&self, node_id: &str) -> Option<broadcast::Receiver<Vec<u8>>> {
        self.handles.get(node_id).map(super::handle::TransportHandle::subscribe)
    }

    /// 获取指定节点的连接状态 (未知 id 返回 None)
    pub fn state(&self, node_id: &str) -> Option<ConnectionState> {
        self.handles.get(node_id).map(super::handle::TransportHandle::state)
    }

    /// 获取指定节点的统计信息 (未知 id 返回 None)
    pub fn stats(&self, node_id: &str) -> Option<TransportStats> {
        self.handles.get(node_id).map(super::handle::TransportHandle::stats)
    }

    /// 获取指定节点的配置 (未知 id 返回 None) — 供外部查询 CAN 波特率等
    pub fn config(&self, node_id: &str) -> Option<TransportConfig> {
        self.handles.get(node_id).map(|h| h.config().clone())
    }

    /// 指定节点是否有打开的连接
    pub fn is_open(&self, node_id: &str) -> bool {
        self.handles.contains_key(node_id)
    }

    /// 列出所有已打开连接的节点 ID
    pub fn list_open(&self) -> Vec<String> {
        self.handles.keys().cloned().collect()
    }

    /// 更新指定节点的接收统计 (由外部调用, 当数据被消费时)
    pub fn record_rx(&self, node_id: &str, bytes: usize, frames: u64) {
        if let Some(h) = self.handles.get(node_id) {
            h.record_rx(bytes, frames);
        }
    }

    /// 设置指定节点的测试数据生成器运行状态 (仅 TestData 有效)
    pub fn set_test_data_running(&self, node_id: &str, running: bool) {
        if let Some(h) = self.handles.get(node_id) {
            h.set_test_data_running(running);
        }
    }

    /// 获取指定节点的测试数据生成器当前运行状态
    pub fn is_test_data_running(&self, node_id: &str) -> bool {
        self.handles
            .get(node_id)
            .is_some_and(super::handle::TransportHandle::is_test_data_running)
    }

    /// 运行时热更新指定节点的链路配置 (图/协议变化后调用)
    ///
    /// 仅 TestData 实际消费链路配置; 其他传输类型静默接受。
    /// 节点未打开时返回 Error::PortNotFound, 调用方据此提示用户重连。
    pub fn update_link(&self, node_id: &str, link: TestDataLink) -> Result<()> {
        self.get(node_id)?.update_link(link)?;
        Ok(())
    }

    fn get(&self, node_id: &str) -> Result<&TransportHandle> {
        self.handles
            .get(node_id)
            .ok_or_else(|| Error::PortNotFound(format!("传输节点未打开: {node_id}")))
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}