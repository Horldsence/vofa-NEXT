# 节点与控件类型参考

## 传输节点 (Transport, 全局)

| kind | 参数要点 |
|---|---|
| Serial | port_name (先 list_serial_ports 查询)、baud_rate、data_bits、parity、stop_bits、flow_control |
| Udp | local_addr/local_port (本端)、remote_addr/remote_port (对端) |
| TcpClient | host、port |
| TcpServer | listen_addr、listen_port |
| TestData | channels、sample_rate、signal (Sine/Square/Triangle/Sawtooth/Random/Dc/Chirp/Steps/Noise/MultiTone) — 无硬件模拟数据源 |
| Slcan | 串口参数 + can_bitrate (lawicel 协议适配器) |
| CandleLight | bus、address、can_bitrate、channel (canable 等 USB-CAN) |

节点 id 规则:`transport-XXXXXXXX` (nanoid)。连接状态可用 list_transports 查询。

## 协议节点 (Protocol, 全局)

| kind | 说明 |
|---|---|
| JustFloat | 二进制浮点帧: N×f32(小端) + 帧尾 00 00 80 7F;channels: null=自动检测 |
| FireWater | ASCII 帧: "1.23,4.56\n" 逗号分隔浮点 + 换行 |
| RawData | 原始字节直通 (不做浮点解析),供 RawData 控件显示 |
| Slcan / CandleLight | CAN 传输的协议层 (encode_can 编码 CAN 帧) |
| LogicDecode | 逻辑分析仪解码协议 (decoder 配置 UART/I2C/SPI) |

节点 id 规则:`protocol-XXXXXXXX`。协议节点可配 convertTo (二次转换) 与自定义帧 schema。

## 控件 widget (按 tab)

- **输入类**:Knob (旋钮 min/max/step)、Slider、Button (按下/松开各发 press_value/release_value)、
  Radio (options: [[标签, 值],...])、Checkbox (checked_value/unchecked_value)、TextInput (文本下发)。
  输入控件的值变化经 set_input_value 即时生效。
- **显示类**:Waveform (多通道波形)、Gauge、NumberDisplay、LED (阈值变色)、PieChart、
  Image (rgb888 帧图)、Label、Spectrum (频谱)、Model3D (轨迹/姿态)、TableView、TextDisplay、RawData。
- **计算类**:Math (add/sub/mul/div/pow/mod/min/max/abs/sin/cos/tan/log/sqrt..., 可选 inputCount)、
  Filter (Lowpass/Highpass/Bandpass/Bandstop)、FFT、IFFT、Str (字符串处理)、
  FrameDecoder (自定义帧解析块: header/field/checksum/tail, field 可命名 portName 输出)、
  Trigger (规则匹配: pattern → 输出值/文本)、Command (组帧发送: 常量/变量引用/校验块)、TextOut (字符串回发到 Transport)、Custom (JS 自定义控件)。

widget 节点 id 规则:`widget-XXXXXXXX` (widget 内部 params.id 与节点 id 一致)。

## 连线规则

### 端口域 (domain) — 同域才能相连

每个端口属于固定域,跨域连线会被后端编译直接拒绝 (错误信息含两端口的具体域):

| 域 | 典型端口 |
|---|---|
| bytes (字节) | Transport.rx / Transport.tx、Protocol.in / Protocol.out、FrameDecoder.in、Command.loopbackOut |
| time (时域数值) | Protocol 的 ch0..chN、Math.result、Filter.result、IFFT.out0、FrameDecoder 各字段口、各类输入控件 value、Waveform.CH0..N / Gauge.value 等显示输入 |
| freq (频域) | FFT.spectrum (输出) → 仅 IFFT.spectrum / Spectrum.spectrum 输入口接受 |
| string (字符串) | RawData 预设 Protocol 的 str、TextInput.str、Trigger.text、Str 的字符串口、TextDisplay.text、TextOut.text |

典型链路:Transport.rx(bytes) → Protocol.in;Protocol.chN(time) → Waveform/Gauge/Math;
FFT.spectrum(freq) → IFFT 或 Spectrum;str 口(string) → TextDisplay / Str / TextOut。

### 其他规则

- RawData 控件的输入口是动态派生的 (`src:<source>:<handle>`),连线时自动改写
  targetHandle,无需手工指定;它接受 bytes 与 time 源,仅拒绝 freq。
- widget 只能与同 tab 的 widget 或全局节点相连 (跨 tab 会被拒绝)。
- 端口名与端口域用 get_workspace 返回中各节点的 ports 字段对照 (inputs/outputs
  均含 domain 标注,与画布渲染同源);连线前先读再连。
- 连线由后端编译校验:域不匹配 / 成环 / 端口不存在会直接报错且不建边,
  错误信息含原因 — 按提示换端口或改配置后重试。
- 编辑节点图统一走内置工具 (add_node/connect_nodes/...),画布即时同步且可撤销。
