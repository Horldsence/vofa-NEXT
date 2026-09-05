use error::PortNotFoundError;
use schema_types::TestDataLink;
use std::collections::HashMap;
use tokio::sync::broadcast;
use vofa_core::{
    ConnectionState, Error, PortInfo, Result, TestDataConfig, TransportConfig, TransportStats,
};

use crate::handle::{LiveNodeHandle, TransportHandle};

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
        serial::serial::list_ports()
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

        let (write_tx, data_tx, cancel, test_data_runtime) = match &config {
            TransportConfig::Serial(c) => {
                let (w, d, c) = serial::serial::spawn(c.clone())?;
                (w, d, c, None)
            }
            TransportConfig::Udp(c) => {
                let (w, d, c) = net::udp::spawn(c.clone()).await?;
                (w, d, c, None)
            }
            TransportConfig::TcpClient(c) => {
                let (w, d, c) = net::tcp::spawn_client(c.clone()).await?;
                (w, d, c, None)
            }
            TransportConfig::TcpServer(c) => {
                let (w, d, c) = net::tcp::spawn_server(c.clone()).await?;
                (w, d, c, None)
            }
            TransportConfig::TestData(c) => {
                // 连接即生成: 生成器随 open 立即产出字节流, 随 close 生灭
                let (write_tx, data_tx, cancel, runtime) =
                    crate::test_data::spawn(c.clone(), link)?;
                (write_tx, data_tx, cancel, Some(runtime))
            }
            TransportConfig::Slcan(c) => {
                let (w, d, c) = can_bridge::slcan::spawn(c.clone())?;
                (w, d, c, None)
            }
            TransportConfig::CandleLight(c) => {
                let (w, d, c) = can_bridge::candle::spawn(c.clone()).await?;
                (w, d, c, None)
            }
        };

        self.handles.insert(
            node_id.to_string(),
            TransportHandle::new(write_tx, data_tx, cancel, test_data_runtime, config.clone()),
        );

        log::info!("连接已建立: 节点 {node_id} -> {config:?}");
        Ok(())
    }

    /// 关闭指定节点的连接 (不存在的 id 静默忽略)
    pub fn close(&mut self, node_id: &str) {
        // 移除即触发 TransportHandle::Drop, 后台任务收到取消信号
        self.handles.remove(node_id);
    }

    /// 发送数据 — 未知 id 返回 Error::PortNotFound
    pub fn send(&self, node_id: &str, data: &[u8]) -> Result<()> {
        self.get(node_id)?.send(data)
    }

    /// 订阅指定节点的接收数据 (未知 id 返回 None)
    pub fn subscribe(&self, node_id: &str) -> Option<broadcast::Receiver<Vec<u8>>> {
        self.handles
            .get(node_id)
            .map(super::handle::TransportHandle::subscribe)
    }

    /// 获取指定节点的连接状态 (未知 id 返回 None)
    pub fn state(&self, node_id: &str) -> Option<ConnectionState> {
        self.handles
            .get(node_id)
            .map(super::handle::TransportHandle::state)
    }

    /// 获取指定节点的统计信息 (未知 id 返回 None)
    pub fn stats(&self, node_id: &str) -> Option<TransportStats> {
        self.handles
            .get(node_id)
            .map(super::handle::TransportHandle::stats)
    }

    /// 获取指定节点的配置 (未知 id 返回 None) — 供外部查询 CAN 波特率等
    pub fn config(&self, node_id: &str) -> Option<TransportConfig> {
        self.handles.get(node_id).map(TransportHandle::config)
    }

    /// 列出所有已打开连接的节点 ID
    pub fn list_open(&self) -> Vec<String> {
        self.handles.keys().cloned().collect()
    }

    /// 取节点轻量句柄 — 读任务启动时调用一次, 之后每批的配置查询与 rx 统计
    /// 上报都免 manager 全局锁 (不再与 open/close/其他传输串行化)
    pub fn live_handle(&self, node_id: &str) -> Option<LiveNodeHandle> {
        self.handles.get(node_id).map(|handle| handle.live(node_id))
    }

    /// 运行时热更新指定节点的链路配置 (图/协议变化后调用)
    ///
    /// 仅 TestData 实际消费链路配置; 其他传输类型静默接受。
    /// 节点未打开时返回 Error::PortNotFound, 调用方据此提示用户重连。
    pub fn update_link(
        &self,
        node_id: &str,
        link: TestDataLink,
        config: Option<TestDataConfig>,
    ) -> Result<()> {
        self.get(node_id)?.update_link(link, config)?;
        Ok(())
    }

    fn get(&self, node_id: &str) -> Result<&TransportHandle> {
        self.handles.get(node_id).ok_or_else(|| {
            Error::PortNotFound(PortNotFoundError {
                port: node_id.to_string(),
            })
        })
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}
