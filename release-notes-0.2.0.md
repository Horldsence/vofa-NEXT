# VOFA-NEXT Release Notes

## v0.2.0

This release is a **protocol & widget** release: all devices (transports and protocols) are now **widgets** living in the unified node graph, and every protocol is rebuilt on a **custom frame schema engine** — a protocol is just a frame schema (a list of blocks) shared by decoding and encoding, so user-defined frames get named ports and transport hot-update for free. The Command widget grows into a **multi-frame sender** with per-frame auto-send; the raw data view understands **byte-source channels** with scoped send targets; theming gains a full **CSS style theme** layer (with external URL loading and reduced-motion); and the app can now **update itself**, with stable/beta channels and an in-app release-notes dialog. On the reliability side, the raw data filter moved to a zero-IPC local incremental view (fixing the >10 MB/s freeze), CAN/logic/decoded lists are wired to backend filtered subscriptions, and the waveform gets proper pan gestures plus auto-set that finally works for sub-mV signals.

## ✨ New Features

### 1. Everything Is a Widget: Devices Unified into the Node Graph

- Transports and protocols are no longer special-cased "devices" — they are widgets in the same node graph as displays and solvers, with unified creation, configuration, and connection flows.
- The backend node crates were restructured around this model (frame decoder blocks, compile/eval pipelines, byte plan), and the frontend store, quick-start templates, and app export/import were migrated to the widget model.

### 2. Custom Frame Schema Engine

- A protocol is now defined as a **frame schema** — a block list shared by decoding (frame decode) and encoding (frame generation/sending). All existing protocol kinds (JustFloat / FireWater / RawData / Slcan / CandleLight / LogicDecode) are schema **presets**; users can define fully **custom** blocks.
- Blocks get **named ports**, so schema-defined fields wire directly into the node graph; transports support **hot-update** of their configuration without reconnecting.
- New `schema` model in the core crate (shared serde contract with the frontend) and a `schema_engine` in the protocol crate, including checksum algorithms (sum8 / xor8 / crc8 / crc16-Modbus / crc16-CCITT / crc32 / LRC / custom script).

### 3. Multi-Frame Command Sender

- The Command widget now holds **multiple frames** instead of a single block list: add, remove, and edit frames in a sidebar, each with its own send mode — manual or **per-frame auto-send timer**.
- `var_ref` blocks across all frames derive **named input ports** on the widget node (union, deduplicated), so graph edges can drive any frame field. Legacy single-frame configs are normalized transparently.

### 4. Raw Data Byte-Source Channels & Scoped Send

- Each raw data channel is classified as **decoder-node** (FrameDecoder raw bypass bytes), **byte-source** (raw RX/TX bytes of the upstream transport, traced through protocol nodes), or **numeric** — byte-source channels show the transport's actual wire bytes.
- The send panel in the raw data view sends to the **scoped transport target** of the selected channel, and new graphs get a sensible initial seed.

### 5. CSS Style Themes & Reduced Motion

- A new **CSS style theme** layer sits alongside color-token themes, with built-in **Default** and **Monet** CSS themes under `public/themes/`; custom CSS themes can be loaded from **external URLs**.
- The theme editor is now tabbed (Color Theme / CSS Style Theme), and a new **reduced-motion** accessibility toggle disables animations app-wide.

### 6. Auto Update with Release / Beta Channels

- The app checks for updates on startup (toggleable) and shows status in the **status bar**; the update dialog displays **release notes** and supports **skip this version**.
- Switchable **stable / beta** channel (follows the current version by default, switching asks for confirmation); cross-channel upgrades are allowed, downgrades are never offered. Download and install reuse tauri-plugin-updater with verified signatures.

### 7. Custom Baud Rate & Blur Validation

