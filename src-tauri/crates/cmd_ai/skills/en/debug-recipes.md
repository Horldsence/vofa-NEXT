# Device Debugging Playbook

## Task 1: Connect a device and show a waveform

1. list_serial_ports → confirm the port name.
2. get_workspace → understand current tabs/nodes.
3. If nodes are missing: add_node(type=transport, kind=Serial) → add_node(
   type=protocol, kind=JustFloat) → add_node(type=widget, kind=Waveform).
4. connect_nodes: transport.rx → protocol.in; protocol channel → waveform.data.
5. connect_transport(node_id=transport-xxx).
6. send_string / send_bytes to command the device (if any).
7. get_recent_waveform(source=protocol node id, count=100) to verify data; or
   get_graph_outputs for live port values.

For custom device protocols, add a FrameDecoder widget and describe the frame
with blocks; named fields become output ports.

## Task 2: Periodic control / acquisition

- Input control: add a Knob/Slider widget and wire it downstream; or just
  set_input_value(widget_id, value) to change a value instantly.
- Button press value: set_input_value works too (press_value).
- TextInput + TextOut writes strings back to a Transport's tx.

## Task 3: Hardware-less protocol debugging

1. Build Transport → Protocol → display chain (no device needed).
2. inject_bytes(source_node_id=transport id, data=[...]) simulates device bytes.
3. Verify with get_graph_outputs / get_recent_waveform.

## Task 4: Diagnosing "no data"

1. list_transports → is the transport Connected?
2. get_raw_data(source=transport id) → are bytes actually arriving (TX/RX)?
3. Bytes OK but no waveform → check wiring (get_workspace), protocol kind and
   channels.
4. get_buffer_info(source=protocol id) → is the buffer accumulating points?

## Task 5: CAN bus debugging

1. add_node(type=transport, kind=CandleLight or Slcan) → connect_transport.
2. get_can_frames(count=100) for recent frames + bus load.
3. send_can_frame(node_id=..., frame={id: 0x123, data: [1,2,3], direction: "tx"}).

## Task 6: Quick demo workspace

apply_template(template_id=...) applies a built-in template in one step (query
list_templates first) — ideal for demonstrating standard chains.

## General Working Conventions

- Read before write: get_workspace first; edit in small steps and verify each.
- Node ids are the primary key of every tool (transport-/protocol-/widget-
  prefixes). Never invent ids — take them from get_workspace etc.
- Compile failures keep the old graph and return the reason (cycle / port
  domain mismatch) — fix and retry.
