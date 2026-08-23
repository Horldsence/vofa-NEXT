# 节点图编译器架构 — HIR → 平面投影 → Lowering 三段式流水线

`node_engine` crate 将前端同步来的节点图 (`Vec<NodeDef>` + `Vec<Edge>`) 编译为
可执行产物。编译分三段, 对应编译器的前端/中端/后端模型。

```
Vec<NodeDef> + Vec<Edge>
      │
      ▼  前端 (hir.rs)
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
      ▼  中端 (plane.rs)
┌──────────────────┐  ┌──────────────────────────────┐
│ 字节平面子图      │  │ 值平面子图 (f32 ∪ 字符串边,   │
│ → BytePlan       │  │ 无值输出节点剔除)             │
│ (拓扑序+路由表)   │  │ → ValueMir (拓扑序 + 输入索引)│
└──────────────────┘  └──────────────────────────────┘
      │                      │
      ▼  后端                ▼  (lower.rs / lower_kinds.rs / ops.rs)
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

## 关键类型

| 类型 | 模块 | 职责 |
|---|---|---|
| `TypedGraph` | hir.rs | HIR: interning + 双角色定义 + 边分类 |
| `EdgeClass` | hir.rs | 边分类: Byte / F32 / Str / RawDataMarker |
| `ValueMir` | plane.rs | 值平面拓扑序 + input_index + in_names |
| `BytePlan` | byte_plan.rs | 字节平面拓扑序 + consumers 路由表 |
| `SlotArena` | lower.rs | 槽位分配器 (同 (node,port) dedup) |
| `CompiledOp` | ops.rs | 平坦槽位操作枚举 (定义与执行分离) |
| `CompiledEval` | eval.rs | 槽位评估表 — 热路径 `run` 零字符串哈希 |
| `CompiledGraph` | compile.rs | 编译 facade + 节点查询访问器 |

## 双路径求值

- **快路径**: `CompiledEval::run` — 平坦 op 数组 + 槽位读写, 700k 帧/s 热路径。
- **慢路径**: `CompiledGraph::evaluate_into` — 逐节点 map 语义参考实现。
- `equiv_tests` 持续校验两路径输出一致; 任何 lowering/求值改动必须保持该网全绿。

## 扩展指南

- **新增节点类型**: `node_kind` 加 NodeKind 变体 + `port_domain` 域表 →
  `lower_kinds.rs` 加一个 `lower_*` 函数并在 `lower_node` 分派 →
  `ops.rs` 加 `CompiledOp` 变体 → `eval.rs::run` 与 `evaluate.rs` 各加一个执行臂。
- **新增平面**: `EdgeClass` 加分类 + `plane.rs` 加一个投影函数 (子图谓词 +
  `topo` 排序) + 对应后端产物。
- **环诊断**: `CompileError::Cycle` / `ByteCycle` 携带完整环路径
  (如 `a → b → a`), 由 `plane::extract_cycle` 三色 DFS 提取。
