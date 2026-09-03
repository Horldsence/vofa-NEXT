# L4 cmd — Tauri IPC 命令层

Tauri 命令薄适配: 参数反序列化 + State 借用 + 命令注册。可复用逻辑必须下沉到 L3 或以下。

**规则**: 可依赖 L0-L3；禁止被任何非 cmd crate 依赖 (二进制除外)。

详见 [crate-layers.md](../../../docs/architecture/crate-layers.md)。
