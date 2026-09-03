# Protocols & Data Formats

## JustFloat (binary float)

- Frame = N × float32 (little-endian) + tail `00 00 80 7F` (+Inf).
- Example 2-channel frame: `A0 B0 C0 D0 | A1 B1 C1 D1 | 00 00 80 7F`.
- The parser slides through the byte stream looking for the tail; channels null
  = auto-detect, or fixed.
- Device-side example (C):
  `float ch[2]; memcpy(buf, ch, 8); buf[8]=0; buf[9]=0; buf[10]=0x80; buf[11]=0x7f;`

## FireWater (ASCII)

- Frame = comma-separated floats + newline: `1.23,4.56,7.89\n`.
- Channel count auto-detected from the first line. Great for printf debugging
  (`printf("%.2f,%.2f\n", a, b)`).

## Custom Frames (FrameDecoder / frame schema)

A FrameDecoder widget describes the frame as ordered blocks:
- `header`: fixed header bytes (hex).
- `field`: value field (uint8/uint16LE/uint32LE/float32LE/...); a named
  portName becomes a numeric output port.
- `checksum`: sum8/xor/xor_ff/crc16-modbus..., with coverage and append/verify
  position.
- `tail`: fixed tail bytes (hex).

Successful decode outputs named field ports plus optional
valid/frameCount/lastTimestamp/fps ports.

## CAN

- Transport: Slcan (serial CAN adapter) or CandleLight (USB-CAN); matching
  Protocol kind required.
- Send: send_can_frame tool (encoded via the protocol node's encode_can; frame
  has id/extended/data/direction).
- Read: get_can_frames returns recent frames plus load stats (fps, load_ratio).

## Logic Analyzer

- LogicDecode decodes UART/I2C/SPI from captured samples.
- get_logic_data returns recent samples and decoded events.

## Protocol Debugging Tips

- Without hardware, inject_bytes from a source node verifies the parsing chain
  (returns the number of downstream targets hit).
- get_raw_data shows actual raw TX/RX bytes (hex) — first stop for "no data".
- channels: protocol.channels (null = auto) decides how many ch0..chN outputs
  the ProtocolSource exposes.
