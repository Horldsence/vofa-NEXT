//! MCP 工具入参 DTO — schemars 派生 JSON Schema (与外部客户端契约)

use can_types::CanFrame;
use rmcp::schemars;
use serde::Deserialize;
use serde_json::Value;

/// 发送字节的入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SendBytesParams {
    /// 目标传输节点 id。
    pub(super) node_id: String,
    /// 字节数组 (0-255)。
    pub(super) data: Vec<u8>,
}

/// 发送文本的入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SendStringParams {
    /// 目标传输节点 id。
    pub(super) node_id: String,
    /// UTF-8 文本。
    pub(super) text: String,
}

/// 字节注入入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct InjectBytesParams {
    /// 注入源节点 id (字节边起点)。
    pub(super) source_node_id: String,
    /// 字节数组 (0-255)。
    pub(super) data: Vec<u8>,
}

/// 输入控件赋值入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SetInputValueParams {
    /// 控件节点 id。
    pub(super) widget_id: String,
    /// 目标值。
    pub(super) value: f32,
}

/// 波形读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct WaveformParams {
    /// 数据源 (协议/FrameDecoder 节点 id)。
    pub(super) source: String,
    /// 读取的最近采样点数。
    pub(super) count: u32,
}

/// 时间窗波形读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct WaveformWindowParams {
    /// 数据源 (协议/FrameDecoder 节点 id)。
    pub(super) source: String,
    /// 窗口起点 (相对最新时间戳的毫秒偏移, 负数=过去)。
    pub(super) start_ms: i64,
    /// 窗口终点 (同上)。
    pub(super) end_ms: i64,
}

/// 缓冲区信息入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct BufferInfoParams {
    /// 数据源节点 id。
    pub(super) source: String,
}

/// 图更新参数 — nodes/edges 为前端同构的 JSON (`NodeDef` / `Edge`)。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct UpdateGraphParams {
    /// 目标 tab id (前端控件页 tab)。
    pub(super) tab_id: String,
    /// 节点定义数组 (与前端 NodeDef 格式一致: id / tab_id / kind)。
    #[serde(default)]
    pub(super) nodes: Vec<Value>,
    /// 边数组 (与前端 Edge 格式一致: from/to + 端口引用)。
    #[serde(default)]
    pub(super) edges: Vec<Value>,
    /// widget 配置记录数组 ({id, kind, params}) — 提供时整体替换该 tab 的
    /// widget 配置 (画布可完整渲染), 缺省保留现状。
    #[serde(default)]
    pub(super) widgets: Option<Vec<Value>>,
    /// 节点画布位置 ({node_id: {x, y}}) — 提供时合并进工作区位置表。
    #[serde(default)]
    pub(super) positions: Option<std::collections::HashMap<String, Value>>,
}

/// 连线入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ConnectEdgeParams {
    /// 源节点 id。
    pub(super) source: String,
    /// 目标节点 id。
    pub(super) target: String,
    /// 归属 tab (缺省自动定位: 优先同时持有两端的 tab)。
    pub(super) tab_id: Option<String>,
    /// 源端口 id (缺省按端口提示/节点类型补默认, 如 rx / out)。
    pub(super) source_handle: Option<String>,
    /// 目标端口 id (缺省按端口提示/节点类型补默认, 如 in)。
    pub(super) target_handle: Option<String>,
}

/// 删线入参 — edge_id 或 source/target 至少给一个。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct DisconnectEdgeParams {
    /// 连线 id (优先精确匹配)。
    pub(super) edge_id: Option<String>,
    /// 源节点 id (与 target 组合过滤, 可只给一端)。
    pub(super) source: Option<String>,
    /// 目标节点 id。
    pub(super) target: Option<String>,
}

/// CAN 帧读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CanFramesParams {
    /// 读取的最近帧条数 (上限 1000)。
    pub(super) count: u32,
    /// 总线比特率 (用于负载百分比估算, 缺省 500k)。
    pub(super) bitrate: Option<u32>,
}

/// CAN 帧发送入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SendCanFrameParams {
    /// 目标 Transport 节点 id。
    pub(super) node_id: String,
    /// 编码用 Protocol 节点 id (缺省沿字节平面自动溯源第一个)。
    pub(super) protocol_node: Option<String>,
    /// CAN 帧 (id 11/29 位;extended = 扩展帧;direction 通常 tx)。
    pub(super) frame: CanFrameDto,
}

/// CAN 帧入参 DTO — 本地定义以派生 `JsonSchema` (`can_types::CanFrame` 无此派生)。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CanFrameDto {
    /// 帧 id (11 位标准帧或 29 位扩展帧)。
    pub(super) id: u32,
    /// 是否扩展帧 (29 位 id)。
    #[serde(default)]
    pub(super) extended: bool,
    /// 是否远程帧。
    #[serde(default)]
    pub(super) rtr: bool,
    /// 数据字节 (最多 8 个)。
    #[serde(default)]
    pub(super) data: Vec<u8>,
    /// 方向 ("tx"/"rx", 发送填 "tx")。
    #[serde(default)]
    pub(super) direction: Option<String>,
}

impl From<CanFrameDto> for CanFrame {
    fn from(dto: CanFrameDto) -> Self {
        let direction = match dto.direction.as_deref() {
            Some("tx" | "Tx" | "TX") => can_types::CanDirection::Tx,
            _ => can_types::CanDirection::Rx,
        };
        Self {
            timestamp: vofa_core::now_us(),
            id: dto.id,
            extended: dto.extended,
            rtr: dto.rtr,
            dlc: u8::try_from(dto.data.len().min(8)).unwrap_or(8),
            data: dto.data.into_iter().take(8).collect(),
            direction,
        }
    }
}

/// 逻辑分析数据读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct LogicParams {
    /// 读取的最近采样 / 事件条数 (上限 5000)。
    pub(super) count: u32,
}

/// 原始字节读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct RawDataParams {
    /// 数据源节点 id (Transport 或 FrameDecoder)。
    pub(super) source: String,
    /// 最大读取字节数 (上限 64KiB)。
    pub(super) max_bytes: u32,
}
