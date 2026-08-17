# VOFA-NEXT Release Notes

## v0.1.9

This release is a **raw data** release: the raw data view now distinguishes **receive (RX) vs send (TX)** — every byte sent from the send panel is recorded into the raw data stream and marked with a color-coded direction indicator (↓ green RX / ↑ blue TX); a new **filter toolbar** adds direction (All / RX / TX) and content **search**, with the filtering executed in the Rust backend — only matching chunks are pushed to the frontend, so even at 20 MB/s+ rates the view only receives what it actually displays; the raw data collector is rebuilt as a **cursor-based non-consuming buffer** so every subscriber reads shared history independently. The DSP graph gains **FFT/IFFT solver nodes** with time/frequency domain-typed port wiring (cross-domain connections are blocked, plus a new FFT quick-start template); on Windows the serial port list now shows **device descriptions**; and the built-in themes get a refreshed palette with a new window-background token.

## ✨ New Features

### 1. RX / TX Direction Tracking in the Raw Data View

- Data sent from the app (send panel raw/hex, send-string, widget value sends) is now captured into the raw data stream and marked as **TX**; everything received from the port is **RX**. Direction travels with every chunk (timestamp + direction + bytes) through the sharded stream, and the frontend ring buffer resolves the direction for any byte offset.
- Each row shows a color-coded direction indicator — ↓ green for RX, ↑ blue for TX — with a matching header gutter.

### 2. Backend-Side Filtering: Direction + Search

- New filter toolbar in the raw data view: **All / RX / TX** direction toggle and a **search box**.
- Search semantics: a search string consisting only of hex characters is parsed as a **hex byte pattern** (`31 32` and `3132` both work); anything else is matched as a **UTF-8 substring**; matching works across chunk boundaries (the tail of the previous matching chunk is kept).
- Filtering runs in the backend: new `subscribe_rawdata_filtered` / `subscribe_rawdata_node_filtered` sharded subscriptions push only matching chunks to the frontend — no frontend-side iteration, targeting >20 MB/s scenarios. The filtered view uses its own buffer fed by the filtered stream; switching direction/search re-subscribes and automatically pulls all matching history within the buffer capacity.
- Backend filter primitives for CAN / logic / decoded streams (`CanFrameFilter` with exact ID / mask / range, extended & RTR flags, direction and data-content pattern) are laid down as shared infrastructure for future per-stream filters.

### 3. Cursor-Based Non-Consuming Raw Data Collector

- The raw data collector no longer consumes data on drain: it is a fixed-capacity ring of chunks with an absolute `base_index`; every subscriber reads with its own cursor (`read_from` / `read_filtered_from`), so multiple views share the same history within capacity, and a stale cursor auto-aligns to `base_index` when data has been dropped.

### 4. FFT / IFFT Solver Nodes with Domain-Typed Port Wiring

- New **FFT (SpectrumSink)** and **IFFT** solver nodes in the node graph (30 FPS spectrum ticker, zero-phase IFFT reconstruction with ring playback); the Spectrum widget gains a frequency-domain `spectrum` input port fed by graph edges instead of a dropdown (legacy `sourceId` fallback kept).
- Ports are typed by domain (time / frequency): hover tooltips show the domain and connection validation blocks cross-domain edges (RawData dynamic ports resolve as time domain).
- Quick Start gains an **FFT spectrum analysis** template (FFT → Spectrum → IFFT).

### 5. Windows: Serial Port Device Descriptions

