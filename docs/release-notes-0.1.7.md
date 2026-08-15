# VOFA-NEXT Release Notes

## v0.1.7

This release is a performance & high-throughput release centered on the data pipeline: the RX path is now a two-stage data loop (feed → eval) with auto-scaling parallel parsing — when backlog builds up, the stream is split at frame boundaries and parsed by up to N parallel workers with order-preserving merge, falling back to sequential mode with zero loss when the backlog clears. All data streams (raw data / waveform / CAN / logic samples / decoded events) share a new unified sharded subscription framework that automatically activates extra channels under backlog and reorders batches by a group-level seq. The frontend coalesces high-frequency pushes into one store update per frame, raw-data chunks are now base64-encoded (~2.6× smaller JSON), a new Settings → Performance category exposes seven live-tunable pipeline parameters, and the status bar now clearly distinguishes real data loss (red drop alarm) from preview overflow (yellow badge) with explanatory guidance.

## ✨ New Features

### 1. Auto-Scaling Parallel Data Pipeline

- Two-stage data loop: RX feed and graph evaluation are now decoupled stages connected by bounded channels, so a slow evaluation no longer directly back-pressures parsing.
- Auto-scaling parallel feed: at low backlog the feed parses sequentially in a single worker (behavior identical to before); when backlog exceeds a threshold it automatically scales up to maxFeedWorkers parallel workers.
- Frame-aligned split: the byte stream is split at frame boundaries (ProtocolEngine::split_aligned) and each block is parsed by an independent stateless protocol engine; results are merged in block order — exactly equivalent to sequential parsing.
- Backlog subsides → automatic fallback to sequential mode; incomplete trailing bytes are fed back into the main engine's internal buffer — zero loss across mode switches.
- Parallel split is supported for frame-delimited protocols only (JustFloat / FireWater / Slcan / CandleLight); LogicDecoder (cross-byte state machine) and RawData (no frames) automatically fall back to sequential.
- Protocol parsing unified into a single-pass feed (FeedOutput) with linear-time parsers.

### 2. Unified Sharded Stream Subscription

- All data streams (global raw data / raw-data node bypass / waveform / CAN frames / logic samples / decoded events) now share a single subscription protocol and dispatch mechanism.
- Subscription groups: the first Channel creates a group and returns a group id; subsequent Channels join the same group (up to maxStreamShards), sharing one stream source instance and a group-level monotonic seq (assigned under the source lock, strictly consistent with drain order).
- Automatic concurrency: shard 0 is always active; shard i activates only when backlog ≥ threshold and sleeps again when it clears — a single channel costs nothing extra when it suffices, and multiple channels push in parallel when it doesn't.
- Ordering: incremental streams (raw data / CAN / logic / decoded events) are strictly re-ordered by seq on the frontend (with a stall-guard jump if a shard dies); snapshot streams (waveform) use "latest seq wins" and simply discard out-of-order stale snapshots.
- Adaptive rate: the push interval speeds up to 16 ms while data is flowing and backs off to 250 ms when idle.

### 3. High-Frequency Event Batching & Stream Efficiency

- RAF coalescing: graphOutputs / customInputs / spectrumResults channel pushes are written to a module-level cache and applied to the zustand store once per animation frame (~16 ms) instead of once per message — far fewer React renders at high data rates.
- Narrow widget subscriptions: useGraphInput / useGraphInputs now subscribe only to the upstream outputs a widget actually reads (useShallow per-port comparison); scalar widgets no longer re-render on every graph snapshot.
- Raw data chunks are now base64-encoded (bytes_b64) — roughly 2.6× smaller JSON payload, decoded once with atob.
- Removed no-op Tauri event listeners (transport:frames / can-frames / logic-samples / decoded-events); data flows exclusively through the sharded Channel path. Buffers gained monotonic version counters and incremental cursor reads (drain_from).

### 4. Pipeline Performance Settings

