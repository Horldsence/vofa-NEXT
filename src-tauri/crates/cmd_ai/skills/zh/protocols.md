# 协议与数据格式

## JustFloat (二进制浮点)

- 帧 = N 个 float32 (小端) + 帧尾 `00 00 80 7F` (+Inf)。
- 例: 2 通道帧 `A0 B0 C0 D0 | A1 B1 C1 D1 | 00 00 80 7F` (每个通道 4 字节)。
- 解析器在字节流中滑动查找帧尾对齐;channels 设 null 自动检测,或手动指定。
- 设备端发送示例 (C):
  `float ch[2]; memcpy(buf, ch, 8); buf[8]=0; buf[9]=0; buf[10]=0x80; buf[11]=0x7f;`

## FireWater (ASCII)

- 帧 = 逗号分隔浮点 + 换行: `1.23,4.56,7.89\n`。
- 通道数自动检测 (首行字段数)。适合 printf 调试 (`printf("%.2f,%.2f\n", a, b)`)。

## 自定义帧 (FrameDecoder / 帧 schema)

FrameDecoder 控件用块描述帧结构,依次匹配:
- `header`: 固定帧头 (hex)。
- `field`: 字段 (uint8/uint16LE/uint32LE/float32LE/...),可命名 portName 输出为数值口。
- `checksum`: 校验 (sum8/xor/xor_ff/crc16-modbus...),cover 指定覆盖范围,append/verify 位置。
- `tail`: 固定帧尾 (hex)。

解码成功输出 field 命名端口 + 可选 valid/frameCount/lastTimestamp/fps 端口。

## CAN

- Transport 选 Slcan (串口 CAN 适配器) 或 CandleLight (USB-CAN),Protocol 对应 Slcan/CandleLight。
- 发送 CAN 帧: send_can_frame 工具 (协议节点 encode_can 编码;帧含 id/extended/data/direction)。
- 读取: get_can_frames 返回最近帧与负载统计 (fps、load_ratio)。

## 逻辑分析仪

- Transport/协议侧接入后,LogicDecode 解码 UART/I2C/SPI。
- get_logic_data 返回最近采样与解码事件。

## 协议调试技巧

- 无硬件时用 inject_bytes 从源节点注入字节,验证解析链路 (返回命中下游数)。
- get_raw_data 查看设备实际收发的原始字节 (hex, 分 TX/RX) — 排查"没数据"先看这里。
- 通道数:protocol.channels (null=自动) 决定 ProtocolSource 输出 ch0..chN 的数量。
