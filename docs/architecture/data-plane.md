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
 │ record_frames:       │ 有界队列 (8 批/源)             │
 │ 原始帧无条件入库       │ 满则丢最旧整批 = 显式缺口 (A5)   │
 │ 分块锁 (16k/次)       │ eval_workers = min(cores,8)   │
 │ 端口预览降载发布       │ 缺口 → 滤波/触发/IFFT 复位+告警 │
 │ (≤512 点/批, A4)     │ 派生输出写独立时间轴 (带显式 ts) │
 ▼┴─────────────────────┴──────────────────────────────┤
 │ DataBuffer: L0 原始环 (容量 = 速率×窗口, 预算封顶, 自动整定)
 │   + min-max 金字塔 L1..Ln (×16/层, 固定 4096 条目/层)
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
   实现: `ProtocolNodeState::restamp_frames`。数值平面/显示端**不加工时间戳**
   (`spread_batch_timestamps` 已删除)。
2. **容量自洽** — L0 容量由 `速率 × 2.5s` 自动整定
   (`tune_buffer_capacity`, ±5% 抖动不重复), 受内存预算半额折算封顶
   (默认 memory_budget_mb 的一半, 绝对上限 16M 点)。超出部分由金字塔层承担,
   L0 滚动覆盖时 `storage_overflow` 计数 (WWB1 v2 元数据 + 前端徽标)。
3. **记录与求值解耦** — 原始波形入库 (`record_frames`) 在字节平面路由时完成,
   分块持锁; 求值积压/丢弃只影响派生通道与 source_frames。派生通道持有
   **独立时间轴 + 独立 Mutex** (`DerivedStore`), 与原始通道零锁竞争;
   查询按时间精确对齐, 求值缺口表现为 NaN 断线而非错位缝合。
4. **一次解析多路分发** — 同 (字节源, 协议配置等价) 的 Protocol 节点共享一次
   解析/时钟/检测/旁路/记录, 帧以 `Arc<Vec<DataFrame>>` fan-out 到各节点
   评估队列 (零拷贝)。组员显示经 `buffer_aliases` 读代表缓冲。
   组在 `sync_protocol_states` 尾部重建 (`rebuild_route_groups`)。
5. **丢弃显式化** — 任何丢弃都可见: 求值丢批 → 缺口记账 (`eval_gaps`) →
   eval 侧复位该源关联的滤波/触发/IFFT 状态并 `log::warn`;
   原始层覆盖 → `storage_overflow` 计数 → 2s 指标 warn + WWB1 头 + 前端徽标;
   预览降载 → 步长抽点恒含批尾最新值。**不存在静默缝合**。

## 4. 金字塔存储 (min-max tiers)

- 结构: L1..Ln 每层是上一层的 ×16 min-max 摘要, 每块一对交错条目
  (min_ts, max_ts)/(min, max); 每层固定 4096 条目 → 跨度按 ×16 几何增长
  (L1≈6.5 万样本, L2≈100 万, L3≈1600 万…), 内存每层 ~100KB/通道。
- 写入: L0 每 16 样本折叠一块, 级联向上, 摊销 O(1)/样本
  (`DataBuffer::maybe_fold`)。
- 查询 (`snapshot_window_budget` / `snapshot_all_budget`): 窗口完整落在 L0
  覆盖内且点数 ≤ 4×预算 → 原始快照; 否则自底向上找"覆盖窗口起点且
  条目数 ≤ 4×预算"的最小层。派生序列按时间精确对齐到层轴。
- 停止态快照: `DataBuffer::clone` 深拷贝即冻结 (含金字塔/派生)。

## 5. 成本模型 (全样本严格求值的边界)

串行求值实测 ~0.91µs/帧 (含逐帧图求值); fork-join + SIMD 路径
(`eval_workers = min(cores, 8)`, 与串行逐位一致) 按核数折算。
**原始波形显示不消耗该预算** (不变量 3)。挂接重滤波图组时, 若
`帧率 × 单帧成本 / workers > 1` 将持续出现"评估队列丢弃"告警 —
这是显式承载上限而非缺陷; 处置: 降低采样率 / 精简图组 / 提高 eval_workers。

## 6. 诊断指标 (2s 窗口, `数据平面指标`)

`rx MB/s (消息/s) | feed N 批, 均 Xms (parse 均 | eval 均), 帧均, 产帧≈N/s
| Lagged 丢弃 | 评估队列丢弃` + `缓冲降载` warn (storage_overflow 增量)。

判读:
- `评估队列丢弃 > 0` → 求值承载不足 (见 §5), 有状态算子已复位;
- `缓冲降载` warn → 窗口超出原始层容量, 前端出现降载徽标;
- `产帧≈` 与 TestData 名义速率的比值反映系统实际吞吐 (生成器截止驱动补齐,
  单轮最多补 2 轮)。

## 7. 关键位置索引

| 组件 | 位置 |
|---|---|
| 采样时钟域 | `data_plane/src/data_plane/mod.rs` (`SampleClock`, `restamp_frames`) |
| 记录/求值入口 | `data_plane/src/data_plane/frame_dispatch.rs` (`record_frames` / `eval_frames`) |
| 去重组路由 | `data_plane/src/data_plane/byte_router.rs` (`route_inner` 分组路径), `mod.rs` (`rebuild_route_groups`) |
| 缺口记账/复位 | `mod.rs` (`eval_gaps`), `graph_eval.rs` (`reset_source_transient_state`) |
| 金字塔 | `buffer_databuffer/src/tier.rs`, 预算查询 `window.rs` (`snapshot_window_budget`) |
| 派生独立时间轴 | `buffer_databuffer/src/derived.rs` (`DerivedStore`) |
| 容量整定 | `mod.rs` (`tune_buffer_capacity`), `data_buffer.rs` (`ensure_capacity_for_rate`) |
| WWB1 v2 | `cmd/display/src/waveform_binary.rs`, 前端 `src/lib/data/waveformProtocol.ts` |
| 降载徽标 | `src/components/displays/waveform/WaveformChart.tsx` |
| 合批上限/生成器补速 | `data_plane/src/data_plane/read_task.rs`, `transport_core/src/test_data.rs` |
