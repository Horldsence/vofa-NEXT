//! 内置工具清单 — 静态规格目录与前后端工具名集合

use provider::ToolSpecDto;
use serde_json::{json, Value};

// ============ 工具清单 ============

/// 工具规格简写构造。
fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpecDto {
    let mut schema = json!({
        "type": "object",
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    ToolSpecDto {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
    }
}

/// 内置工具清单 (静态;中文名描述)。
pub fn native_tool_specs() -> Vec<ToolSpecDto> {
    vec![
        // ---- 后端直连: 设备交互 ----
        spec(
            "list_transports",
            "列出全部传输节点 (串口/TCP/UDP/CAN 等) 及连接状态 [{node_id, state}]",
            json!({}),
            &[],
        ),
        spec(
            "list_serial_ports",
            "列出系统可用串口 [{name, port_type, vid, pid, manufacturer, product}]。连接串口前先查询端口名",
            json!({}),
            &[],
        ),
        spec(
            "send_bytes",
            "向指定传输节点发送原始字节。返回发送字节数",
            json!({
                "node_id": {"type": "string", "description": "目标传输节点 id"},
                "data": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "description": "字节数组"}
            }),
            &["node_id", "data"],
        ),
        spec(
            "send_string",
            "向指定传输节点发送 UTF-8 文本 (原样发送, 不自动加换行)。返回发送字节数",
            json!({
                "node_id": {"type": "string", "description": "目标传输节点 id"},
                "text": {"type": "string", "description": "要发送的文本"}
            }),
            &["node_id", "text"],
        ),
        spec(
            "send_can_frame",
            "发送 CAN 帧 (经 CAN 协议节点 encode_can 编码)",
            json!({
                "node_id": {"type": "string", "description": "目标 Transport 节点 id"},
                "protocol_node": {"type": "string", "description": "编码用 Protocol 节点 id, 缺省自动溯源"},
                "frame": {
                    "type": "object",
                    "description": "CAN 帧",
                    "properties": {
                        "id": {"type": "integer", "description": "帧 id (11/29 位)"},
                        "extended": {"type": "boolean", "description": "是否扩展帧"},
                        "data": {"type": "array", "items": {"type": "integer"}, "description": "数据字节 (最多 8)"},
                        "direction": {"type": "string", "enum": ["tx", "rx"], "description": "方向, 发送填 tx"}
                    },
                    "required": ["id", "data"]
                }
            }),
            &["node_id", "frame"],
        ),
        spec(
            "set_input_value",
            "设置节点图输入控件的值 (widget_id 为控件节点 id), 立即生效并触发求值",
            json!({
                "widget_id": {"type": "string", "description": "控件节点 id"},
                "value": {"type": "number", "description": "目标值"}
            }),
            &["widget_id", "value"],
        ),
        spec(
            "inject_bytes",
            "把字节从 source_node_id 注入全局字节平面路由到下游 (协议解析/回环), 无设备也可调试协议。返回命中下游数量",
            json!({
                "source_node_id": {"type": "string", "description": "注入源节点 id (字节边起点)"},
                "data": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "description": "字节数组"}
            }),
            &["source_node_id", "data"],
        ),
        // ---- 后端直连: 数据读取 ----
        spec(
            "get_graph_outputs",
            "读取节点图输出快照: {widgetId: {portId: value}} — 全部节点输出端口最新值, 观察计算/控件/波形输出的首选",
            json!({}),
            &[],
        ),
        spec(
            "get_recent_waveform",
            "读取指定数据源 (协议/FrameDecoder 节点 id) 最近 count 个采样点波形, 含通道名与数值",
            json!({
                "source": {"type": "string", "description": "数据源节点 id"},
                "count": {"type": "integer", "description": "采样点数 (上限 10000)"}
            }),
            &["source", "count"],
        ),
        spec(
            "get_waveform_window",
            "读取指定数据源时间窗口内波形 (start_ms/end_ms 为相对最新时间戳的毫秒偏移, 负数=过去)",
            json!({
                "source": {"type": "string", "description": "数据源节点 id"},
                "start_ms": {"type": "integer", "description": "窗口起点毫秒偏移, 如 -1000"},
                "end_ms": {"type": "integer", "description": "窗口终点毫秒偏移, 最新为 0"}
            }),
            &["source", "start_ms", "end_ms"],
        ),
        spec(
            "get_buffer_info",
            "读取指定数据源波形缓冲的通道数与点数 {channel_count, point_count}",
            json!({"source": {"type": "string", "description": "数据源节点 id"}}),
            &["source"],
        ),
        spec(
            "list_data_sources",
            "列出存在波形缓冲的数据源 id (配合 get_recent_waveform 使用)",
            json!({}),
            &[],
        ),
        spec(
            "get_can_frames",
            "读取最近 CAN 帧与总线负载统计 (fps, load_ratio)",
            json!({
                "count": {"type": "integer", "description": "最近帧条数 (上限 1000)"},
                "bitrate": {"type": "integer", "description": "总线比特率, 缺省 500000 (用于负载估算)"}
            }),
            &["count"],
        ),
        spec(
            "get_logic_data",
            "读取逻辑分析仪最近采样与解码事件 (UART/I2C/SPI 等)",
            json!({"count": {"type": "integer", "description": "条数 (上限 5000)"}}),
            &["count"],
        ),
        spec(
            "get_raw_data",
            "读取指定源最近收发的原始字节 (hex 编码, 分 TX/RX 方向与时间戳)。排查设备是否有数据的第一工具",
            json!({
                "source": {"type": "string", "description": "数据源节点 id (Transport 或 FrameDecoder)"},
                "max_bytes": {"type": "integer", "description": "最大读取字节 (上限 64KiB)"}
            }),
            &["source"],
        ),
        // ---- 知识库 ----
        spec(
            "read_skill",
            "读取内置知识库文档全文 (id 见系统提示词索引)",
            json!({
                "skill_id": {"type": "string", "description": "文档 id, 如 overview / nodes-reference / protocols / debug-recipes / tools-guide"},
                "lang": {"type": "string", "enum": ["zh", "en"], "description": "文档语言, 缺省 zh"}
            }),
            &["skill_id"],
        ),
        // ---- 前端托管: 节点编辑与 UI 操作 ----
        spec(
            "get_workspace",
            "读取画布全量状态: tabs、活跃 tab、全部 widget (id/kind/位置/配置/端口表)、全局 transport/protocol 节点 (id/配置/端口表)、全部连线。编辑节点前必读; 连线前对照各节点 ports 的 domain (同域才可连)",
            json!({}),
            &[],
        ),
        spec(
            "add_node",
            "添加节点: transport (传输) / protocol (协议) / widget (控件)。返回新节点 id",
            json!({
                "type": {"type": "string", "enum": ["transport", "protocol", "widget"], "description": "节点类别"},
                "kind": {"type": "string", "description": "类型: transport=Serial/Udp/TcpClient/TcpServer/TestData/Slcan/CandleLight; protocol=JustFloat/FireWater/RawData/Slcan/CandleLight/LogicDecode; widget=Knob/Slider/Button/Waveform/Math/FrameDecoder/..."},
                "tab_id": {"type": "string", "description": "widget 归属 tab, 缺省当前活跃 tab"},
                "config": {"type": "object", "description": "配置 (可选, 与默认配置深合并)"},
                "position": {"type": "object", "description": "画布位置 {x, y}, 缺省自动排布", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}}
            }),
            &["type", "kind"],
        ),
        spec(
            "update_node_config",
            "更新节点配置 (widget 为 params 深合并; transport/protocol: kind 可变 — kind 变化时配置整体替换, 其余字段深合并/重建)",
            json!({
                "node_id": {"type": "string", "description": "目标节点 id"},
                "config": {"type": "object", "description": "新配置 (部分或完整)"}
            }),
            &["node_id", "config"],
        ),
        spec(
            "remove_node",
            "删除节点 (widget 或全局 transport/protocol), 自动清理其连线并关闭连接",
            json!({"node_id": {"type": "string", "description": "目标节点 id"}}),
            &["node_id"],
        ),
        spec(
            "connect_nodes",
            "连接两个节点的端口 (后端编译校验)。handle 缺省时自动补默认端口;RawData 控件目标自动改写 src: 端口。端口域不匹配 (如频域接时域) 或成环会直接报错且不建边 — 错误信息含真实原因, 换端口或改配置后重试。成功返回 edge_id 并实时同步画布",
            json!({
                "source": {"type": "string", "description": "源节点 id"},
                "source_handle": {"type": "string", "description": "源端口 id (如 rx / out / ch0), 缺省自动"},
                "target": {"type": "string", "description": "目标节点 id"},
                "target_handle": {"type": "string", "description": "目标端口 id (如 in / data), 缺省自动"},
                "tab_id": {"type": "string", "description": "归属 tab, 缺省自动定位 (优先同时持有两端的 tab)"}
            }),
            &["source", "target"],
        ),
        spec(
            "disconnect_edge",
            "删除连线: 给 edge_id 精确删除, 或给 source+target (可只给一端) 删除第一条匹配",
            json!({
                "edge_id": {"type": "string", "description": "连线 id"},
                "source": {"type": "string", "description": "源节点 id (与 target 组合过滤)"},
                "target": {"type": "string", "description": "目标节点 id"}
            }),
            &[],
        ),
        spec(
            "move_node",
            "移动节点画布位置 (纯视觉调整)",
            json!({
                "node_id": {"type": "string", "description": "目标节点 id"},
                "x": {"type": "number"}, "y": {"type": "number"}
            }),
            &["node_id", "x", "y"],
        ),
        spec(
            "create_tab",
            "新建控制页 (画布 tab)。返回 tab_id",
            json!({"name": {"type": "string", "description": "页名, 缺省自动编号"}}),
            &[],
        ),
        spec(
            "set_active_tab",
            "切换活跃控制页",
            json!({"tab_id": {"type": "string", "description": "目标 tab id"}}),
            &["tab_id"],
        ),
        spec(
            "connect_transport",
            "打开传输连接 (串口/TCP/UDP/CAN/TestData)。连接后即可收发数据",
            json!({"node_id": {"type": "string", "description": "传输节点 id"}}),
            &["node_id"],
        ),
        spec(
            "disconnect_transport",
            "关闭传输连接",
            json!({"node_id": {"type": "string", "description": "传输节点 id"}}),
            &["node_id"],
        ),
        spec(
            "list_templates",
            "列出内置工作区模板 (id 与说明), 配合 apply_template 使用",
            json!({}),
            &[],
        ),
        spec(
            "apply_template",
            "一键应用内置工作区模板 (自动搭建传输→协议→显示链路)",
            json!({"template_id": {"type": "string", "description": "模板 id (先 list_templates 查询)"}}),
            &["template_id"],
        ),
    ]
}