- The Windows serial port list now shows the device description / friendly name for each port (previously ID-only; closes #4), displayed in the port picker.

### 6. Theme Refresh & Window Background Token

- New `bgWindow` theme token; the body background now uses it instead of the editor background, and it participates in acrylic blending.
- Dark / Monet / Light palettes are rebalanced for deeper backgrounds and higher contrast (brighter primary/secondary text, sharper disabled/hover states, finer waveform grid and text).

### 7. About & Project Polish

- The About dialog uses the project icon, updates the author to **Horldsence** and the license to **GPLv2**; GitHub and Docs links corrected to Horldsence/vofa-NEXT (#7). Local state/cache directories are removed from the repo and ignored.

## 📦 Installers

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)

---

# VOFA-NEXT 发布说明

## v0.1.9

本次发布是 原始数据 版本：原始数据视图现在区分 接收 (RX) 与 发送 (TX)——发送面板发出的每一字节都会进入原始数据流并带颜色区分的方向标识（↓ 绿色 RX / ↑ 蓝色 TX）；新增 过滤工具栏（方向 全部/RX/TX + 内容搜索），且过滤在 Rust 后端完成——只把匹配的 chunk 推给前端，20MB/s 以上高码率下前端也只需显示什么收什么；原始数据收集器重构为 游标式非消费缓冲，各订阅者独立读取共享历史。DSP 节点图新增 FFT/IFFT 求解节点并带 时域/频域 类型化端口接线（跨域连线被拦截，另有新的 FFT 快速开始模板）；Windows 串口列表显示 设备描述；内置主题刷新配色并新增 窗口背景 令牌。

## ✨ 新特性

### 1. 原始数据 RX/TX 方向跟踪

- 应用发出的数据（发送面板原始/字符串、控件值发送）现在会写入原始数据流并标记为 TX；串口收到的数据为 RX。方向随每个 chunk 一起流转（时间戳 + 方向 + 字节，走统一分片流），前端环形缓冲按字节偏移解析方向。
- 每行显示颜色区分的方向指示符——↓ 绿色 RX / ↑ 蓝色 TX，表头对齐。

### 2. 后端过滤：方向 + 搜索

- 原始数据视图新增过滤工具栏：全部 / RX / TX 方向切换 + 搜索框。
- 搜索语义：只含十六进制字符的输入按 hex 字节模式解析（`31 32` 与 `3132` 均可）；其余按 UTF-8 子串匹配；支持跨 chunk 边界匹配（保留上一个匹配 chunk 的尾部字节）。
- 过滤在后端完成：新增 subscribe_rawdata_filtered / subscribe_rawdata_node_filtered 分片订阅，只推送匹配 chunk——前端无需遍历过滤，面向 20MB/s 以上场景。过滤视图使用独立缓冲接收过滤流；切换方向/搜索会重建订阅并自动拉取容量内的全部匹配历史。
- CAN / 逻辑 / 解码流的后端过滤原语（CanFrameFilter：精确 ID / 掩码 / 范围、扩展帧与远程帧标志、方向、数据内容模式）作为共享基础设施铺垫，供后续按流过滤使用。

### 3. 游标式非消费原始数据收集器

- 原始数据收集器不再"取出即消费"：改为固定容量环形 chunk 存储 + 绝对 base_index；每个订阅者用自己的游标读取（read_from / read_filtered_from），多个视图在容量内共享同一份历史；游标落后（数据已被丢弃）时自动对齐到 base_index。

### 4. FFT/IFFT 求解节点与时域/频域类型化端口

- 节点图新增 FFT（SpectrumSink）与 IFFT 求解节点（30 FPS 频谱 ticker、零相位 IFFT 重建 + 环形回放）；Spectrum 控件新增频域 spectrum 输入端口，数据源由节点图边提供（不再用下拉框，保留旧 sourceId 回退）。
- 端口按域（时域/频域）类型化：悬停 tooltip 显示所属域，连线校验拦截跨域连接（RawData 动态端口解析为时域）。
- 快速开始新增 FFT 频谱分析模板（FFT → Spectrum → IFFT）。

### 5. Windows：串口设备描述

- Windows 串口列表现在显示每个串口的设备描述 / 友好名称（此前只有 ID；closes #4），端口选择器同步展示。

### 6. 主题刷新与窗口背景令牌

- 新增 bgWindow 主题令牌；body 背景改用窗口背景（不再用编辑器背景），并参与亚克力混合。
- Dark / Monet / Light 三套配色重新平衡：背景更深、对比度更高（主/次文字更亮、禁用/悬停态更清晰、波形网格与文字更细腻）。

### 7. 关于与项目打磨

- 关于对话框改用项目图标，作者更新为 Horldsence，许可证改为 GPLv2；GitHub 与文档链接修正为 Horldsence/vofa-NEXT（#7）。本地状态/缓存目录移出仓库并加入忽略。

## 📦 安装包

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)
