# VOFA-NEXT

一个使用 **Rust + egui** 完全重构的下一代串口助手。

![Logo](icon.png)

现代化串口调试工具，支持波形显示、节点编辑器与多协议解析。

## 核心特性

- **多传输层支持**：串口（Serial）、TCP 客户端 / 服务器、UDP、CAN（slcan / candleLight）、逻辑分析仪，支持自动重连与连接状态通知。
- **协议解析引擎**：内置 VOFA FireWater、JustFloat 协议，支持通道自动检测与原始数据查看。
- **节点编辑器**：基于 egui-snarl 的节点图，支持输入控件、数学运算、滤波器、频谱 / 帧解码 Sink 等节点编排。
- **数据可视化**：波形（egui_plot）、原始数据 Hex 视图、CAN 帧列表、逻辑时序图、解码事件、频谱图。
- **Catppuccin 主题**：内置 Mocha / Macchiato / Frappé / Latte 四套配色，简洁美观。
- **纯 Rust 桌面应用**：无 WebView、无 Node.js、无 Tauri，单二进制交付。

## 技术栈

- [Rust](https://www.rust-lang.org/)
- [egui / eframe](https://github.com/emilk/egui)（GUI）
- [egui-snarl](https://github.com/zakarumych/egui-snarl)（节点编辑器）
- [egui_dock](https://github.com/Adanos020/egui_dock)（停靠布局）
- [egui_plot](https://github.com/emilk/egui_plot)（波形 / 频谱）
- [Tokio](https://tokio.rs/)（异步传输层）
- [Serde](https://serde.rs/)（配置持久化）

## 目录结构

```
vofa-NEXT/
├── Cargo.toml                  # Cargo workspace root
├── app/                        # egui 桌面应用
│   ├── src/
│   │   ├── main.rs             # eframe 入口
│   │   ├── app.rs              # 应用外壳（菜单/状态栏/停靠区）
│   │   ├── core/               # 后端核心（状态/服务/数据流水线）
│   │   ├── ui/                 # 布局/面板/显示组件/节点编辑器
│   │   ├── settings.rs         # 配置持久化
│   │   └── theme.rs            # Catppuccin 主题
│   └── icons/
├── crates/                     # 业务逻辑 workspace crates
│   ├── vofa-next-core          # 核心类型与配置
│   ├── vofa-next-transport     # 传输层（串口/TCP/UDP/CAN）
│   ├── vofa-next-protocol      # 协议解析引擎
│   ├── vofa-next-buffer        # 数据缓冲与波形窗口
│   ├── vofa-next-nodes         # 节点图 DAG 引擎
│   ├── vofa-next-dsp           # FFT / 滤波器
│   └── vofa-next-automotive    # 诊断（ISO-TP / UDS / OBD-II）
├── scripts/
│   └── build.sh                # 发布构建脚本
├── docs/
└── images/
```

## 开发环境

- [Rust](https://www.rust-lang.org/tools/install)（stable）

## 运行与开发

```bash
# 开发运行（打开 GUI）
cargo run -p vofa-next-app

# 类型检查
cargo check -p vofa-next-app

# 单元测试
cargo test --workspace
```

## 构建发布版

```bash
# 本机构建
./scripts/build.sh

# 指定目标
./scripts/build.sh aarch64-apple-darwin
./scripts/build.sh x86_64-pc-windows-msvc
./scripts/build.sh x86_64-unknown-linux-gnu
```

产物位于 `target/release/`（或对应 target 目录）。

## 贡献指南

1. Fork 本项目
2. 创建功能分支：`git checkout -b feature/AmazingFeature`
3. 提交改动：`git commit -m 'Add some AmazingFeature'`
4. 推送到分支：`git push origin feature/AmazingFeature`
5. 提交 Pull Request

## 开源协议

本项目基于 MIT 协议开源，详情请参阅 [LICENSE](./LICENSE)。

## 鸣谢

- [VOFA+](https://www.vofa.plus/) 提供的 FireWater / JustFloat 协议参考
- [egui](https://github.com/emilk/egui)
- [egui-snarl](https://github.com/zakarumych/egui-snarl)
- [Catppuccin](https://catppuccin.com/)