/// 前端托管工具名集合。
pub(super) const FRONTEND_TOOLS: &[&str] = &[
    "get_workspace",
    "add_node",
    "update_node_config",
    "remove_node",
    "move_node",
    "create_tab",
    "set_active_tab",
    "connect_transport",
    "disconnect_transport",
    "list_templates",
    "apply_template",
];

/// 后端直连工具名集合 (与 `call_backend` 的 match 分支一一对应)。
pub(super) const BACKEND_TOOLS: &[&str] = &[
    "list_transports",
    "list_serial_ports",
    "send_bytes",
    "send_string",
    "send_can_frame",
    "set_input_value",
    "inject_bytes",
    "get_graph_outputs",
    "get_recent_waveform",
    "get_waveform_window",
    "get_buffer_info",
    "list_data_sources",
    "get_can_frames",
    "get_logic_data",
    "get_raw_data",
    "read_skill",
    // 连线拓扑 — 后端权威 (编译校验 + graph:source 事件同步画布)
    "connect_nodes",
    "disconnect_edge",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 内置清单完备: specs 与两路名字集合一致, 无重名, 前端工具都有声明。
    #[test]
    fn specs_cover_all_tools_without_duplicates() {
        let specs = native_tool_specs();
        let mut seen = std::collections::HashSet::new();
        for t in &specs {
            assert!(seen.insert(t.name.as_str()), "重复工具名: {}", t.name);
            assert!(!t.description.is_empty());
            assert_eq!(t.input_schema["type"], "object");
        }
        assert_eq!(seen.len(), BACKEND_TOOLS.len() + FRONTEND_TOOLS.len());
        for f in FRONTEND_TOOLS {
            assert!(seen.contains(f), "前端工具 {f} 未在 specs 中声明");
        }
        for b in BACKEND_TOOLS {
            assert!(seen.contains(b), "后端工具 {b} 未在 specs 中声明");
        }
        assert!(specs.len() >= 25);
    }
}
