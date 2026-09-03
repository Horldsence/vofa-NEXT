# VOFA-NEXT 软件与核心概念总览

VOFA-NEXT 是一款数据采集与调试上位机 (VOFA+ 的下一代),面向嵌入式开发中的
串口 / TCP / UDP / CAN / 逻辑分析仪调试场景。核心能力:设备连接、协议解析、
波形显示、控件交互面板、CAN 总线分析、逻辑解码。

## 双平面数据架构

- **字节平面 (全局)**:Transport (传输) 与 Protocol (协议) 节点组成的事件驱动字节流网络。
  设备字节从 Transport.rx 流出,进入 Protocol.in 解析;Protocol.out 产出数值通道。
- **数值平面 (每 tab)**:每个控件页 (tab) 是一张独立编译的节点图 (DAG)。
  ProtocolSource 引用全局 Protocol 节点的最新帧,输出 ch0..chN 数值通道,
  供波形、仪表等下游节点消费。求值由后端编译后的 CompiledGraph 执行。

## 画布与节点

- **控制页 (tab)**:用户可建多个页签,每页是一张画布,承载若干 widget 节点。
- **全局节点**:Transport / Protocol 节点不属于任何 tab,渲染在所有画布上。
  这是字节平面的组成部分。
- **widget 节点**:输入控件 (Knob/Slider/Button/...)、显示控件 (Waveform/Gauge/...)、
  计算节点 (Math/Filter/FFT/FrameDecoder/Trigger/...) 都以 widget 形式放在某 tab 内。
- **边**:字节边 (Vec<u8>,Transport→Protocol 等) 与数值边 (f32,控件间)。
  连线时后端自动编译、派生端口表;编译失败保留旧图并报错。

## 软件操作入口 (内置 AI 工具)

- 节点图读写:get_workspace 读画布全量状态;add_node / remove_node /
  update_node_config / connect_nodes / disconnect_edge / move_node 增量编辑
  (与手工编辑等效:画布实时刷新,进撤销历史)。
- 传输:list_serial_ports 查串口;connect_transport / disconnect_transport 连接;
  send_bytes / send_string / send_can_frame 发送。
- 数据读取:get_graph_outputs (全部输出端口最新值)、get_recent_waveform /
  get_waveform_window (波形)、get_can_frames (CAN)、get_logic_data (逻辑解码)、
  get_raw_data (原始字节 TX/RX)、get_buffer_info、list_data_sources。
- 其他:set_input_value 控制输入控件、apply_template 一键搭建模板工作区。

## 数据流示例

TestData (模拟设备) → Protocol(JustFloat) → Waveform:
连接 Transport 与 Protocol 的字节边 (transport.rx → protocol.in),再连数值边
(protocol.out → waveform.data),connect_transport 后即可看到波形。
