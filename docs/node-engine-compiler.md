# 节点图编译器架构 — HIR → 平面投影 → Lowering 三段式流水线

节点图引擎将前端同步来的节点图 (`Vec<NodeDef>` + `Vec<Edge>`) 编译为
可执行产物。编译分三段, 对应编译器的前端/中端/后端模型, 按 crate 拆分:
`node_hir` (前端) → `node_plane` (中端) → `node_lower` (后端 lowering) +
`node_eval` (槽位运行时), 门面 `node_engine` 驱动流水线并保有 CompiledGraph
与慢路径求值。

```
Vec<NodeDef> + Vec<Edge>
      │
      ▼  前端 (node_hir)
┌─────────────────────────────────────────────┐
│ TypedGraph: petgraph StableDiGraph           │
│ <HirNode, HirEdge>                           │
│ - 节点 id interning (String → NodeIndex)     │
│ - 双角色节点: value_def / byte_def 同槽共存   │
│   (同 id 的全局 Protocol 定义与本 tab 的      │
│   ProtocolSource 引用)                       │
│ - 端口域解析 + 边分类 → EdgeClass            │
│ - 跨域边 → CompileError::DomainMismatch      │
└─────────────────────────────────────────────┘
      │
      ▼  中端 (node_plane)
┌──────────────────┐  ┌──────────────────────────────┐
│ 字节平面子图      │  │ 值平面子图 (f32 ∪ 字符串边,   │
│ → BytePlan       │  │ 无值输出节点剔除)             │
│ (拓扑序+路由表)   │  │ → ValueMir (拓扑序 + 输入索引)│
└──────────────────┘  └──────────────────────────────┘
      │                      │
      ▼  后端                ▼  (node_lower: lower.rs / kinds / ops.rs)
┌──────────────────┐  ┌──────────────────────────────┐
│ BytePlan          │  │ SlotArena (f32/字符串双 arena)│
│ order + consumers │  │ → 平坦 CompiledOp 序列        │
│ + O(1) 成员查询   │  │ = CompiledEval                │
└──────────────────┘  └──────────────────────────────┘
```

## 平面划分

端口域 (`PortDomain`) 决定边所属平面:

- **字节平面** (`EdgeClass::Byte` / `RawDataMarker(Bytes)`): Transport / Protocol /
  FrameDecoder 字节入口 / widget `loopbackOut` 出口, 边携带 `Vec<u8>`, 事件驱动。
- **值平面** (`EdgeClass::F32` / `Str` / `RawDataMarker(F32)`): f32 槽位 +
  字符串槽位共享同一拓扑序 (保证上游 string 节点先于下游 Str 节点求值)。
- **跨平面不构成循环**由投影结构性保证: 各平面子图只含本平面边。
- **RawData 关联通道边** (Sink 的 `src:` 动态端口) 只是用户意图标记,
  按源端域归类参与对应平面拓扑; 字符串源不参与任何平面 (订阅旁路)。

### 字符串平面的正式通路 (`source_texts`)

RawData 协议不产帧, 其原始字节走一条与 `source_frames` 对称的正式通道进入
字符串平面:

```
Transport.rx 字节 ──► byte_router::route_bytes
                         │ RawData 预设 (或 schema preset=RawData)
                         ▼
   frame_dispatch::cache_source_text → source_texts[proto_id]  (UTF-8 lossy, latest-value)
                         │
                         ▼ 值平面求值 (ProtocolSource 的 "str" 端口, String 域)
   CompiledOp::ProtocolSourceStr (快) / protocol_source arm (慢) ──► 字符串槽位
                         │
                         ▼ 下游消费: Str 算子链 / Trigger / Custom / TextOut …
   materialize_str ──► graph_string_outputs ──► text_output_ticker (前端显示/回发桥)
```

- 写入口唯一: `frame_dispatch::cache_source_text` (自定义 schema 的 RawData 不写,
  走 SchemaEngine 产帧语义, 见 byte_router_tests)。
