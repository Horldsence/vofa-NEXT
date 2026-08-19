use std::collections::HashMap;
use tokio::sync::broadcast;
use vofa_next_core::{
    ConnectionState, Error, PortInfo, Result, TestDataLink, TransportConfig, TransportStats,
};

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
        crate::serial::list_ports()
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
                    let (w, d, c) = crate::serial::spawn(c.clone())?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::Udp(c) => {
                    let (w, d, c) = crate::udp::spawn(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TcpClient(c) => {
                    let (w, d, c) = crate::tcp::spawn_client(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TcpServer(c) => {
                    let (w, d, c) = crate::tcp::spawn_server(c.clone()).await?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::TestData(c) => {
                    let (w, d, c, r, n, p) = crate::test_data::spawn(c.clone(), link).await?;
                    (w, d, c, Some(r), Some(n), Some(p))
                }
                TransportConfig::Slcan(c) => {
                    let (w, d, c) = crate::slcan::spawn(c.clone())?;
                    (w, d, c, None, None, None)
                }
                TransportConfig::CandleLight(c) => {
                    let (w, d, c) = crate::candle::spawn(c.clone()).await?;
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

        log::info!("连接已建立: 节点 {} -> {:?}", node_id, config);
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
        self.handles.get(node_id).map(|h| h.subscribe())
    }

    /// 获取指定节点的连接状态 (未知 id 返回 None)
    pub fn state(&self, node_id: &str) -> Option<ConnectionState> {
        self.handles.get(node_id).map(|h| h.state())
    }

    /// 获取指定节点的统计信息 (未知 id 返回 None)
    pub fn stats(&self, node_id: &str) -> Option<TransportStats> {
        self.handles.get(node_id).map(|h| h.stats())
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
            .map(|h| h.is_test_data_running())
            .unwrap_or(false)
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
            .ok_or_else(|| Error::PortNotFound(format!("传输节点未打开: {}", node_id)))
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_next_core::{ProtocolConfig, TestDataConfig, TestSignal};

    fn test_data_config() -> TransportConfig {
        TransportConfig::TestData(TestDataConfig {
            channels: 2,
            sample_rate: 1000.0,
            signal: TestSignal::Sine,
        })
    }

    async fn open_node(mgr: &mut TransportManager, id: &str) {
        mgr.open(id, test_data_config(), TestDataLink::new(ProtocolConfig::RawData))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn open_close_single_node() {
        let mut mgr = TransportManager::new();
        assert!(!mgr.is_open("a"));
        open_node(&mut mgr, "a").await;
        assert!(mgr.is_open("a"));
        assert_eq!(mgr.state("a"), Some(ConnectionState::Connected));
        assert!(matches!(
            mgr.config("a"),
            Some(TransportConfig::TestData(_))
        ));

        mgr.close("a");
        assert!(!mgr.is_open("a"));
        assert!(mgr.state("a").is_none());
        assert!(mgr.config("a").is_none());
    }

    #[tokio::test]
    async fn multiple_nodes_are_independent() {
        let mut mgr = TransportManager::new();
        open_node(&mut mgr, "a").await;
        open_node(&mut mgr, "b").await;

        let mut open = mgr.list_open();
        open.sort();
        assert_eq!(open, vec!["a".to_string(), "b".to_string()]);

        // 各自收发统计互不影响
        mgr.send("a", &[1, 2, 3]).unwrap();
        mgr.send("a", &[4]).unwrap();
        mgr.send("b", &[5, 6]).unwrap();
        let sa = mgr.stats("a").unwrap();
        assert_eq!((sa.tx_bytes, sa.tx_frames), (4, 2));
        let sb = mgr.stats("b").unwrap();
        assert_eq!((sb.tx_bytes, sb.tx_frames), (2, 1));

        mgr.record_rx("a", 10, 2);
        assert_eq!(mgr.stats("a").unwrap().rx_bytes, 10);
        assert_eq!(mgr.stats("b").unwrap().rx_bytes, 0);

        // TestData 运行开关互不影响
        mgr.set_test_data_running("a", true);
        assert!(mgr.is_test_data_running("a"));
        assert!(!mgr.is_test_data_running("b"));

        // 关闭 a 不影响 b
        mgr.close("a");
        assert!(!mgr.is_open("a"));
        assert!(mgr.is_open("b"));

        mgr.close_all();
        assert!(mgr.list_open().is_empty());
    }

    #[tokio::test]
    async fn reopen_same_id_replaces_connection() {
        let mut mgr = TransportManager::new();
        open_node(&mut mgr, "a").await;
        mgr.send("a", &[1, 2, 3]).unwrap();
        mgr.set_test_data_running("a", true);

        // 重复 open 同 id: 先关闭旧连接, 状态/统计重置
        open_node(&mut mgr, "a").await;
        assert!(mgr.is_open("a"));
        assert_eq!(mgr.list_open().len(), 1);
        let s = mgr.stats("a").unwrap();
        assert_eq!((s.tx_bytes, s.tx_frames), (0, 0));
        assert!(!mgr.is_test_data_running("a"));
    }

    #[tokio::test]
    async fn unknown_node_id_errors() {
        let mut mgr = TransportManager::new();
        let err = mgr.send("nope", &[1]).unwrap_err();
        assert!(matches!(err, Error::PortNotFound(_)));
        assert!(err.to_string().contains("nope"));
        assert!(mgr.state("nope").is_none());
        assert!(mgr.stats("nope").is_none());
        assert!(mgr.config("nope").is_none());
        assert!(mgr.subscribe("nope").is_none());
        assert!(!mgr.is_open("nope"));
        // 关闭未知 id 不 panic
        mgr.close("nope");
    }

    #[tokio::test]
    async fn subscribe_receives_test_data() {
        let mut mgr = TransportManager::new();
        open_node(&mut mgr, "a").await;
        open_node(&mut mgr, "b").await;
        let mut rx_a = mgr.subscribe("a").unwrap();
        let mut rx_b = mgr.subscribe("b").unwrap();

        // 只启动 a, b 不应有数据
        mgr.set_test_data_running("a", true);
        let data = tokio::time::timeout(std::time::Duration::from_secs(2), rx_a.recv())
            .await
            .expect("a 应产生数据")
            .unwrap();
        assert!(!data.is_empty());

        let b_result =
            tokio::time::timeout(std::time::Duration::from_millis(300), rx_b.recv()).await;
        assert!(b_result.is_err(), "b 未启动, 不应有数据");

        // 关闭 a 后其后台任务退出, 通道最终关闭
        mgr.close("a");
    }

    #[tokio::test]
    async fn send_loops_back_to_subscribers() {
        let mut mgr = TransportManager::new();
        open_node(&mut mgr, "a").await;
        let mut rx = mgr.subscribe("a").unwrap();

        // 写入的字节统一回环到本节点接收广播 (transport→transport 路由链不断裂)
        mgr.send("a", &[0xDE, 0xAD]).unwrap();
        let data = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("写入应回环到接收广播")
            .unwrap();
        assert_eq!(data, vec![0xDE, 0xAD]);
    }

    #[tokio::test]
    async fn test_data_protocol_hot_update() {
        let mut mgr = TransportManager::new();
        mgr.open(
            "a",
            test_data_config(),
            TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(2) }),
        )
        .await
        .unwrap();
        let mut rx = mgr.subscribe("a").unwrap();
        mgr.set_test_data_running("a", true);

        // JustFloat: 帧尾 00 00 80 7f
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("JustFloat 数据")
            .unwrap();
        assert!(
            first.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]),
            "应为 JustFloat 格式"
        );

        // 热更新为 FireWater — 无需重建连接, 后续批次应为 ASCII CSV
        mgr.update_link(
            "a",
            TestDataLink::new(ProtocolConfig::FireWater { channels: Some(2) }),
        )
        .unwrap();
        let mut saw_csv = false;
        for _ in 0..50 {
            let batch = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("热更新后数据")
                .unwrap();
            if batch.last() == Some(&b'\n') && batch.iter().all(|b| b.is_ascii()) {
                saw_csv = true;
                break;
            }
        }
        assert!(saw_csv, "热更新后应生成 FireWater CSV 格式");

        // 未打开的节点热更新报错 (前端据此提示重连)
        assert!(
            mgr.update_link("nope", TestDataLink::new(ProtocolConfig::RawData))
                .is_err()
        );
    }

    /// TestData 经 schema 热更新: Custom encode 块改变输出格式
    #[tokio::test]
    async fn test_data_schema_hot_update() {
        use vofa_next_core::{
            DecoderBlockDef, EncodeBlockDef, FieldType, ProtocolSchema, SchemaPreset,
        };

        let mut mgr = TransportManager::new();
        mgr.open(
            "a",
            test_data_config(),
            TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(1) }),
        )
        .await
        .unwrap();
        let mut rx = mgr.subscribe("a").unwrap();
        mgr.set_test_data_running("a", true);

        // 初始: legacy JustFloat (帧尾 00 00 80 7f)
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("JustFloat 数据")
            .unwrap();
        assert!(
            first.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]),
            "应为 JustFloat 格式"
        );

        // 热更新为 Custom schema: encode = AA + float32LE(v) + BB
        // → 每采样帧 6 字节, 批次为帧拼接 (AA 开头, 长度 % 6 == 0)
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![DecoderBlockDef::Field {
                id: "f".into(),
                field_type: FieldType::Float32LE,
                port_name: "v".into(),
                length_ref: None,
                match_id: None,
            }],
            encode: Some(vec![
                EncodeBlockDef::ConstHex { hex: "AA".into() },
                EncodeBlockDef::VarRef {
                    port_name: "v".into(),
                    field_type: FieldType::Float32LE,
                },
                EncodeBlockDef::ConstHex { hex: "BB".into() },
            ]),
        };
        mgr.update_link(
            "a",
            TestDataLink {
                protocol: ProtocolConfig::JustFloat { channels: Some(1) },
                schema: Some(schema),
            },
        )
        .unwrap();

        let mut saw_schema_frame = false;
        for _ in 0..50 {
            let batch = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("schema 热更新后数据")
                .unwrap();
            if batch.first() == Some(&0xAA) && batch.len() % 6 == 0 && batch.last() == Some(&0xBB)
            {
                saw_schema_frame = true;
                break;
            }
        }
        assert!(saw_schema_frame, "热更新后应按 Custom schema encode 块生成帧");

        // 再次热更新回 legacy (schema = None): 输出恢复 JustFloat
        mgr.update_link(
            "a",
            TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(1) }),
        )
        .unwrap();
        let mut saw_legacy = false;
        for _ in 0..50 {
            let batch = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("回退 legacy 后数据")
                .unwrap();
            if batch.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]) {
                saw_legacy = true;
                break;
            }
        }
        assert!(saw_legacy, "回退后应恢复 legacy JustFloat 格式");
    }
}