- New Settings → Performance category (Gauge icon) with seven live-tunable parameters, pushed to the backend immediately on change (set_pipeline_config); the backend does not persist them — the frontend replays the saved config on startup.
- Max Feed Workers (1–8): cap on workers parsing data in parallel; increase for high-rate sources.
- Feed Parallel Unit (1–256): message batch size dispatched to parallel workers each time.
- Min Worker Bytes (4–1024 KB): below this size data is not split across workers, avoiding scheduling overhead.
- Coalesce Max Messages (1–1024) / Coalesce Max Bytes (16–4096 KB): caps for a single coalesced push.
- Max Stream Shards (1–8): subscription stream shard cap; more shards improve push concurrency.
- Parse Channel Cap (16–4096): capacity of the feed mpsc channel.

### 5. Data Loss vs Preview Drop Indicators

- Real loss is now visible: the status bar shows a red pulsing dot with the per-window dropped message count (broadcast Lagged — bytes that never reached the parser, leaving waveform gaps); the 0 → >0 rising edge also fires a warning notification (30 s throttled).
- Preview drops stay distinct: the raw-data view's overflow badge is now yellow (not red) — those bytes were parsed normally and reached the waveform; only the preview did not retain them. This is not data loss.
- DroppedInfoPopover explains both cases ("What / Why / What to do"), with a one-click Open Settings shortcut that jumps to the right category (Data Cache vs Performance).

### 6. Responsive Status Bar

- The status bar now collapses in tiers driven by a ResizeObserver: full content at ≥960 px; rx/tx frame counters hidden below 960 px; transport/protocol labels hidden below 780 px; compact ↓/↑ byte counters and hidden buffer stats below 620 px. Connection state, alarms and the refresh button are always kept.

### 7. Graph Evaluation & Protocol Parsing Performance

- Compiled slot-based graph evaluation: each graph is compiled once into a flat op sequence over pre-allocated output slots, so per-frame evaluation is pure array reads/writes with zero string hashing; output value maps use FxHash (~3–5× faster lookups on the hot path).
- Linear-time frame-decoder drain and encode_frame helper fixes.

## 📦 Installers

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)

---

# VOFA-NEXT 发布说明

## v0.1.7

本次发布是围绕 数据管道 的 性能与高通量 版本：接收链路重构为 两段式数据循环（feed → eval），并在积压时 自动扩展并行解析（按帧边界切分、多 worker 并行、按块序合并，积压消退自动回落且零丢失）；所有数据流（原始数据 / 波形 / CAN / 逻辑采样 / 解码事件）统一接入新的 分片订阅框架（积压时自动多通道并行推送、按组级 seq 重组顺序）；前端对高频推送做 RAF 合批（每帧一次 store 更新），原始数据分片改为 base64 编码（JSON 体积缩小约 2.6 倍）；新增 设置→性能 分类，七个管道参数实时可调；状态栏新增 数据丢失 vs 预览丢弃 的明确区分（红色丢弃告警 vs 黄色预览徽标）并附说明引导。

## ✨ 新特性

### 1. 自动扩展的并行数据管道

- 两段式数据循环：RX 喂入与图求值解耦为独立阶段，通过有界通道连接——图求值变慢不再直接反压解析。
- 自动扩展并行喂入：积压低时单 worker 顺序解析（与之前行为完全一致）；积压超过阈值时自动扩展到最多 maxFeedWorkers 个并行 worker。
- 按帧边界切分（ProtocolEngine::split_aligned）：每块交给独立的空状态协议引擎解析，结果按块序合并——与顺序解析严格等价。
- 积压消退自动回落顺序模式；跨批次的不完整尾字节喂回主引擎内部缓冲——模式切换零丢失。
- 仅帧定界协议（JustFloat / FireWater / Slcan / CandleLight）支持并行切分；LogicDecoder（跨字节状态机）与 RawData（无帧）自动回退顺序解析。
- 协议解析统一为单遍 feed 输出（FeedOutput），各解析器线性时间。

### 2. 统一分片订阅流

