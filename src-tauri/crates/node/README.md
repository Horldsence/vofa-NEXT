# L1 node — 节点图编译器与运行时

编译管线 hir → plane → lower → eval，门面 engine；含 kind/frame_decoder/trigger/testkit(仅 dev-dep)。

**规则**: 可依赖 L0；禁止依赖 L2+。

详见 [crate-layers.md](../../../docs/architecture/crate-layers.md)。
