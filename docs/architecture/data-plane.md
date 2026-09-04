# 数据平面架构 (2026-09 重设计)

> 本文是数据平面 (byte plane / record plane / eval plane) 的设计权威说明,
> 对应 crate: `pipeline/data_plane`、`foundation/buffer_databuffer`、
> `transport/transport_core` (TestData)、`node/plane` (BytePlan)。

## 1. 问题背景 (为什么重设计)

旧管线在高采样率 (≥100k 帧/s) 下出现三类症状:

| 症状 | 旧机制 | 根因 |
|---|---|---|
| 2s 视窗只有最右 ~150ms 有数据 | 缓冲硬编码 100k 点 | 容量 ≠ 速率 × 窗口, 一批帧 (>100k) 就把环挤穿 |
| 曲线折叠成抛物线/台阶, 越跑越乱 | 4 套时间戳机制混用 | 协议 feed 墙钟 + 逐批实测线速 + eval 期到达摊开 + 到达回填互相交错, x 坐标不再对应采样相位 |
| `评估队列丢弃 4808300 帧` (2s) | 逐帧图求值与波形入库绑在同一批处理/同一把锁 | 显示数据可用性取决于求值吞吐; ×N 协议节点重复解析放大负载 |

## 2. 目标数据流

```
Transport (声明时钟: TestData=精确采样率 / 串口=波特率名义线速)
 ▼ 读任务: 合批 — 字节目标 与 "样本时长 ≤ 50ms" 取小 (C2, 防批失控)
 ▼ BytePlan 去重组: 同 (字节源, 协议配置等价) 只喂一个引擎 ── 一次解析 (C1)
 ▼ 采样时钟定案 (A1): ts = 锚点 + 逻辑推进, 域锁定, 到达时间不参与
 ▼┬─────────────────────┬──────────────────────────────┐
 │ 记录平面 (A3)         │ 求值平面                       │
 │ record_frames:       │ 有界队列 (帧/字节/批数预算)    │
 │ 原始帧无条件入库       │ 满则丢最旧整批 = 显式缺口 (A5)   │
 │ 分块锁 (16k/次)       │ eval_workers = min(cores,8)   │
 │ 端口预览降载发布       │ 缺口 → 滤波/触发/IFFT 复位+告警 │
 │ (≤512 点/批, A4)     │ 派生输出写独立时间轴 (带显式 ts) │
 ▼┴─────────────────────┴──────────────────────────────┤
 │ DataBuffer: L0 原始环 (容量 = 速率×窗口, 预算封顶, 自动整定)
 │   + min-max 金字塔 L1..Ln (×16/层, 固定 4096 块/层)
 │   溢出滚动覆盖计数 storage_overflow (显式降载)
 │   派生通道: 独立 Mutex, 独立时间轴, 与原始通道零锁竞争
 ▼ 快照: 窗口在 L0 覆盖内且点数 ≤ 4×预算 → 原始;
 │   否则自动选最小可用层 → 真实 min-max 包络 (旧数据降质不消失)
 ▼ WWB1 v2 全窗快照 (头含 storage_overflow / buffer_tier)
 ▼ 前端: 整窗替换渲染 + "缓冲已降载" 徽标 (WaveformChart)
```

## 3. 五条结构不变量

每类历史问题对应一条**结构上不可能再发生**的约束:

1. **单一时钟域** — 每源一个逻辑时钟, 首批锁定 (Source/Arrival 二选一),
   流内不切换; 帧时间戳 = 锚点 + 逻辑推进 (TestData: 帧×1/速率; 串口: 字节×位时间),
   到达时间只在首批锚点出现。hint 中途缺失沿用已锁定速率外推, 绝不落入到达域。
   TestData 的 broadcast `Lagged(n)` 按生成器消息契约换算为丢失帧数, 下一批逻辑
   时钟显式跨过缺口, 不把缺口两侧静默拼接。实现: `ProtocolNodeState::restamp_frames`。
   数值平面/显示端**不加工时间戳** (`spread_batch_timestamps` 已删除)。
