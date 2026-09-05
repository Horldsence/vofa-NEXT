//! MCP 工具 handler — `rmcp::tool_router` 宏生成路由与分发

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo};
use rmcp::ServerHandler;
use tauri::AppHandle;

use super::params::{
    BufferInfoParams, CanFramesParams, ConnectEdgeParams, DisconnectEdgeParams, InjectBytesParams,
    LogicParams, RawDataParams, SendBytesParams, SendCanFrameParams, SendStringParams,
    SetInputValueParams, UpdateGraphParams, WaveformParams, WaveformWindowParams,
};
use super::toolbox::Toolbox;
use crate::tools;

/// MCP server handler — 以宏生成工具路由与分发。
#[derive(Clone)]
pub struct VofaMcpServer {
    toolbox: Toolbox,
    app: AppHandle,
}

/// 序列化值的工具结果统一包装。
fn tool_result(value: impl serde::Serialize) -> Result<CallToolResult, rmcp::ErrorData> {
    let content = ContentBlock::json(value)?;
    Ok(CallToolResult::success(vec![content]))
}

/// 共享实现错误字符串 → MCP internal error。
fn internal(e: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}

#[rmcp::tool_router]
impl VofaMcpServer {
    /// 构造 handler。
    pub const fn new(toolbox: Toolbox, app: AppHandle) -> Self {
        Self { toolbox, app }
    }