- 无缓存时对应槽位不写 → 快照保持上次值 (对齐 Trigger 未激活帧语义)。

## 源间交互与互相转化

同一 tab 内不同源之间的交互有三条既有机制 + 一个发送桥:

1. **域校验边连线**: F32/String/Bytes 三类域一致即可连; 跨源链路经全局 BytePlan
   合并重算, Transport.rx → 另一 Transport.tx 的转发直接在字节平面成立。
2. **convert_to 重编码链**: Protocol 解析出帧后按 convert_to 目标协议重编码,
   沿自身 out 边递归下推 (深度上限 16), 实现协议间互转。
3. **inject_bytes**: 前端可向任意节点字节入口注入数据 (FrameDecoder.in /
   Protocol.in / Transport.tx), 无需真实连接。
4. **TextOut 发送桥** (新增): 图内动态字符串写回传输口, 见下节。

### convert_to 语义表

| 本端协议 | convert_to 行为 |
|---|---|
| JustFloat / FireWater / 自定义 schema (产帧) | 帧 → convert 引擎重编码 → out 边下推 |
| Slcan / CandleLight / LogicDecode | 同上 (有帧则转) |
| **RawData (preset)** | 不产帧, 重编码产物恒为空 → **原始字节从 out 口透传** (UI 有提示); 不静默丢弃 |

## 动态发送回传 (TextOut 桥)

图内字符串 → 传输口的单向桥, 复用通用发布管道, 不在求值热路径做 IO:

```
[值平面求值] TextOut op: 上游 str 槽位 ──透传──► str_slot(tout,"text")
                    │ (materialize_str 发布点, 与其它字符串输出同生命周期)
                    ▼
        graph_string_outputs["tout"]["text"]
                    │ 读取 (锁内收集, 锁外 IO)
                    ▼
        app_state::tickers::textout_sender_ticker (10ms tick)
        ── 变化且距上次 ≥ min_interval_ms 才发送 (未到窗口挂起待补发)
        ── transport.send(target_transport, text + newline_suffix)
```

- 规格 (`TextOutSpec { target_transport, newline_suffix, min_interval_ms }`)
  在 lowering 编译期收集于 `SlotPlan.textouts`, 快慢两条求值路径行为一致
  (TextOut 参与 eval_order, 其 "text" 槽位为无输出端口的透传写)。
- 手动触发: `send_text_out_now(node_id)` 命令立即读当前文本发送一次;
  目标未打开时报错并按窗口节奏限频重试 (ticker 侧仅记日志)。

## 关键类型

| 类型 | 模块 | 职责 |
|---|---|---|
| `TypedGraph` | node_hir | HIR: interning + 双角色定义 + 边分类 |
| `EdgeClass` | node_hir | 边分类: Byte / F32 / Str / RawDataMarker |
| `ValueMir` | node_plane | 值平面拓扑序 + input_index + in_names |
| `BytePlan` | node_plane::byte_plan | 字节平面拓扑序 + consumers 路由表 |
| `SlotArena` | node_lower | 槽位分配器 (同 (node,port) dedup) |
| `CompiledOp` | node_lower::ops | 平坦槽位操作枚举 (定义与执行分离) |
| `CompiledEval` | node_eval | 槽位评估表 — 热路径 `run` 零字符串哈希 |
| `CompiledGraph` | node_engine::compile | 编译 facade + 节点查询访问器 |

## 双路径求值

- **快路径**: `CompiledEval::run` — 平坦 op 数组 + 槽位读写, 700k 帧/s 热路径。
- **慢路径**: `CompiledGraph::evaluate_into` — 逐节点 map 语义参考实现。
- `equiv_tests` 持续校验两路径输出一致; 任何 lowering/求值改动必须保持该网全绿。

### 字符串算子表 (StrOp)

字符串平面同时承担 数值 ↔ 文本 的互转 (源间转化的桥), 算子分四类
(`node_kind::str_op`, 端口表 = 单一事实源, 前端 `STR_OP_PORTS` 镜像):

