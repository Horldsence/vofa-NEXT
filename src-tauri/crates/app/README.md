# L3 app — 应用核心

AppState 全局状态、图提交核心 graph_ops (无 IPC, 命令层与 MCP 共用)、原生菜单、更新流。

**规则**: 可依赖 L0-L2；禁止依赖 L4 (cmd)。

详见 [crate-layers.md](../../../docs/architecture/crate-layers.md)。