- The baud rate dropdown is now an **editable input with preset suggestions** — any baud rate can be entered (closes #3). Numeric fields keep a local text state and validate **on blur** instead of clamping per keystroke; the same pattern applies to the test-data sample rate.

### 8. Backend Filtered Subscriptions for CAN / Logic / Decoded Streams

- New unified filtered stream sources (`Filtered{Can,Logic,Decoded}StreamSource`) with `CanFrameFilter` / `LogicSampleFilter` / `DecodedEventFilter` in the core crates; a filtered subscription **replays full matching history** then goes incremental.
- The CAN frame list (ID / direction filter) and the decoded event list (protocol type filter) now run on local buffers fed by these backend filtered subscriptions instead of client-side array filtering.
- Shard activation fix: backlog now reports per-cursor unread bytes, so idle shards sleep instead of waking on stored history.

## 🐛 Fixes & Performance

- **Raw data freeze eliminated** (refs #8): the backend filtered subscription pushed a second full-rate stream over IPC (measured 2×16.6 MB/s base64 at >10 MB/s) with no backpressure, freezing the app for 10+ seconds when switching filters. Filtering now runs in a **local incremental view** over the existing frontend buffer — instant filter switch, zero extra IPC — with 13 new unit tests.
- **Data-path performance instrumentation**: rate-limited `perfLog` aggregation (msg/s, MB/s, queue depth) toggleable via localStorage, plus a fix for a subscribe-cancel race that leaked backend tasks.
- **Waveform pan & zoom**: left-drag keeps box-select; pan via **right-drag, shift+left-drag, or middle-drag** (with a one-time hint toast); the browser and app context menus are suppressed over the chart. Auto-set snaps vPerDiv to 1-2-5 steps across any decade and formats with SI prefixes (e.g. `20uV/div`), so sub-mV signals are no longer flattened.
- **ResizeObserver loops silenced**: all RO callbacks deferred to rAF; benign third-party loop errors filtered.
- **Structured backend errors**: backend errors serialize as a tagged enum for structured, actionable frontend guidance.
- Backend refactor: 8 oversized modules (500+ lines) split into focused subdirectories with tests extracted to `tests/` — no behavior change.

## 📦 Installers

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)

---

# VOFA-NEXT 发布说明

## v0.2.0

本次发布是 协议与控件 版本：所有设备（传输层与协议）统一为 节点图控件，协议全部重构在 自定义帧 Schema 引擎 之上——协议即一份帧 schema（块列表），解码与编码共用同一份定义，用户自定义帧天然拥有命名端口与传输热更新。命令发送控件升级为 多帧发送器，支持逐帧定时自动发送；原始数据视图理解 字节源通道 并支持定向发送；主题系统新增完整的 CSS 样式主题 层（支持外部 URL 加载与减弱动效）；应用现在可以 自更新，支持 stable/beta 通道与应用内发布说明对话框。可靠性方面：原始数据过滤改为零 IPC 的本地增量视图（修复 >10MB/s 卡死），CAN/逻辑/解码列表接入后端过滤订阅，波形获得完整的平移手势与适配亚毫伏信号的 auto-set。

## ✨ 新特性

### 1. 万物皆控件：设备统一进节点图

- 传输层与协议不再是特殊处理的"设备"——它们与显示、求解器一样成为节点图中的控件，创建、配置与连线流程完全统一。
- 后端 nodes crate 围绕此模型重构（帧解码块、编译/求值流水线、字节规划），前端 store、快速开始模板与应用导入导出同步迁移到控件模型。

### 2. 自定义帧 Schema 引擎

- 协议现在定义为一份 帧 schema——解码（帧解析）与编码（帧生成/发送）共用的块列表。所有现有协议 kind（JustFloat / FireWater / RawData / Slcan / CandleLight / LogicDecode）都是 schema 预设；用户可完全自定义块。
- 块拥有 命名端口，schema 定义的字段可直接接入节点图连线；传输层支持 热更新 配置，无需断开重连。
- core crate 新增 schema 模型（与前端共享 serde 契约），protocol crate 新增 schema_engine，内置校验算法（sum8 / xor8 / crc8 / crc16-Modbus / crc16-CCITT / crc32 / LRC / 自定义脚本）。

### 3. 多帧命令发送器

- 命令控件从单块列表升级为 多帧：侧边栏增删编辑帧，每帧独立发送模式——手动或 逐帧定时自动发送。
- 所有帧的 var_ref 块在控件节点上派生 命名输入端口（并集去重），节点图边可驱动任意帧字段；旧版单帧配置自动透明归一化。

### 4. 原始数据字节源通道与定向发送

- 原始数据每条通道分类为 解码节点（FrameDecoder raw 旁路字节）、字节源（经协议节点上溯到的上游传输原始收发字节流）或 数值——字节源通道展示传输线上真实字节。
- 原始数据视图的发送面板按所选通道 定向发送到对应传输，新建图获得合理的初始节点种子。

### 5. CSS 样式主题与减弱动效

- 新增 CSS 样式主题 层，与颜色令牌主题并存；内置 Default 与 Monet 两套 CSS 主题（位于 public/themes/），支持从 外部 URL 加载自定义 CSS 主题。
- 主题编辑器改为分页（颜色主题 / CSS 样式主题），新增 减弱动效 无障碍开关，一键禁用全应用动画。

### 6. 自动更新（Release / Beta 通道）

- 启动时检查更新（可关闭），状态栏显示更新状态；更新对话框展示 发布说明，支持 跳过此版本。
- 可切换 stable / beta 通道（默认跟随当前版本，切换需确认）；允许跨通道升级，永不提供降级。下载与安装复用 tauri-plugin-updater，签名校验。

### 7. 自定义波特率与失焦校验

- 波特率下拉框改为 可编辑输入框 + 预设建议，可输入任意波特率（closes #3）。数值字段保留本地文本状态、失焦时校验，不再逐键钳制；测试数据采样率字段采用同样模式。

### 8. CAN / 逻辑 / 解码流的后端过滤订阅

- 新增统一过滤流源（Filtered{Can,Logic,Decoded}StreamSource）与核心 crate 中的 CanFrameFilter / LogicSampleFilter / DecodedEventFilter；过滤订阅先 回放全部匹配历史 再增量推送。
- CAN 帧列表（ID / 方向过滤）与解码事件列表（协议类型过滤）改为由后端过滤订阅喂入本地缓冲，不再做前端数组过滤。
- 分片激活修复：积压量按游标未读字节统计，空闲分片不再因存量历史被唤醒。

## 🐛 修复与性能

- 原始数据卡死消除（refs #8）：后端过滤订阅经 IPC 推送了第二条全速率流（实测 >10MB/s 时 2×16.6MB/s base64）且无背压，切换过滤时应用卡死 10 秒以上。过滤改为在现有前端缓冲上做 本地增量视图——切换即时、零额外 IPC——并新增 13 个单元测试。
- 数据路径性能埋点：速率受限的 perfLog 聚合（msg/s、MB/s、队列深度），localStorage 开关；修复订阅取消竞态导致的后端任务泄漏。
- 波形平移与缩放：左键拖拽保持框选；右键拖拽、Shift+左键或中键 平移（带一次性提示 toast）；屏蔽图表上的浏览器与应用右键菜单。auto-set 按 1-2-5 步进跨数量级吸附 vPerDiv 并以 SI 前缀格式化（如 20uV/div），亚毫伏信号不再被压成一条直线。
- ResizeObserver 循环静音：所有 RO 回调延迟到 rAF；过滤第三方良性循环报错。
- 结构化后端错误：后端错误序列化为标记枚举，前端可给出结构化、可操作的指引。
- 后端重构：8 个超过 500 行的模块拆分为子目录，测试抽取到 tests/——无行为变化。

## 📦 安装包

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)
