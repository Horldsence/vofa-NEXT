# VOFA-NEXT Software & Core Concepts Overview

VOFA-NEXT is a data-acquisition and debugging host application (successor of
VOFA+) for embedded development with serial / TCP / UDP / CAN / logic-analyzer
scenarios. Core capabilities: device connection, protocol parsing, waveform
display, interactive control panels, CAN bus analysis, logic decoding.

## Dual-plane Data Architecture

- **Byte plane (global)**: an event-driven byte-stream network of Transport and
  Protocol nodes. Device bytes flow out of Transport.rx into Protocol.in;
  Protocol.out produces numeric channels.
- **Value plane (per tab)**: each control tab is an independently compiled node
  graph (DAG). A ProtocolSource references the latest frame of a global Protocol
  node and outputs ch0..chN channels for downstream widgets (waveform, gauge,
  ...). Evaluation runs on the backend-compiled CompiledGraph.

## Canvas & Nodes

- **Control tabs**: users create multiple tabs; each tab is a canvas holding
  widget nodes.
- **Global nodes**: Transport / Protocol nodes belong to no tab and render on
  every canvas. They form the byte plane.
- **Widget nodes**: input controls (Knob/Slider/Button/...), display widgets
  (Waveform/Gauge/...), and compute nodes (Math/Filter/FFT/FrameDecoder/
  Trigger/...) live inside a tab as widgets.
- **Edges**: byte edges (Vec<u8>, Transport→Protocol etc.) and value edges
  (f32, between widgets). On connect the backend compiles and derives port
  tables; compile failures keep the old graph and report an error.

## Software Operation Entrypoints (built-in AI tools)

- Graph read/write: get_workspace reads full canvas state; add_node /
  remove_node / update_node_config / connect_nodes / disconnect_edge /
  move_node edit incrementally (equivalent to manual editing: canvas updates
  live and changes enter undo history).
- Transports: list_serial_ports, connect_transport / disconnect_transport,
  send_bytes / send_string / send_can_frame.
- Data reads: get_graph_outputs (latest values of all output ports),
  get_recent_waveform / get_waveform_window, get_can_frames, get_logic_data,
  get_raw_data (raw TX/RX bytes), get_buffer_info, list_data_sources.
- Others: set_input_value drives input widgets; apply_template builds a
  template workspace in one step.

## Example Data Flow

TestData (simulated device) → Protocol(JustFloat) → Waveform: connect a byte
edge transport.rx → protocol.in, then a value edge protocol channel →
waveform.data; connect_transport and the waveform appears.