2. **容量自洽** — L0 容量由 `速率 × 2.5s` 自动整定
   (`tune_buffer_capacity`, ±5% 抖动不重复), 受内存预算半额折算封顶
   (默认 memory_budget_mb 的一半, 绝对上限 16M 点)。超出部分由金字塔层承担,
   L0 滚动覆盖时 `storage_overflow` 计数 (WWB1 v2 元数据 + 前端徽标)。
3. **记录与求值解耦** — 原始波形入库 (`record_frames`) 在字节平面路由时完成,
   分块持锁; 求值积压/丢弃只影响派生通道与 source_frames。派生通道持有
   **独立时间轴 + 独立 Mutex** (`DerivedStore`)，求值只在取 `DerivedWriter`
   时短暂访问原始缓冲，不在整批图计算期间持有原始锁；派生写入按批获取锁。
   查询按时间精确对齐, 求值缺口表现为 NaN 断线而非错位缝合。
4. **一次解析多路分发** — 同 (字节源, 协议配置等价) 的 Protocol 节点共享一次
   解析/时钟/检测/旁路/记录, 帧以 `Arc<Vec<DataFrame>>` fan-out 到各节点
   评估队列 (零拷贝)。组员显示经 `buffer_aliases` 读代表缓冲。
   组在 `sync_protocol_states` 尾部重建 (`rebuild_route_groups`)。
5. **丢弃显式化** — 任何丢弃都可见: 求值丢批 → 队列缺口标记 (`pending_gap`) →
   eval 侧复位该源关联的滤波/触发/IFFT 状态并 `log::warn`;
   原始层覆盖 → `storage_overflow` 计数 → 2s 指标 warn + WWB1 头 + 前端徽标;
   预览降载 → 步长抽点恒含批尾最新值。**不存在静默缝合**。

## 4. 金字塔存储 (min-max tiers)

- 结构: L1..Ln 每层是上一层的 ×16 min-max 摘要。每块使用一对相同的公共
  块末时间戳, 各通道在该公共 X 位置保存 `(min,max)`；时间轴始终单调且通道
  严格对齐，绝不借用某个通道自己的极值时刻作为其他通道的 X 坐标。
  每层固定 4096 块 → 跨度按 ×16 几何增长
  (L1≈6.5 万样本, L2≈100 万, L3≈1600 万…), 内存每层 ~100KB/通道。
- 写入: L0 每 16 样本折叠一块, 级联向上, 摊销 O(1)/样本
  (`DataBuffer::maybe_fold`)。
- 查询 (`snapshot_window_budget` / `snapshot_all_budget`): 窗口完整落在 L0
  覆盖内且点数 ≤ 4×预算 → 原始快照; 否则自底向上找"覆盖窗口起点且
  条目数 ≤ 4×预算"的最小层。派生序列按时间精确对齐到层轴，但没有独立
  长期金字塔，不能把块末派生样本当成派生 min-max 包络保证。
- 停止态快照: `DataBuffer::clone` 深拷贝即冻结 (含金字塔/派生)。

## 5. 成本模型 (全样本严格求值的边界)

求值成本取决于实际图结构、订阅与派生写入，不能按核心数线性折算。
`eval_bench` 分别测串行、fork-join 与 SIMD；简单图并行调度可能比串行更慢。
无计算的 ProtocolSource→Sink 图只需批尾快照，标准 ch<n> 波形不重复记录派生环。
**原始波形显示不消耗该预算** (不变量 3)。挂接重滤波图组时, 若
实测每批服务时间超过该批样本的到达间隔，将持续出现"评估队列丢弃"告警 —
应分别检查平均承载与长尾停顿；增加 worker 不一定提高简单图吞吐，需要基准验证。
评估队列每源最多 140,000 帧、8 MiB Vec 分配容量估计和 256 批，三者同时约束。
单批自身超预算时拒绝并清空更旧积压，全部计入求值缺口；预算不含执行中的批次、
allocator 元数据或其他源，不是进程级内存上限。
消费者仅合并已经到达的同源独占小批，最多 16,384 帧，不额外等待；共享批不复制、
不越过。这样在摄入延迟后的集中补交中摊薄任务提交成本，仍保留逐帧计算和跨源轮询。

