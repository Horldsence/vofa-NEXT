# VOFA-NEXT

[English](./README.md) | [简体中文](./README.zh-CN.md)

A next-generation serial assistant fully rebuilt with Rust + Tauri — built for embedded debugging, waveform visualization, node-based dataflow orchestration, CAN/automotive diagnostics, and logic analysis.

<!-- PROJECT SHIELDS -->

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![MIT License][license-shield]][license-url]

<!-- PROJECT LOGO -->
<br />

<p align="center">
  <a href="https://github.com/horldsence/vofa-next">
    <img src="icon.png" alt="Logo" width="80" height="80">
  </a>

  <h3 align="center">VOFA-NEXT</h3>
  <p align="center">
    A modern serial debugging tool with waveform display, node editor, multi-protocol parsing, CAN diagnostics, and logic analysis.
    <br />
    <a href="https://github.com/horldsence/vofa-next"><strong>Explore the repo »</strong></a>
    <br />
    <br />
    <a href="https://github.com/horldsence/vofa-next/issues">Report Bug</a>
    ·
    <a href="https://github.com/horldsence/vofa-next/issues">Request Feature</a>
  </p>
</p>

![](./images/example.png)
![](./images/example2.png)

## Table of Contents

- [Introduction](#introduction)
- [Core Features](#core-features)
- [Usage Guide](#usage-guide)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Prerequisites](#prerequisites)
- [Installation & Running](#installation--running)
- [Build & Packaging](#build--packaging)
- [Testing](#testing)
- [Contributing](#contributing)
- [Versioning](#versioning)
- [License](#license)
- [Acknowledgements](#acknowledgements)

## Introduction

VOFA-NEXT is a desktop serial assistant designed for embedded debugging scenarios. The frontend is built with React 19 + TypeScript + Vite, while the backend is powered by Rust + Tauri 2 to deliver high-performance transport I/O, protocol parsing, a node-graph DAG engine, DSP (FIR/IIR filters, FFT spectrum), and automotive diagnostic protocols (ISO-TP / UDS / OBD-II / J1939).

The app supports 7 transport types, 7 protocol engines, a React Flow-based node editor for dataflow orchestration, oscilloscope-style waveform display, CAN frame / load analysis, logic analyzer with UART/I2C/SPI decoding, and a custom JS widget system running in sandboxed iframes.

The UI features a **dock-style window layout**: control canvases and data views are presented as splittable, mergeable, dockable cards with multi-tab switching, and the layout is persisted automatically.

## Core Features

### Transports

- **Serial** (USB-CDC) with configurable baud rate / data bits / parity / stop bits / flow control.
- **TCP Client** / **TCP Server**.
- **UDP** with independent local & remote addresses.
- **Test Data** — built-in signal generator (Sine / Square / Triangle / Sawtooth / Random / DC / Chirp / Steps / Noise / MultiTone), ideal for offline prototyping.
- **Slcan** — CAN over serial.
- **CandleLight** — native USB CAN backend.
- Auto-reconnect and connection-state notifications.

### Protocol Engines

- **JustFloat** & **FireWater** — VOFA+ protocols with automatic channel detection.
- **RawData** — raw byte stream inspection.
- **Slcan** / **CandleLight** — CAN frame parsing.
- **LogicDecode** — UART / I2C / SPI protocol decoding from sampled digital levels.
- **Diagnostic** — ISO-TP / UDS / OBD-II / J1939 automotive diagnostic stack (powered by `libautomotive`).

### Node Editor & Dataflow

- Built on **React Flow** — drag widgets from the sidebar onto the canvas and wire up dataflows.
- Backend **DAG engine** (`node_engine`) compiles the graph into a topological order and evaluates all node outputs per frame, with cycle detection.
- Node kinds: `ChannelSource`, `Input`, `Math`, `Filter`, `SpectrumSink`, `FrameDecoder`, `Custom` (JS), `Sink`.
- **Math nodes**: Add / Sub / Mul / Div / Avg / Min / Max / Abs / Neg / Square / Sqrt / Sin / Cos / Tan / Log.
- **Filter nodes**: Lowpass / Highpass / Bandpass / Bandstop (FIR coefficients or IIR biquad), with cross-frame state persistence.
- **SpectrumSink**: block-based FFT with selectable window (Hann / Hamming / Blackman / Rect) and output modes (Magnitude / Power / PSD / dB), driven by an independent 30 FPS ticker.
- **FrameDecoder**: block-based byte-stream parser (Header / Length / Id / Field / Bitfield / Checksum / Tail) with multi-frame dispatch via `match_id` and checksum validation.
- **Custom JS nodes**: user JavaScript runs in a sandboxed iframe; outputs are posted back to the backend graph.

### Operation History · Undo / Redo / Time Travel

- **Snapshot-based undo stack** — canvas actions (node add/remove, wiring, widget config, tab management) are recorded automatically onto a timeline; session-scoped, up to 200 steps.
- **Shortcuts** — `Ctrl+Z` to undo, `Ctrl+Y` to redo; continuous gestures such as node dragging or slider tweaks coalesce into a single entry.
- **Operation History panel** — timeline list with newest first and badges colored by node kind (edges show two-tone endpoint dots); clicking any entry jumps straight back to that moment (snapshot rollback). The current entry is highlighted, and entries above it appear grayed out as "Undone · redoable" — click them to redo.
- **Branch semantics** — editing after an undo discards the redo branch as usual; importing a backup or applying a template rebases the timeline onto a fresh baseline.
- **Panel entry** — "Open Operation History" in the "＋" menu of the data card title bar; also practiced hands-on in the guided tour.

### Displays & Widgets

- **Oscilloscope-style waveform** — powered by uPlot, with timebase zoom, cursor measurements, Run/Stop freeze, per-channel independent / shared Y-axis, thumbnail zoom, crosshair, hover-point markers, and cursor snap.
- **Gauge / LED / NumberDisplay / PieChart / Label** — at-a-glance readouts.
- **Image viewer** — RGB888 / RGB565 / Gray8 pixel formats.
- **Spectrum chart** — real-time FFT visualization.
- **3D model viewer** — powered by Three.js / React Three Fiber.
- **CAN frame list / CAN sender / CAN load view** — with CSV export and load alarms.
- **Logic timing chart** + decoded event list (UART/I2C/SPI).
- **Command sender** (with block editor) and **frame decoder** manual test panel.
- **Raw data view** — standalone widget with grid / line grouping, HEX / ASCII representation, per-port channel switching, timestamp / offset display, and send panel.
- **Custom widget editor** — CodeMirror 6-powered JS editor with live preview.

### Window Organization & Layout

- **Activity bar** — VSCode-style icon rail on the left, one-click switching of the sidebar view (Data Interface / Protocol Engine / Widget Palette).
- **Dockable sidebar** — docks to the left or right edge of the window; drag its title bar to the window edge to switch sides, and show / hide it anytime.
- **Dock-style center area** — control canvases and data views live in splittable / mergeable cards:
  - Drag a tab onto another card's title bar → merge it into that card's tabs;
  - Drag a tab to a card's edge → split it into a standalone panel;
  - Drag an empty area of the title bar → move the whole card next to another card;
  - Drag a tab / card to the four edges of the page → dock as a full row / column strip.
  - The layout tree and card sizes persist automatically (`vofa-dock`) and survive restarts.
- **Dual tab system** — control-canvas tabs (node editor) and data tabs (waveform / raw data / CAN / logic analyzer, etc.) can be mixed and arranged freely.
- **Status bar** — connection state, transport / protocol, RX / TX bytes and frames, CAN load, and buffer usage at a glance.
- **Multiple tabs** — each tab can be named, renamed, duplicated, and closed independently.

### UX & Platform

- VSCode-style layout: activity bar, sidebar, resizable panels, status bar, multiple tabs.
- Native menu bar (macOS / Windows / Linux) and global shortcuts (`Ctrl+,` for settings; menu items for new / close tab and toggle sidebar).
- **i18n** — Chinese / English UI copy managed via YAML.
- **Settings modal** — general / appearance / editor / data / serial / notifications, persisted via `tauri-plugin-store`.
- **Full-app config export / import** — back up settings, protocol, transport, widgets, node graph, data tabs and view preferences to a single JSON file via system file dialogs, for easy migration.
- Custom theme editor, onboarding wizard, help center, and contextual hints.
- Transparent window with acrylic / vibrancy effect (macOS).
- Native OS notifications via `tauri-plugin-notification`.
- Structured logging via `tauri-plugin-log` (stdout / log dir / webview).

## Usage Guide

### Quick Start

1. **Configure the transport** — click the Data Interface icon in the left activity bar, choose a transport (Serial / TCP / UDP / Test Data / Slcan / CandleLight) in the sidebar, fill in the parameters, then click **Connect**.
2. **Select the protocol** — click the Protocol Engine icon and pick the protocol matching your device (JustFloat / FireWater / RawData / Slcan / CandleLight / LogicDecode).
3. **Build a dataflow** — click the Widget icon to open the Widget Palette, drag widgets onto the canvas, and wire them to channel sources or upstream widgets.
4. **View data** — display widgets such as Waveform automatically create matching data tabs; you can also click the "＋" in the data card's title bar to add CAN Frames, Logic Analyzer, and other tabs manually.

### Window Organization

The default layout looks like this (all panels can be freely rearranged):

```
┌──────────┬──────────────────────────────────────────────┐
│          │  Control canvas card (tab bar + node editor)  │
│ Activity ├──────────────────────────────────────────────┤
│  Bar     │  Data view card (Waveform / RawData / CAN /   │
│          │  Logic / ...)                                 │
│          ├──────────────────────────────────────────────┤
│          │  Status bar: connection · transport/protocol  │
│          │  · RX/TX · load                               │
└──────────┴──────────────────────────────────────────────┘
```

- **Activity bar** (leftmost rail) — click an icon to switch the sidebar view; the bottom icons open the help center, About, and Settings.
- **Sidebar** (left by default) — shows the configuration panel for the current view; drag its title bar to the window's left / right edge to switch the dock side; toggle visibility via the activity bar icons or the "Toggle Sidebar" menu item.
- **Dock center area** — made of cards, each hosting one or more tabs:
  - **Merge**: drag a tab onto another card's title bar and release to merge it into that card.
  - **Split**: drag a tab to a card's edge (top / bottom / left / right) and release to split it into a standalone panel.
  - **Move card**: drag the empty area of the title bar to move the whole card next to another card.
  - **Page docking**: drag a tab / card to any edge of the window (center area) to dock it as a full row or column strip.
  - **Resize**: drag the separator between cards to change sizes; all sizes and the layout are saved automatically and restored on restart.
- **Status bar** (bottom) — connection state, current transport & protocol, RX / TX bytes and frames, CAN load alarm, and buffer usage; refresh button on the right.

### Tab Management

- **Control-canvas tabs** — each tab is an independent node-editor canvas. Click "＋" in the title bar to create a new one; right-click a tab to rename / duplicate / close / close others; double-click the tab label to rename it directly.
- **Data tabs** — Waveform, Raw Data, Pie Chart, Image, 3D Model, Spectrum, Command Sender, CAN Frames, Logic Analyzer, Table View, Frame Decoder, etc. The "＋" in the data card's title bar adds CAN Frames and Logic Analyzer tabs.
- Tabs can be freely dragged between cards to merge / split, and closed via the context menu.

### Common Actions

- **Settings**: `Ctrl+,` / `Cmd+,` or the gear icon in the activity bar; full-app config export / import is available (backup to a single JSON file).
- **Undo & time travel**: `Ctrl+Z` to undo, `Ctrl+Y` to redo; clicking any entry in the Operation History panel jumps back to that moment (see Core Features).
- **Refresh ports**: the refresh button in the status bar or the context menu.
- **Help & onboarding**: the help icon at the bottom of the activity bar opens the Help Center anytime; a guided tour appears on first launch (can be disabled in Settings).

## Tech Stack

### Frontend

- [React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/) + [Vite 7](https://vitejs.dev/)
- [Tailwind CSS 4](https://tailwindcss.com/) (via `@tailwindcss/vite`)
- [React Flow](https://reactflow.dev/) (`@xyflow/react`) — node editor
- [uPlot](https://github.com/leeoniya/uPlot) — waveform charts
- [Three.js](https://threejs.org/) + [`@react-three/fiber`](https://github.com/pmndjs/react-three-fiber) — 3D viewer
- [CodeMirror 6](https://codemirror.net/) — custom widget code editor
- [TanStack React Virtual](https://tanstack.com/virtual) — virtualized lists
- [react-resizable-panels](https://github.com/bvaughn/react-resizable-panels) — VSCode-style layout
- [Zustand](https://github.com/pmndrs/zustand) — state management
- [lucide-react](https://lucide.dev/icons/) — icons
- [YAML](https://github.com/eemeli/yaml) — i18n

### Backend

- [Rust](https://www.rust-lang.org/) + [Tauri 2](https://tauri.app/)
- [Tokio](https://tokio.rs/) — async runtime
- [Serde](https://serde.rs/) — serialization
- [parking_lot](https://github.com/Amanieu/parking_lot) — synchronization
- [window-vibrancy](https://github.com/tauri-apps/window-vibrancy) — acrylic / mica effects
- [libautomotive](https://crates.io/crates/libautomotive) — UDS / OBD-II / J1939 diagnostics
- Tauri plugins: `tauri-plugin-log`, `tauri-plugin-notification`, `tauri-plugin-store`, `tauri-plugin-opener`

### Backend Workspace Crates

| Crate | Responsibility |
| --- | --- |
| `vofa_core` | Core types & config (transport / widget / pipeline configs, error types) |
| `schema_types` / `schema_engine` | Protocol frame schema types & schema-driven protocol engine |
| `can_types` / `logic_types` / `logic_decoder` / `diagnostic` | CAN / logic / diagnostics types & decoders |
| `transport_core` / `transport_serial` / `transport_net` / `transport_can_bridge` | Transport layer (serial / TCP / UDP / Slcan / CandleLight / test data) |
| `protocol_engine` / `protocol_float` / `protocol_can_bridge` | Protocol engines (JustFloat / FireWater / RawData / Slcan / CandleLight / LogicDecode) |
| `buffer_ring` / `buffer_databuffer` / `buffer_raw` / `buffer_graph` | Ring buffer, multi-channel `DataBuffer`, raw-data collector, graph routing |
| `node_kind` / `node_engine` / `node_frame_decoder` / `node_trigger` | DAG node definitions, compiler & evaluator, frame decoder, trigger matching |
| `dsp_window` / `dsp_fft` / `dsp_filter` | Digital signal processing (window functions, FFT spectrum, FIR/IIR filters) |
| `automotive_isotp` / `automotive_can` / `automotive_diag` | Diagnostic engine (ISO-TP / UDS / OBD-II / J1939) bridging CAN backends |
| `pipeline_data_plane` / `pipeline_stream` / `pipeline_dispatcher` / `subscription` | Data plane: byte routing, chunked stream dispatch, subscription registry |
| `app_state` / `notify_events` / `menu_shell` / `update_flow` | App state & tickers, frontend event contracts & notifications, menu, updater |
| `cmd_*` (7 crates) | Tauri commands (buffer / can_load / can_transport / debug / graph / pipeline / rawdata) |

## Project Structure

```
vofa-next/
├── scripts/                       # Build scripts
│   └── build.sh
├── src/                           # Frontend source
│   ├── components/
│   │   ├── controls/              # Knob / Button / Slider / Radio / Checkbox / Label
│   │   ├── displays/              # Waveform / Gauge / LED / PieChart / Spectrum /
│   │   │                          # Image / NumberDisplay / Model3D / TableView /
│   │   │                          # CanView / CanSender / CanLoadView / LogicView /
│   │   │                          # RawDataView / CommandSender / FrameDecoder /
│   │   │                          # OperationHistory / ...
│   │   ├── layout/                # ActivityBar / Sidebar / DockLayout /
│   │   │                          # DockCardFrame / DataTabContent / NodeEditor /
│   │   │                          # StatusBar / BufferUsageStats / CanLoadAlarm
│   │   ├── nodes/                 # React Flow node types (ChannelSource / Widget)
│   │   ├── onboarding/            # OnboardingWizard / HelpCenter / Tour / ContextualHint
│   │   ├── panels/
│   │   │   ├── transport/         # Serial / Udp / TcpClient / TcpServer / TestData /
│   │   │   │                      # Slcan / Candle forms
│   │   │   ├── PortPicker.tsx
│   │   │   ├── ProtocolSection.tsx
│   │   │   ├── TransportConfigPanel.tsx
│   │   │   └── WidgetPalette.tsx
│   │   ├── ui/                    # ContextMenu / PanelTabs / ToolbarIconButton /
│   │   │                          # WidgetCard / SlidingPill / SnapDropOverlay /
│   │   │                          # AnimatedSwitch
│   │   ├── AboutModal.tsx
│   │   ├── CodeEditor.tsx
│   │   ├── CustomWidgetEditor.tsx
│   │   ├── NotificationToasts.tsx
│   │   ├── SettingsModal.tsx
│   │   └── ThemeEditor.tsx
│   ├── i18n/                      # i18n loader + locales (en.yml / zh.yml)
│   ├── lib/                       # Tauri API / buffers / subscriptions / utils / config export
│   ├── settings/                  # Settings schema, defaults, theme application
│   ├── store/                     # Zustand stores (connection / data / graph / tabs /
│   │                              # dock layout / waveform scope / ... slices)
│   ├── types/                     # TypeScript types (can / logic / transport / waveform / ...)
│   ├── App.tsx                    # Top-level layout orchestration (activity bar + sidebar
│   │                              # + dock area + status bar)
│   └── main.tsx
├── src-tauri/                     # Tauri + Rust backend
│   ├── crates/                    # Rust workspace (see table above)
│   ├── src/
│   │   ├── commands/              # Tauri command handlers (transport/protocol/buffer/
│   │   │                          # graph/can/logic/can_load/frame_decoder/window/...)
│   │   ├── pipeline/              # data_loop / decoder_feed / graph_eval / spectrum_sync
│   │   ├── state/                 # AppState / tickers (graph output / custom input / spectrum)
│   │   ├── subscription/          # Event subscription manager
│   │   ├── commands.rs
│   │   ├── menu.rs                # Native menu bar
│   │   ├── notify.rs
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── capabilities/default.json
│   ├── icons/                     # App icons (macOS / Windows / iOS / Android)
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── public/                        # Static assets (tauri.svg / vite.svg)
├── images/                        # README assets
├── .github/workflows/             # CI: build.yml / release.yml
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── index.html
└── README.md
```

## Prerequisites

- [Node.js](https://nodejs.org/) (LTS recommended)
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Tauri 2 system dependencies](https://tauri.app/start/prerequisites/)
- For CAN diagnostics: a compatible CAN interface (Slcan adapter or CandleLight-compatible USB dongle)

## Installation & Running

1. Clone the repository

```sh
git clone https://github.com/horldsence/vofa-next.git
cd vofa-next
```

2. Install frontend dependencies

```sh
pnpm install
```

3. Start the dev environment

```sh
pnpm tauri dev
```

The app loads the frontend at `http://localhost:1420` by default and launches a Tauri desktop window.

## Build & Packaging

Build production frontend assets and package the desktop app:

```sh
pnpm tauri build
```

Artifacts are output to `src-tauri/target/release/bundle/`.

Cross-platform build examples (see `scripts/build.sh`):

```sh
# Windows cross-compilation
pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc

# macOS dmg package
pnpm tauri build --bundles dmg
```

CI workflows are provided in `.github/workflows/` (`build.yml`, `release.yml`).

## Testing

Frontend type check:

```sh
pnpm tsc --noEmit
```

Frontend production build:

```sh
pnpm build
```

Backend unit tests (workspace-wide):

```sh
cd src-tauri && cargo test
```

The backend enforces strict Clippy lints (`all` / `pedantic` / `nursery` / `cargo` denied by default with curated allows) — run `cargo clippy --workspace` before submitting PRs.

## Contributing

Contributions are what make the open-source community such a great place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

1. Fork this project
2. Create a feature branch: `git checkout -b feature/AmazingFeature`
3. Commit your changes: `git commit -m 'Add some AmazingFeature'`
4. Push to the branch: `git push origin feature/AmazingFeature`
5. Open a Pull Request

Please make sure `pnpm tsc --noEmit` and `cd src-tauri && cargo clippy --workspace && cargo test` pass before opening a PR.

## Versioning

This project is managed with Git. Available releases can be found on the [Releases](https://github.com/horldsence/vofa-next/releases) page.

## License

This project is licensed under the MIT License — see [LICENSE](./LICENSE) for details.

## Acknowledgements

- [VOFA+](https://www.vofa.plus/) for the FireWater / JustFloat protocol references
- [Tauri](https://tauri.app/)
- [React Flow](https://reactflow.dev/)
- [uPlot](https://github.com/leeoniya/uPlot)
- [Three.js](https://threejs.org/) / [React Three Fiber](https://github.com/pmndjs/react-three-fiber)
- [CodeMirror](https://codemirror.net/)
- [Tailwind CSS](https://tailwindcss.com/)
- [lucide-react](https://lucide.dev/)
- [libautomotive](https://crates.io/crates/libautomotive)

<!-- links -->
[contributors-shield]: https://img.shields.io/github/contributors/horldsence/vofa-next.svg?style=flat-square
[contributors-url]: https://github.com/horldsence/vofa-next/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/horldsence/vofa-next.svg?style=flat-square
[forks-url]: https://github.com/horldsence/vofa-next/network/members
[stars-shield]: https://img.shields.io/github/stars/horldsence/vofa-next.svg?style=flat-square
[stars-url]: https://github.com/horldsence/vofa-next/stargazers
[issues-shield]: https://img.shields.io/github/issues/horldsence/vofa-next.svg?style=flat-square
[issues-url]: https://github.com/horldsence/vofa-next/issues
[license-shield]: https://img.shields.io/github/license/horldsence/vofa-next.svg?style=flat-square
[license-url]: https://github.com/horldsence/vofa-next/blob/master/LICENSE
