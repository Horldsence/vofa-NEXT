# Node & Widget Type Reference

## Transport nodes (global)

| kind | key params |
|---|---|
| Serial | port_name (query via list_serial_ports first), baud_rate, data_bits, parity, stop_bits, flow_control |
| Udp | local_addr/local_port (local), remote_addr/remote_port (peer) |
| TcpClient | host, port |
| TcpServer | listen_addr, listen_port |
| TestData | channels, sample_rate, signal (Sine/Square/Triangle/Sawtooth/Random/Dc/Chirp/Steps/Noise/MultiTone) — simulated source for hardware-less testing |
| Slcan | serial params + can_bitrate (lawicel adapter) |
| CandleLight | bus, address, can_bitrate, channel (canable etc. USB-CAN) |

Node id pattern: `transport-XXXXXXXX` (nanoid). Connection state via list_transports.

## Protocol nodes (global)

| kind | notes |
|---|---|
| JustFloat | binary float frame: N×f32 (little-endian) + tail 00 00 80 7F; channels: null = auto-detect |
| FireWater | ASCII frame: "1.23,4.56\n" comma-separated floats + newline |
| RawData | raw byte passthrough (no float parsing), consumed by RawData widget |
| Slcan / CandleLight | protocol layer for CAN transports (encode_can encodes CAN frames) |
| LogicDecode | logic-analyzer decoding protocol (decoder config for UART/I2C/SPI) |

Node id pattern: `protocol-XXXXXXXX`. Protocols support convertTo (secondary
conversion) and custom frame schema.

## Widgets (per tab)

- **Input**: Knob / Slider (`min/max/step/value`), Button (`pressValue/releaseValue`),
  Radio (`options: [{id,label,value}], selectedId`), Checkbox
  (`options: [{id,label,value}], selectedIds`; output is the sum of selected values),
  TextInput (send text). Numeric inputs keep one `value` output and changes apply via
  set_input_value immediately. Direct bindings select a Transport explicitly; Auto bindings
  also select the Protocol and channel.
- **Display**: Waveform, Gauge, NumberDisplay, LED (threshold color), PieChart,
  Image (rgb888 frames), Label, Spectrum, Model3D (trajectory/attitude),
  TableView, TextDisplay, RawData.
- **Compute**: Math (add/sub/mul/div/pow/mod/min/max/abs/sin/cos/tan/log/sqrt...,
  configurable inputCount), Filter (Lowpass/Highpass/Bandpass/Bandstop), FFT,
  IFFT, Str (string ops), FrameDecoder (custom frame blocks: header/field/
  checksum/tail; named fields become output ports), Trigger (rule matching:
  pattern → value/text), Command (frame builder: const/var-ref/checksum blocks),
  TextOut (send graph strings back to a Transport), Custom (JS widget).

Widget node id pattern: `widget-XXXXXXXX` (widget params.id equals node id).

## Wiring Rules

### Port domains — only same-domain ports may connect

Every port has a fixed domain; cross-domain wiring is rejected outright by the
backend compiler (the error names both ports and their domains):

| Domain | Typical ports |
|---|---|
| bytes | Transport.rx / Transport.tx, Protocol.in / Protocol.out, FrameDecoder.in, Command.loopbackOut |
| time | protocol ch0..chN, Math.result, Filter.result, IFFT.out0, FrameDecoder field ports, input-widget value outputs, display inputs (Waveform.CH0..N, Gauge.value, ...) |
| freq | FFT.spectrum (output) → only IFFT.spectrum / Spectrum.spectrum inputs |
| string | RawData-preset protocol str, TextInput.str, Trigger.text, Str string ports, TextDisplay.text, TextOut.text |

Typical chains: Transport.rx(bytes) → Protocol.in; Protocol.chN(time) →
Waveform/Gauge/Math; FFT.spectrum(freq) → IFFT or Spectrum; str(string) →
TextDisplay / Str / TextOut.

### Other rules

- RawData widget input ports are dynamically derived (`src:<source>:<handle>`);
  targetHandle is rewritten automatically on connect. RawData accepts bytes and
  time sources; only freq is rejected.
- Widgets can only connect to widgets of the same tab or to global nodes
  (cross-tab is rejected).
- Port names and domains: use each node's `ports` field in the get_workspace
  response (inputs/outputs carry domain annotations, same source as the
  canvas); read first, then connect.
- Wires are compile-checked by the backend: domain mismatch / cycles /
  non-existent ports fail with the real reason and no edge is created — fix
  the port or config per the message and retry.
- Always edit the graph via built-in tools (add_node/connect_nodes/...):
  canvas syncs live and edits are undoable.
