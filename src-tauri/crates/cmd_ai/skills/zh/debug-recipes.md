# 设备调试实战手册

## 任务一: 连接设备并显示波形

1. list_serial_ports → 确认端口名。
2. get_workspace → 了解当前 tab 与节点。
3. 若无现成节点: add_node(type=transport, kind=Serial) → add_node(type=protocol,
   kind=JustFloat) → add_node(type=widget, kind=Waveform)。
4. connect_nodes: transport.rx → protocol.in;protocol 通道口 → waveform.data。
5. connect_transport(node_id=transport-xxx)。
6. send_string / send_bytes 发送设备指令 (如有)。
7. get_recent_waveform(source=protocol 节点 id, count=100) 验证有数据;
   或 get_graph_outputs 看各端口实时值。

设备协议不是 JustFloat/FireWater 时,用 add_node(type=widget, kind=FrameDecoder)
建自定义帧解析,field 块命名 portName 输出。

## 任务二: 周期控制/采集

- 输入控制: add_node(type=widget, kind=Knob/Slider) → connect 到下游;或直接
  set_input_value(widget_id, value) 立即改值触发求值。
- Button 按下发值: set_input_value 同样适用 (press_value)。
- TextInput + TextOut 可把字符串写回 Transport 的 tx。

## 任务三: 无硬件调试协议

1. 搭好 Transport → Protocol → 显示 链路 (不必连接设备)。
2. inject_bytes(source_node_id=transport id, data=[...]) 模拟设备字节。
3. get_graph_outputs / get_recent_waveform 验证解析结果。

## 任务四: 排查"没有数据"

1. list_transports → 传输是否 Connected。
2. get_raw_data(source=transport id) → 设备字节真的进来了吗 (分 TX/RX)。
3. 字节正常但无波形 → 检查连线 (get_workspace) 与协议类型/channels。
4. get_buffer_info(source=协议 id) → 缓冲是否有累积点数。

## 任务五: CAN 总线调试

1. add_node(type=transport, kind=CandleLight 或 Slcan) → connect_transport。
2. get_can_frames(count=100) 查看最近帧 + 负载。
3. send_can_frame(node_id=..., frame={id: 0x123, data: [1,2,3], direction: "tx"})。

## 任务六: 快速搭建演示工作区

apply_template(template_id=...) 一键应用内置模板 (先 list_templates 查可用模板),
适合给用户演示标准链路。

## 通用工作约定

- 编辑前先 get_workspace 读现状;小步修改,每步用对应读取工具验证。
- 节点 id 是一切工具的主键 (transport-/protocol-/widget- 前缀)。
- 编译失败 (环/端口不匹配) 会返回错误且保留旧图 — 修正后重试即可。