## 6. 诊断指标 (2s 窗口, `数据平面指标`)

`rx MB/s (消息/s) | ingest N 批, 均 Xms, 帧均, 产帧≈N/s
| eval M 批, 均 Yms, 消费帧数 | Lagged 丢弃 | 评估队列丢弃`，另有
`缓冲降载` warn (storage_overflow 增量)。异步平面分别计时、分别取批数作分母；
速率按实际报告间隔换算，产帧口径不叠加 fan-out 的消费/丢弃帧数。

判读:
- `评估队列丢弃 > 0` → 已超出积压预算，可能是持续承载不足、短时调度/服务停顿
  或单批过大；消费者读取缺口时复位有状态算子。通过 `eval_diagnostics` 的
  排队、blocking 调度和服务最大耗时区分等待环节，不能只看平均求值耗时;
- `缓冲降载` warn → 窗口超出原始层容量, 前端出现降载徽标;
- `Lagged 丢弃 > 0` → 解析前真实丢失；TestData 时间轴保留对应空洞，不再发生
  随丢包累计的整体时间压缩；数据本身无法恢复，仍需降低源速率或减少负载;
- `产帧≈` 与 TestData 名义速率的比值反映系统实际吞吐。生成器约 5ms 固定批量，
  保持截止相位补齐短时调度迟到；暂停期间不补账。

## 7. 关键位置索引

| 组件 | 位置 |
|---|---|
| 采样时钟域 | `data_plane/src/data_plane/mod.rs` (`SampleClock`, `restamp_frames`) |
| 记录/求值入口 | `data_plane/src/data_plane/frame_dispatch.rs` (`record_frames` / `eval_frames`) |
| 去重组路由 | `data_plane/src/data_plane/byte_router.rs` (`route_inner` 分组路径), `mod.rs` (`rebuild_route_groups`) |
| 缺口记账/复位 | `eval_queue.rs` (`pending_gap` 与批次一同出队), `graph_eval.rs` (`reset_source_transient_state`) |
| 金字塔 | `buffer_databuffer/src/tier.rs`, 预算查询 `window.rs` (`snapshot_window_budget`) |
| 派生独立时间轴 | `buffer_databuffer/src/derived.rs` (`DerivedStore`) |
| 容量整定 | `mod.rs` (`tune_buffer_capacity`), `data_buffer.rs` (`ensure_capacity_for_rate`) |
| WWB1 v2 | `cmd/display/src/waveform_binary.rs`, 前端 `src/lib/data/waveformProtocol.ts` |
| 降载徽标 | `src/components/displays/waveform/WaveformChart.tsx` |
| 合批上限/生成器补速 | `data_plane/src/data_plane/read_task.rs`, `transport_core/src/test_data.rs` |

## 8. 性能基准

完整基准矩阵、数据与未验证边界见 [波形链路性能审计](../performance/bench-audit-2026-09-04.md)。

长期波形与完整摄入路径使用 Criterion 独立量化：

```bash
cd src-tauri
cargo bench -p buffer_databuffer --bench waveform_pyramid_bench
cargo bench -p data_plane --bench ingest_bench
```

- `waveform_pyramid_write`: 4 通道 L0 滚动覆盖 + 多层级联写入，以 JustFloat
  20 B/帧折算吞吐；
- `overview_3200k_to_2000`: 320 万帧历史的全局概览预算查询；
- `detail_2s_700ksps_to_12000`: 700 kS/s 最近 2 秒主图预算查询；
- `justfloat_4ch_64kb_parse_record_enqueue`: 64 KB 合批的解码、采样时钟、原始记录
  与评估入队完整路径，持续门禁目标为 **>10 MB/s**。
