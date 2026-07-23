# VOFA-NEXT

[English](./README.md) | [简体中文](./README.zh-CN.md)

一个使用 Rust + Tauri 完全重构的下一代串口助手 —— 面向嵌入式调试、波形可视化、节点式数据流编排、CAN / 汽车诊断与逻辑分析。

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
    现代化串口调试工具，支持波形显示、节点编辑器、多协议解析、CAN 诊断与逻辑分析。
    <br />
    <a href="https://github.com/horldsence/vofa-next"><strong>查看项目仓库 »</strong></a>
    <br />
    <br />
    <a href="https://github.com/horldsence/vofa-next/issues">报告 Bug</a>
    ·
    <a href="https://github.com/horldsence/vofa-next/issues">提出新特性</a>
  </p>
</p>

![](./images/example.png)

## 目录

- [项目简介](#项目简介)
- [核心特性](#核心特性)
- [技术栈](#技术栈)
- [目录结构](#目录结构)
- [开发环境](#开发环境)
- [安装与运行](#安装与运行)
- [构建与打包](#构建与打包)
- [测试](#测试)
- [贡献指南](#贡献指南)
- [版本控制](#版本控制)
- [开源协议](#开源协议)
- [鸣谢](#鸣谢)

## 项目简介

VOFA-NEXT 是一款面向嵌入式调试场景的桌面串口助手。前端基于 React 19 + TypeScript + Vite，后端由 Rust + Tauri 2 提供高性能传输层 I/O、协议解析、节点图 DAG 引擎、DSP（FIR/IIR 滤波、FFT 频谱）以及汽车诊断协议（ISO-TP / UDS / OBD-II / J1939）。

应用支持 7 种传输方式、7 种协议引擎、基于 React Flow 的节点编辑器数据流编排、示波器式波形显示、CAN 帧 / 负载分析、逻辑分析仪（UART/I2C/SPI 解码），以及运行在沙箱 iframe 中的自定义 JS 控件系统。

## 核心特性

### 传输层

- **串口**（USB-CDC），可配置波特率 / 数据位 / 校验位 / 停止位 / 流控。
- **TCP 客户端** / **TCP 服务端**。
- **UDP**，独立配置本地与远端地址。
- **测试数据** —— 内置信号发生器（正弦 / 方波 / 三角 / 锯齿 / 随机 / 直流 / 扫频 / 阶梯 / 噪声 / 多频叠加），便于离线原型验证。
- **Slcan** —— 串口 CAN。
- **CandleLight** —— 原生 USB CAN 后端。
- 支持自动重连与连接状态通知。

### 协议引擎

- **JustFloat** & **FireWater** —— VOFA+ 协议，支持通道自动检测。
- **RawData** —— 原始字节流查看。
- **Slcan** / **CandleLight** —— CAN 帧解析。
- **LogicDecode** —— 从数字电平采样解码 UART / I2C / SPI 协议。
- **Diagnostic** —— ISO-TP / UDS / OBD-II / J1939 汽车诊断协议栈（基于 `libautomotive`）。

### 节点编辑器与数据流

- 基于 **React Flow** —— 从侧边栏拖拽控件到画布并连接数据流。
- 后端 **DAG 引擎**（`vofa-next-nodes`）将图编译为拓扑序，逐帧评估所有节点输出，含循环检测。
- 节点类型：`ChannelSource`、`Input`、`Math`、`Filter`、`SpectrumSink`、`FrameDecoder`、`Custom`（JS）、`Sink`。
- **算术节点**：加 / 减 / 乘 / 除 / 均值 / 最小 / 最大 / 绝对值 / 取反 / 平方 / 开方 / sin / cos / tan / log。
- **滤波器节点**：低通 / 高通 / 带通 / 带阻（FIR 系数或 IIR biquad），跨帧状态持久化。
- **SpectrumSink**：块运算 FFT，可选窗函数（Hann / Hamming / Blackman / Rect）与输出模式（Magnitude / Power / PSD / dB），由独立 30 FPS ticker 驱动。
- **FrameDecoder**：基于块的字节流解析器（帧头 / 长度 / ID / 字段 / 位域 / 校验 / 帧尾），支持通过 `match_id` 多帧分派与校验和验证。
- **Custom JS 节点**：用户 JavaScript 运行在沙箱 iframe 中，输出回传到后端图。

### 显示与控件

- **示波器式波形** —— 基于 uPlot，支持时基缩放、游标测量、Run/Stop 冻结、通道 Y 轴独立 / 共享模式、缩略图缩放、十字线、悬停采样点标记、游标吸附。
- **仪表 / LED / 数字显示 / 饼图 / 标签** —— 一眼读数。
- **图像查看器** —— 支持 RGB888 / RGB565 / Gray8 像素格式。
- **频谱图** —— 实时 FFT 可视化。
- **3D 模型查看器** —— 基于 Three.js / React Three Fiber。
- **CAN 帧列表 / CAN 发送器 / CAN 负载视图** —— 支持 CSV 导出与负载告警。
- **逻辑时序图** + 解码事件列表（UART/I2C/SPI）。
- **命令发送器**（含块编辑器）与**帧解码器**手动测试面板。
- **自定义控件编辑器** —— 基于 CodeMirror 6 的 JS 编辑器，实时预览。

### 体验与平台

- VSCode 风格布局：活动栏、侧边栏、可缩放面板、状态栏、多标签页。
- 原生菜单栏（macOS / Windows / Linux）与全局快捷键。
- **国际化** —— 通过 YAML 管理中文 / 英文界面文案。
- **设置面板** —— 通用 / 外观 / 编辑器 / 数据 / 串口 / 通知，通过 `tauri-plugin-store` 持久化。
- 自定义主题编辑器、引导向导、帮助中心、上下文提示。
- 透明窗口与亚克力 / 毛玻璃效果（macOS）。
- 通过 `tauri-plugin-notification` 的原生系统通知。
- 通过 `tauri-plugin-log` 的结构化日志（stdout / 日志目录 / webview）。

## 技术栈

### 前端

- [React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/) + [Vite 7](https://vitejs.dev/)
- [Tailwind CSS 4](https://tailwindcss.com/)（通过 `@tailwindcss/vite`）
- [React Flow](https://reactflow.dev/)（`@xyflow/react`）—— 节点编辑器
- [uPlot](https://github.com/leeoniya/uPlot) —— 波形图表
- [Three.js](https://threejs.org/) + [`@react-three/fiber`](https://github.com/pmndjs/react-three-fiber) —— 3D 查看器
- [CodeMirror 6](https://codemirror.net/) —— 自定义控件代码编辑器
- [TanStack React Virtual](https://tanstack.com/virtual) —— 虚拟列表
- [react-resizable-panels](https://github.com/bvaughn/react-resizable-panels) —— VSCode 风格布局
- [Zustand](https://github.com/pmndrs/zustand) —— 状态管理
- [lucide-react](https://lucide.dev/icons/) —— 图标
- [YAML](https://github.com/eemeli/yaml) —— 国际化

### 后端

- [Rust](https://www.rust-lang.org/) + [Tauri 2](https://tauri.app/)
- [Tokio](https://tokio.rs/) —— 异步运行时
- [Serde](https://serde.rs/) —— 序列化
- [parking_lot](https://github.com/Amanieu/parking_lot) —— 同步原语
- [window-vibrancy](https://github.com/tauri-apps/window-vibrancy) —— 亚克力 / mica 效果
- [libautomotive](https://crates.io/crates/libautomotive) —— UDS / OBD-II / J1939 诊断
- Tauri 插件：`tauri-plugin-log`、`tauri-plugin-notification`、`tauri-plugin-store`、`tauri-plugin-opener`

### 后端 Workspace Crate

| Crate | 职责 |
| --- | --- |
| `vofa-next-core` | 核心类型与配置（传输 / 协议 / 控件 / CAN / 逻辑 / 诊断） |
| `vofa-next-transport` | 传输层（串口 / TCP / UDP / Slcan / CandleLight / 测试数据）+ 管理器 |
| `vofa-next-protocol` | 协议引擎（JustFloat / FireWater / RawData / Slcan / CandleLight / LogicDecode） |
| `vofa-next-buffer` | 环形缓冲区、多通道 `DataBuffer`、原始数据收集器、节点图路由 |
| `vofa-next-nodes` | DAG 编译器与评估器（Math / Filter / SpectrumSink / FrameDecoder / Custom） |
| `vofa-next-dsp` | 数字信号处理（FIR/IIR 滤波器、FFT 频谱、窗函数） |
| `vofa-next-automotive` | 诊断引擎（ISO-TP / UDS / OBD-II / J1939），桥接 CAN 后端 |

## 目录结构

```
vofa-next/
├── scripts/                       # 构建与补丁脚本
│   ├── build.sh
│   ├── patch_cmdsender.cjs
│   ├── patch_remaining.cjs
│   └── patch_widgetnode.cjs
├── src/                           # 前端源码
│   ├── components/
│   │   ├── controls/              # 旋钮 / 按钮 / 滑块 / 单选 / 复选 / 标签
│   │   ├── displays/              # 波形 / 仪表 / LED / 饼图 / 频谱 /
│   │   │                          # 图像 / 数字显示 / 3D 模型 / 表格 /
│   │   │                          # CAN 视图 / CAN 发送 / CAN 负载 / 逻辑视图 /
│   │   │                          # 原始数据 / 命令发送 / 帧解码 / ...
│   │   ├── layout/                # 活动栏 / 侧边栏 / 控制面板 / 数据面板 /
│   │   │                          # 节点编辑器 / 状态栏 / 缓存使用率
│   │   ├── nodes/                 # React Flow 节点类型（ChannelSource / Widget）
│   │   ├── onboarding/            # 引导向导 / 帮助中心 / 引导层 / 上下文提示
│   │   ├── panels/
│   │   │   ├── transport/         # 串口 / UDP / TCP 客户端 / TCP 服务端 / 测试数据 /
│   │   │   │                      # Slcan / Candle 表单
│   │   │   ├── PortPicker.tsx
│   │   │   ├── ProtocolSection.tsx
│   │   │   ├── TransportConfigPanel.tsx
│   │   │   └── WidgetPalette.tsx
│   │   ├── ui/                    # 右键菜单 / 面板标签 / 工具栏按钮 / 控件卡片
│   │   ├── AboutModal.tsx
│   │   ├── CodeEditor.tsx
│   │   ├── CustomWidgetEditor.tsx
│   │   ├── NotificationToasts.tsx
│   │   ├── SettingsModal.tsx
│   │   └── ThemeEditor.tsx
│   ├── i18n/                      # i18n 加载器 + 语言包（en.yml / zh.yml）
│   ├── lib/                       # Tauri API / 缓冲区 / 订阅 / 工具
│   ├── settings/                  # 设置 schema、默认值、主题应用
│   ├── store/                     # Zustand store（连接 / 数据 / 图 / ... 分片）
│   ├── types/                     # TypeScript 类型（can / logic / transport / waveform / ...）
│   ├── App.tsx
│   └── main.tsx
├── src-tauri/                     # Tauri + Rust 后端
│   ├── crates/                    # Rust workspace（见上表）
│   ├── src/
│   │   ├── commands/              # Tauri 命令处理（transport/protocol/buffer/
│   │   │                          # graph/can/logic/can_load/frame_decoder/window/...）
│   │   ├── pipeline/              # data_loop / decoder_feed / graph_eval / spectrum_sync
│   │   ├── state/                 # AppState / ticker（图输出 / 自定义输入 / 频谱）
│   │   ├── subscription/          # 事件订阅管理器
│   │   ├── commands.rs
│   │   ├── menu.rs                # 原生菜单栏
│   │   ├── notify.rs
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── capabilities/default.json
│   ├── icons/                     # 应用图标（macOS / Windows / iOS / Android）
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── public/                        # 静态资源（tauri.svg / vite.svg）
├── images/                        # README 资源
├── .github/workflows/             # CI：build.yml / release.yml
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── index.html
└── README.md
```

## 开发环境

- [Node.js](https://nodejs.org/)（建议 LTS）
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)（stable）
- [Tauri 2 系统依赖](https://tauri.app/start/prerequisites/)
- 如需 CAN 诊断：兼容的 CAN 接口（Slcan 适配器或 CandleLight 兼容 USB 加密狗）

## 安装与运行

1. 克隆仓库

```sh
git clone https://github.com/horldsence/vofa-next.git
cd vofa-next
```

2. 安装前端依赖

```sh
pnpm install
```

3. 启动开发环境

```sh
pnpm tauri dev
```

应用默认会在 `http://localhost:1420` 加载前端，并启动 Tauri 桌面窗口。

## 构建与打包

生成生产环境前端资源并打包桌面应用：

```sh
pnpm tauri build
```

输出产物位于 `src-tauri/target/release/bundle/`。

跨平台构建示例（见 `scripts/build.sh`）：

```sh
# Windows 交叉编译
pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc

# macOS dmg 包
pnpm tauri build --bundles dmg
```

CI 工作流位于 `.github/workflows/`（`build.yml`、`release.yml`）。

## 测试

前端类型检查：

```sh
pnpm tsc --noEmit
```

前端生产构建：

```sh
pnpm build
```

后端单元测试（整个 workspace）：

```sh
cd src-tauri && cargo test
```

后端强制执行严格 Clippy lint（默认 deny `all` / `pedantic` / `nursery` / `cargo`，并合理放宽部分规则）—— 提交 PR 前请运行 `cargo clippy --workspace`。

## 贡献指南

贡献使开源社区成为一个学习、激励和创造的绝佳场所。你所作的任何贡献都**非常感谢**。

1. Fork 本项目
2. 创建功能分支：`git checkout -b feature/AmazingFeature`
3. 提交改动：`git commit -m 'Add some AmazingFeature'`
4. 推送到分支：`git push origin feature/AmazingFeature`
5. 提交 Pull Request

提交 PR 前请确保 `pnpm tsc --noEmit` 与 `cd src-tauri && cargo clippy --workspace && cargo test` 均通过。

## 版本控制

本项目使用 Git 进行版本管理。你可以在 [Releases](https://github.com/horldsence/vofa-next/releases) 页面查看可用版本。

## 开源协议

本项目基于 MIT 协议开源，详情请参阅 [LICENSE](./LICENSE)。

## 鸣谢

- [VOFA+](https://www.vofa.plus/) 提供的 FireWater / JustFloat 协议参考
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
