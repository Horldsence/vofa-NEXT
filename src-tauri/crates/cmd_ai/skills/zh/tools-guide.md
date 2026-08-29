# 内置工具使用指南

内置工具分两类:后端直连 (数据/发送,直接执行) 与前端托管 (节点编辑/UI 操作,
经界面执行,画布实时刷新并进入撤销历史)。

## 读现状 (先读后写)

- get_workspace: 画布全量 JSON — tabs、active tab、widgets (id/kind/位置/配置/端口表)、
  全局 transport/protocol 节点 (id/配置/端口表)、全部边。**编辑前必读**;
  各节点 ports 字段含 inputs/outputs 及其 domain 标注,连线前用它核对端口与类型。
- list_transports / list_serial_ports: 连接状态与可用串口。
- list_data_sources / get_buffer_info: 有哪些波形数据源及规模。
- list_templates: 可用的工作区模板 id。

## 编辑节点 (前端托管)

- add_node {type: transport|protocol|widget, kind, tab_id?, config?, position?}
  → 返回 node_id。widget 省略 tab_id 时加入当前活跃 tab;config 给部分字段时
  与默认配置深合并。
- update_node_config {node_id, config}: 更新配置 (widget 为 params 深合并;
  transport/protocol 的 kind 可变 — kind 变化时配置整体替换,其余字段合并)。
- remove_node {node_id}: 删除 widget 或全局节点 (自动清理其连线与连接)。
- move_node {node_id, x, y}: 调整画布位置 (纯视觉)。
- create_tab {name?} / set_active_tab {tab_id}。
- connect_transport / disconnect_transport {node_id}: 打开/关闭连接。
- apply_template {template_id}。

## 连线 (后端权威, 编译校验)

- connect_nodes {source, target, source_handle?, target_handle?, tab_id?}: 连线。
  handle 省略时自动补默认端口;RawData 控件目标自动改写 src: 端口。
  **端口域不匹配 (如频域接时域) / 成环 / 端口不存在会直接报错且不建边**,
  错误信息含真实原因 (两端口及其域) — 换端口或改配置后重试。成功返回 edge_id,
  画布实时同步。
- disconnect_edge {edge_id} 或 {source, target} (可只给一端): 删除连线。

## 设备交互 (后端直连)

- send_bytes {node_id, data: number[]} / send_string {node_id, text}: 发送。
- send_can_frame {node_id, protocol_node?, frame}: CAN 帧发送。
- set_input_value {widget_id, value}: 输入控件赋值。

## 数据读取 (后端直连)

- get_graph_outputs: 所有节点输出端口最新值 (低频轮询用它最省)。
- get_recent_waveform {source, count≤10000} / get_waveform_window {source,
  start_ms, end_ms} (相对最新时间戳的毫秒偏移)。
- get_can_frames {count≤1000, bitrate?} / get_logic_data {count≤5000} /
  get_raw_data {source, max_bytes≤64KiB}。
- read_skill {skill_id}: 读本知识库全文 (id 见系统提示词索引)。

## 注意事项与限制

- 波形/CAN/逻辑/原始字节读取有上限,大 count 返回会被截断 — 需要时先小 count 探查。
- 连线拓扑由后端权威保存并编译校验,所有写入方 (画布 / 内置 AI / 外部 MCP) 共写
  同一张图;本工具失败时返回的错误可直接用于自我修正。
- 工具结果均为 JSON 字符串;节点 id 是主键,不要凭空捏造,一律来自 get_workspace 等读取结果。
- 编辑操作会进入应用撤销历史,用户可 Ctrl+Z 回滚 AI 的修改。
