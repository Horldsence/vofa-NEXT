# wgpu 预处理渲染波形 — 可行性论证与原型结论

> 问题: 能否用 wgpu 在后端「预处理渲染」波形, 让前端退化成纯显示?
> 结论先行: **整帧像素渲染 (路线 A) 技术可行但对本应用是负收益, 不采纳**;
> **GPU 预处理降采样 (路线 B) 是 wgpu 在本应用的正确角色, 已落地原型**。
> 图求值方向的 wgpu 卸载已删除, 由 SciRS2 SIMD 批量求值替代
> (commit 50bc1fe, 逐位等价, 见 `graph_eval_simd_equiv.rs`)。

## 背景

wgpu 最初为数值平面的 Math 单元图求值而引入 (WGSL 内核 + 设备层)。该方向
已回退: 图求值的批尺寸 (EVAL_CHUNK=4096 帧 × 少量槽位) 不足以摊平 GPU 的
上传/回传/会话管理复杂度, 且位级等价测试只能做到 1e-5 容差 (超越函数域约
减近似)。SciRS2 SIMD 替代实现与标量热路径逐位一致且零调度开销。

随之而来的问题: 保留的 wgpu 设备层 (gpu_core) 是否有更合适的用途 — 即
「后端预处理渲染波形, 前端只显示」。

## 路线 A: 整帧像素渲染 (后端画完, 前端贴图)

技术路线: wgpu headless 渲染到纹理 → `copy_texture_to_buffer` (256 字节行
对齐) → `map_async` 回读 → 二进制 IPC → 前端 `putImageData`/`drawImage`。
learn-wgpu 的 windowless 教程覆盖了该路径, 本身没有技术障碍。**不采纳的
论据**:

1. **无零拷贝纹理共享**。Tauri 的 WKWebView (macOS) / WebView2 (Windows)
   不接受外部纹理导入; 后端 GPU 显存里的帧必须以字节形式穿过 IPC。没有
   IOSurface / shared texture 直通路径。

2. **带宽超预算一个数量级**。1200×700 画布 RGBA = 3.36 MB/帧; 30 FPS =
   ~100 MB/s。本应用现有背压预算 `preview_bandwidth_mb_per_sec` 默认
   8 MB/s (超限自动降速), 二进制通道按此设计。改用 PNG/WebP 编码可压到
   1-3 MB/s 量级, 但编码本身 3-10 ms/帧 CPU, 把 GPU 渲染的收益吃掉。

3. **延迟链变长、交互变往返**。渲染→回读→(编码)→IPC→解码→贴图 ≈
   30-60 ms/帧; 而缩放/平移/游标这类交互在「前端持有数据」时是零往返的,
   像素流方案下每次交互都要走一遍后端重渲染 round-trip, 手感必然劣化。

4. **收益不存在**。当前波形推送 maxPoints=2000 @ 30 FPS, uPlot (Canvas2D)
   渲染该量级在主线程 1-2 ms/帧, 远非瓶颈 (uPlot 官方基准可支撑 15 万点
   @ 60 FPS)。为一个非瓶颈买回读开销 (GPU→CPU 回读可砍半吞吐) 与 DPI/
   resize/主题重渲染的全套 Rust 侧复杂度, 是负收益。

## 路线 B: GPU 预处理降采样 (后端压缩数据, 前端绘制) — 采纳

数据量瓶颈出现在「窗口点数 ≫ 画布像素数」时: 前端每帧收到 2000 点采样
只是缓冲窗口的截断, 大窗口 (缓冲容量 100k, 潜在 1M+) 无法全量展示。
正确形态: 后端对全量窗口做**逐列 min/max 包络压缩** (每像素列一列),
N 点 → columns×(min, max, count), 数据量缩小 N/columns 倍; 前端绘制
半透明缎带 (上限前进 + 下限回折), count=0 列断线。

### 原型实现 (本提交)

- `gpu_core/src/envelope.rs`: WGSL 原子 min/max kernel (有序浮点位键:
  负数全位取反 / 非负仅翻符号位; NaN 位模式检测 — Metal 快浮点会把
  `x != x` 折叠, 必须位运算; ±0 归一) + CPU 线性扫描参考实现, 两者
  **位级一致** (`tests/envelope_equiv.rs`, 含 NaN/±inf/±0 边缘与确定性)。
- `cmd_display`: `DisplayRequest::WaveformEnvelope { source, columns }`
  → VENV v1 二进制帧 (头 60B + 每通道 columns×12B), 快照语义推送,
  GPU 不可用自动回退 CPU。
- 前端: `envelopeProtocol.ts` 解码 (零拷贝视图) → `WaveformEnvelopeChart`
  Canvas2D 缎带绘制; 波形图右上角「包络」开关 (默认关)。

### 实测 (criterion, Apple M 系 / Metal, columns=2048, 正弦+噪声+NaN)

| 窗口点数 | CPU 线性扫描 | GPU (wgpu) | GPU 加速比 |
|---|---|---|---|
| 100k | 0.72 ms | 1.21 ms | 0.6× (CPU 胜) |
| 1M | 7.20 ms | 2.75 ms | **2.6×** |
| 4M | 28.7 ms | 9.80 ms | **2.9×** |

复现: `cd src-tauri && cargo bench -p gpu_core --bench envelope_bench`。

解读:

- **典型窗口 (≤100k = 缓冲默认容量) 下 CPU 已亚毫秒, GPU 无收益** —
  每点两次全局原子的 kernel 在小输入下被 dispatch/回读延迟主导。这再次
  印证: 当前数据量级 (2000 点推送) 引入 GPU 毫无必要。
- **窗口 ≥ 1M 点时 GPU 稳定 2.6-2.9×**, 原型 kernel 尚未做 workgroup
  共享内存分层归约, 仍有优化空间。
- 端到端链路 (GPU 压缩 + 60B + columns×12B/ch IPC + 解码 + Canvas 绘制)
  在 1M 点窗口、30 FPS 节拍下每帧 ~3 ms 后端 + 亚毫秒前端, 远低于
  8 MB/s 带宽预算 (每帧仅 ~24 KB/通道)。

## 结论与重启条件

1. **路线 A (像素流) 关闭**。除非未来出现后端独占的渲染资产 (如 GPU FFT
   频谱图着色) 且画布尺寸受限, 否则不重评。
2. **路线 B (降采样) 作为 wgpu 的存续形态保留**, 触发条件: 波形窗口默认
   容量或推送点数提升至 100k 以上 (例如轨迹回放/长时程录波), 届时把
   「包络模式」从按需开关升级为大数据量自动切换。
3. 图求值不再回到 GPU: SciRS2 SIMD 路径逐位对齐标量参考, 无容差负担;
   若未来批尺寸结构性放大 (如批处理 API), 优先扩展 SIMD 分块而非 GPU。

## 参考资料

- learn-wgpu: windowless (offscreen) rendering — `copy_texture_to_buffer`
  与 256 字节行对齐: https://sotrh.github.io/learn-wgpu/showcase/windowless/
- wgpu discussion #6264: CPU 回读的正确姿势 (异步映射):
  https://github.com/gfx-rs/wgpu/discussions/6264
- SciRS2: https://github.com/cool-japan/scirs (本项目使用 scirs2-core 0.6.5
  `simd` feature; 注意其 default-features=false 时需显式开启 `std`)
- uPlot 性能基准: https://github.com/leeoniya/uPlot (150 万点/60 FPS 量级)