| 类别 | 算子 | 说明 |
|---|---|---|
| 纯文本 | LEN/FIND/CONTAINS/LEFT/RIGHT/MID/CONCAT/INSERT/DELETE/REPLACE/UPPER/LOWER/TRIM/REVERSE | 1-based 字符索引; 多字节安全 |
| 文本 → 数值 | PARSE | 从 POS 起扫描首个数字 token (十进制含指数 / 0x 十六进制, 十六进制无符号); 未命中 0.0 |
| 数值 → 文本 | FORMAT | 模板 `{N}` 引用第 N 路 / `{N:.P}` 定精度 / `{{` `}}` 转义; fmt 口未连接用 `tmpl` 参数内联回退 |
| 编码 | ENCODE_HEX | UTF-8 字节 → 大写 HEX 文本 |

典型互转链: 波形通道 → FORMAT 模板文本 → TextOut 发往文本协议设备;
RawData(str) → MID/PARSE → 提取数值回波形通道。

## 扩展指南

- **新增节点类型**: `node_kind` 加 NodeKind 变体 + `port_domain` 域表 →
  `node_lower/src/kinds/` 加一个 `lower_*` 函数并在 `lower_node` 分派 →
  `node_lower::ops` 加 `CompiledOp` 变体 → `node_eval::CompiledEval::run`
  与门面 `evaluate` 各加一个执行臂。若节点**有输入但无输出端口且依赖拓扑序**
  (如 TextOut), 让它参与 eval_order 即可 — 无输出端口天然不会被下游引用;
  若需要编译期收集规格 (如 TextOutSpec), 在 `SlotPlan` 加字段。
- **新增平面**: `EdgeClass` 加分类 + `node_plane` 加一个投影函数 (子图谓词 +
  `topo` 排序) + 对应后端产物。
- **环诊断**: `CompileError::Cycle` / `ByteCycle` 携带完整环路径
  (如 `a → b → a`), 由 `plane::extract_cycle` 三色 DFS 提取。

## 派生计算归后端

`NodeKind` 字段从前端的派生数据逐渐收窄为原始配置:

- **Filter 节点**: `kind: FilterKind { b, a }` → `config: FilterConfig { preset, cutoff/low/high, sample_rate }`。
  前端原样下发 `widget.params`, 后端 `dsp_filter::filter_kind_from_config` 在
  `lower_filter` 时产出 [b, a]; `evaluate` 时按 `config` 比较决定 `filter_states` 重建。
- **Filter 回退 FIR**: 阶段三 FilterConfig 仅含 4 类预设 (lowpass/highpass/bandpass/bandstop);
  旧测试用了 `FilterKind::FIR` 临时造节点, 现已迁移至对应预设构造以维持现有测试。
- **协议节点**: 自 `cmd_graph/src/derived.rs::compute_derived` 提供 `NodeDerived`
  输出端口表与 `effective_channels`, 由 `update_tab_graph` 响应 + `graph:derived`
  事件差分推送, 前端写入 `derivedPorts` store。preset 协议 (JustFloat/FireWater/
  RawData/Slcan/CandleLight/LogicDecode) 的 schema 工厂与 `port_names` 已下沉,
  前端不再下发该字段。

## 命令帧字节打包 (`compute_frame_bytes`)

后端 IPC 单一权威 — `crates/cmd_buffer/src/{frame_field,frame_checksum,command_frame}.rs`:

- 块类型 `ConstHex` / `VarRef` / `TypedConst` / `Checksum` 序列化与前端 `CommandBlock` 对齐
  (snake_case `port_name` / `field_type` / `custom_script`)。
- 字段打包 (`FieldType`) 与校验 (`ChecksumKind`) 8 种算法逐字节对齐前端实现; 新增
  `compute_command_frame_bytes` Tauri 命令, 由前端 `api.computeFrameBytes` 调用。
- 错误按块编号: `error: "块 #N: ..."`, 前端可定位到具体块。
- `Custom` JS 校验脚本 (前端 `new Function`) 后端不支持 — 返回错误而非尝试解析。