- 所有数据流（全局原始数据 / RawData 节点旁路 / 波形 / CAN 帧 / 逻辑采样 / 解码事件）共用同一套订阅协议与分发机制。
- 分片组：首个 Channel 建组并返回组 id，后续 Channel 凭组 id 加入（最多 maxStreamShards 个），组内共享一个流源实例与组级单调 seq（在源锁内分配，与 drain 顺序严格一致）。
- 自动并发：shard 0 常活；shard i 仅在积压 ≥ 阈值时激活、消退自动休眠——单通道够用不浪费，不够自动多通道并行推送。
- 顺序保证：增量流（原始数据 / CAN / 逻辑 / 解码）前端按 seq 严格重组（含分片异常时的防卡死跳变保护）；快照流（波形）按"最新 seq 胜出"，乱序旧快照直接丢弃。
- 自适应速率：有数据时提速到 16ms，空闲时退避到 250ms。

### 3. 高频事件合批与流效率

- RAF 合批：graphOutputs / customInputs / spectrumResults 的 Channel 推送先写入模块级缓存，每动画帧（约 16ms）只更新一次 zustand store，而不是每条消息一次——高码率下大幅减少 React 渲染。
- 窄订阅：useGraphInput / useGraphInputs 只订阅本控件实际读取的上游输出（useShallow 逐端口比较），标量控件不再随每个图快照重渲染。
- 原始数据分片改为 base64 编码（bytes_b64）——JSON 体积缩小约 2.6 倍，一次 atob 解码。
- 移除无意义的 Tauri 事件监听（transport:frames / can-frames / logic-samples / decoded-events），数据只走分片 Channel 路径；缓冲新增单调版本号与增量游标读取（drain_from）。

### 4. 管道性能设置

- 新增 设置→性能 分类（Gauge 图标），七个参数变更即推送到后端（set_pipeline_config）；后端不持久化，前端启动时重放已保存配置。
- 并行解析 Worker 上限（1–8）：并行解析数据的 worker 数量上限，数据源速率高时可调大。
- 并行分发单元（1–256）：每次分发给并行 worker 的消息批大小。
- Worker 最小数据量（4–1024 KB）：低于该数据量时不拆分给多个 worker，避免调度开销。
- 合批最大消息数（1–1024）/ 合批最大字节数（16–4096 KB）：单次合批推送允许的上限。
- 流分片上限（1–8）：订阅流的最大分片数量，分片越多并发推送能力越强。
- 解析通道容量（16–4096）：feed mpsc 通道容量。

### 5. 数据丢失 vs 预览丢弃 指示

- 真实丢失可见：状态栏显示红色脉冲点 + 当前窗口丢弃条数（broadcast Lagged——未进入解析、波形存在缺口的字节）；0 → >0 上升沿触发警告通知（30 秒节流）。
- 预览丢弃区分开：原始数据视图的溢出徽标改为黄色（不再是红色）——这些字节已被正常解析并进入波形，只是预览未全部保留，不代表数据丢失。
- DroppedInfoPopover 说明弹层统一解释两类情况（是什么 / 为什么 / 怎么办），并提供"打开设置"一键跳转到对应分类（数据缓存 vs 性能）。

### 6. 状态栏分级收缩

- 状态栏按宽度分级收缩（ResizeObserver 驱动）：≥960px 显示全量；<960px 隐藏 rx/tx 帧数；<780px 再隐藏传输/协议文本标签；<620px 字节数改为 ↓/↑ 紧凑格式并隐藏缓冲统计。任意档位都保留连接状态、告警与刷新按钮。

### 7. 图求值与协议解析性能

- 编译期槽位图求值：每个图编译为预分配输出槽位上的平坦操作序列，逐帧求值纯数组读写、零字符串哈希；输出值表改用 FxHash（热路径查找快约 3–5 倍）。
- 帧解码器线性时间 drain，encode_frame 辅助函数修复。

## 📦 安装包

- macOS: `.dmg` — universal / arm64 / amd64
- Linux: `.deb` / `.AppImage` / `.rpm`
- Windows: `.msi` / `.exe` (NSIS)