    /// 列出全部传输节点及其连接状态。
    #[rmcp::tool(
        description = "列出全部传输节点 (串口/TCP/UDP 等) 及其连接状态。返回 [{node_id, state}] 数组"
    )]
    async fn list_transports(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_transports(&self.toolbox).await)
    }

    /// 列出可用串口。
    #[rmcp::tool(
        description = "列出系统可用串口 [{name, port_type, vid, pid, serial_number, manufacturer, product}]。连接串口前先用它确定端口名"
    )]
    async fn list_serial_ports(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::list_serial_ports()
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 发送字节到指定传输节点。
    #[rmcp::tool(
        description = "向指定传输节点发送原始字节。data 为字节数组 (0-255)。返回发送字节数"
    )]
    async fn send_bytes(
        &self,
        Parameters(params): Parameters<SendBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_bytes(&self.toolbox, &params.node_id, &params.data)
            .await
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 发送文本 (UTF-8 字符串)。
    #[rmcp::tool(
        description = "向指定传输节点发送 UTF-8 文本 (按字节原样发送, 不自动加换行)。返回发送字节数"
    )]
    async fn send_string(
        &self,
        Parameters(params): Parameters<SendStringParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_string(&self.toolbox, &params.node_id, &params.text)
            .await
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 字节注入 — 沿全局字节平面路由 (喂协议引擎 / FrameDecoder / Transport.tx)。
    #[rmcp::tool(
        description = "把字节从 source_node_id 注入全局字节平面, 路由到其下游 (协议解析/回环发送)。与设备无连接时也可用于协议调试。返回命中下游数量"
    )]
    async fn inject_bytes(
        &self,
        Parameters(params): Parameters<InjectBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::inject_bytes(
            &self.toolbox,
            &self.app,
            &params.source_node_id,
            &params.data,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 设置控件输入值 (Input/Slider/Knob 等 widget 的当前值)。
    #[rmcp::tool(
        description = "设置节点图输入控件的值 (widget_id 为控件节点 id)。立即生效并触发一次求值"
    )]
    async fn set_input_value(
        &self,
        Parameters(params): Parameters<SetInputValueParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::set_input_value(
            &self.toolbox,
            &params.widget_id,
            params.value,
        ))
    }

    /// 读取图输出快照 (全部节点输出端口的最新值)。
    #[rmcp::tool(
        description = "读取节点图输出快照: {widgetId: {portId: value}}。用于观察控件/波形/计算节点的实时输出"
    )]
    async fn get_graph_outputs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_graph_outputs(&self.toolbox))
    }

    /// 读取指定数据源 (协议节点 id) 的最近波形数据。
    #[rmcp::tool(
        description = "读取指定数据源 (协议/FrameDecoder 节点 id) 最近 count 个采样点的波形窗口, 含通道名与数值"
    )]
    async fn get_recent_waveform(
        &self,
        Parameters(params): Parameters<WaveformParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::get_recent_waveform(&self.toolbox, &params.source, params.count)
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 读取指定数据源时间窗内的波形。
    #[rmcp::tool(
        description = "读取指定数据源在时间窗口内的波形 (start_ms/end_ms 为相对最新时间戳的毫秒偏移, 负数=过去, 如 start=-1000/end=0 即最近 1 秒)"
    )]
    async fn get_waveform_window(
        &self,
        Parameters(params): Parameters<WaveformWindowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::get_waveform_window(
            &self.toolbox,
            &params.source,
            params.start_ms,
            params.end_ms,
        )
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取缓冲区信息。
    #[rmcp::tool(description = "读取指定数据源波形缓冲的通道数与点数 {channel_count, point_count}")]
    async fn get_buffer_info(
        &self,
        Parameters(params): Parameters<BufferInfoParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_buffer_info(&self.toolbox, &params.source))
    }

    /// 列出可读取的数据源 (全部缓冲区 key)。
    #[rmcp::tool(description = "列出存在波形缓冲的数据源 id (可配合 get_recent_waveform 使用)")]
    async fn list_data_sources(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_data_sources(&self.toolbox))
    }

    /// 列出已有节点图的 tab id。
    #[rmcp::tool(description = "列出已提交节点图的 tab id 列表 (配合 update_graph 使用)")]
    async fn list_tabs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_tabs(&self.toolbox))
    }

    /// 提交 (替换) 指定 tab 的节点图 — 与前端提交同一路径, 界面实时同步。
    #[rmcp::tool(
        description = "替换指定 tab 的节点图。nodes/edges 与前端 NodeDef/Edge 格式一致;widgets 可选, 为控件配置记录数组 [{id, kind, params}] (提供时画布可完整渲染控件), positions 可选为节点画布位置 {node_id: {x, y}}。提交成功后前端界面实时刷新。返回派生端口表。编译失败 (环/端口域不匹配) 返回错误, 旧图保留"
    )]
    async fn update_graph(
        &self,
        Parameters(params): Parameters<UpdateGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::update_graph(
            &self.toolbox,
            &self.app,
            &params.tab_id,
            params.nodes,
            params.edges,
            params.widgets,
            params.positions,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取最近 CAN 帧与负载统计。
    #[rmcp::tool(
        description = "读取最近 CAN 帧 [{timestamp, id, extended, dlc, data, direction}] 与总线负载统计 {fps, load_ratio}"
    )]
    async fn get_can_frames(
        &self,
        Parameters(params): Parameters<CanFramesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_can_frames(
            &self.toolbox,
            params.count,
            params.bitrate,
        ))
    }

    /// 连线 (后端编译校验 — 域不匹配/成环直接报错, 不建边)。
    #[rmcp::tool(
        description = "在两个节点端口间建立连线。handle 缺省时自动补默认端口;RawData 控件目标自动改写 src: 端口。编译失败 (端口域不匹配/成环) 返回真实原因且不建边。成功返回 {edge_id} 并实时同步到界面"
    )]
    async fn connect_edge(
        &self,
        Parameters(params): Parameters<ConnectEdgeParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::connect_edge(
            &self.toolbox,
            &self.app,
            params.tab_id,
            &params.source,
            &params.target,
            params.source_handle,
            params.target_handle,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 删线。
    #[rmcp::tool(
        description = "删除连线: 给 edge_id 精确删除, 或给 source/target (可只给一端) 删除第一条匹配。成功返回被删边信息并实时同步到界面"
    )]
    async fn disconnect_edge(
        &self,
        Parameters(params): Parameters<DisconnectEdgeParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::disconnect_edge(
            &self.toolbox,
            &self.app,
            params.edge_id,
            params.source,
            params.target,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 发送 CAN 帧。
    #[rmcp::tool(
        description = "发送 CAN 帧 (经 CAN 协议节点 encode_can 编码)。protocol_node 缺省时沿字节平面自动溯源该传输下游的第一个 Protocol 节点"
    )]
    async fn send_can_frame(
        &self,
        Parameters(params): Parameters<SendCanFrameParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_can_frame(
            &self.toolbox,
            &params.node_id,
            params.protocol_node,
            params.frame.into(),
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取逻辑分析数据。
    #[rmcp::tool(
        description = "读取逻辑分析仪最近采样与解码事件 (UART/I2C/SPI 等) {samples, decoded_events}"
    )]
    async fn get_logic_data(
        &self,
        Parameters(params): Parameters<LogicParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_logic_data(&self.toolbox, params.count))
    }

    /// 读取最近原始字节。
    #[rmcp::tool(
        description = "读取指定源 (Transport/FrameDecoder 节点 id) 最近收发的原始字节 (hex 编码, 含方向与时间戳)"
    )]
    async fn get_raw_data(
        &self,
        Parameters(params): Parameters<RawDataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_raw_data(
            &self.toolbox,
            &params.source,
            params.max_bytes,
        ))
    }
}

#[rmcp::tool_handler]
impl ServerHandler for VofaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("vofa-next", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "VOFA-NEXT 串口/波形调试上位机。可发送指令到设备 (send_string/send_bytes/send_can_frame)、\
                 读取波形与图输出 (get_recent_waveform/get_graph_outputs)、\
                 修改节点图 (update_graph 整图替换 / connect_edge+disconnect_edge 增量连线, \
                 编译校验失败会返回真实原因)。先用 list_transports/list_data_sources/list_tabs 了解可用资源。",
            )
    }
}
