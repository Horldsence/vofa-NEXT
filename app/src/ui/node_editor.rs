//! 节点编辑器 — Control 页签的节点图编辑视图 (egui-snarl)
//!
//! 每个 Control 页签持有一个 [`ControlTabState`], 内含 [`Snarl<GraphNode>`] 图。
//! 图被修改 (加/删节点, 连/断线) 时置 `dirty`, 下一帧编译为后端
//! `NodeDef` + `Edge` 并通过 `core::services::update_tab_graph` 同步。

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui;
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, Snarl};

use vofa_next_buffer::graph::Edge;
use vofa_next_core::WidgetBinding;
use vofa_next_nodes::{MathOp, NodeDef, NodeKind};

use crate::core::services;
use crate::core::AppState;
use crate::theme;
use crate::ui::controls;

/// Control 页签的后端图 key 前缀
pub fn backend_tab_id(tab_id: u64) -> String {
    format!("control-{tab_id}")
}

/// Input 节点的 UI 侧控件配置 (仅 UI 使用, 不序列化到后端 NodeDef)
#[derive(Debug, Clone)]
pub struct InputControl {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    /// 发送绑定 (控件值变化时按绑定下发到设备)
    pub binding: WidgetBinding,
}

impl Default for InputControl {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            binding: WidgetBinding::None,
        }
    }
}

/// Sink 节点的显示方式 (仅 UI 使用, 不序列化到后端 NodeDef)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SinkDisplayKind {
    /// 大号数字读数
    #[default]
    Number,
    /// 进度条式表盘 (按 [sink_min, sink_max] 归一化)
    Gauge,
    /// LED 指示灯 (value > 0 点亮)
    Led,
}

/// 图中一个节点的 UI 侧数据
pub struct GraphNode {
    /// 后端节点 id (稳定, 与 widget id 对齐)
    pub id: String,
    /// 节点种类 + 参数 (直接复用后端 NodeKind)
    pub kind: NodeKind,
    /// 显示标题
    pub label: String,
    /// Input 节点的控件配置 (其余节点忽略)
    pub control: InputControl,
    /// Sink 节点的显示方式 (其余节点忽略)
    pub sink_display: SinkDisplayKind,
    /// Sink Gauge 显示区间下限
    pub sink_min: f32,
    /// Sink Gauge 显示区间上限
    pub sink_max: f32,
}

impl GraphNode {
    pub fn new(id: String, kind: NodeKind, label: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            label: label.into(),
            control: InputControl::default(),
            sink_display: SinkDisplayKind::default(),
            sink_min: 0.0,
            sink_max: 100.0,
        }
    }
}

/// 计算某类节点的输入端口 id 列表 (与后端契约一致, 见 vofa-next-nodes 注释)
pub fn input_ports(kind: &NodeKind) -> Vec<String> {
    match kind {
        NodeKind::ChannelSource { .. } => vec![],
        NodeKind::Input => vec![],
        NodeKind::Math { input_count, .. } => (0..*input_count).map(|i| format!("in{i}")).collect(),
        NodeKind::Custom { inputs, .. } => inputs.clone(),
        NodeKind::Filter { .. } => vec!["in0".into()],
        NodeKind::SpectrumSink { .. } => vec!["in0".into()],
        NodeKind::FrameDecoder { .. } => vec![],
        NodeKind::Sink => vec!["value".into()],
        NodeKind::RawDataSink { inputs } => inputs.clone(),
    }
}

/// 计算某类节点的输出端口 id 列表 (与后端契约一致)
pub fn output_ports(kind: &NodeKind) -> Vec<String> {
    match kind {
        NodeKind::ChannelSource { channels } => (0..*channels).map(|i| format!("ch{i}")).collect(),
        NodeKind::Input => vec!["value".into()],
        NodeKind::Math { .. } => vec!["result".into()],
        NodeKind::Custom { outputs, .. } => outputs.clone(),
        NodeKind::Filter { .. } => vec!["result".into()],
        NodeKind::SpectrumSink { .. } => vec![],
        NodeKind::FrameDecoder {
            blocks,
            enable_valid,
            enable_frame_count,
            enable_last_timestamp,
            enable_fps,
        } => {
            let mut ports: Vec<String> = blocks
                .iter()
                .filter_map(|b| b.output_port_name().map(|s| s.to_string()))
                .collect();
            if *enable_valid {
                ports.push("valid".into());
            }
            if *enable_frame_count {
                ports.push("frame_count".into());
            }
            if *enable_last_timestamp {
                ports.push("last_timestamp".into());
            }
            if *enable_fps {
                ports.push("fps".into());
            }
            ports
        }
        NodeKind::Sink => vec![],
        NodeKind::RawDataSink { .. } => vec![],
    }
}

/// 视图操作请求 - 由 minimap/缩放控件写入, 下一帧 current_transform 消费
#[derive(Debug, Clone, Copy)]
enum ViewAction {
    /// 以画布中心为锚点放大一档
    ZoomIn,
    /// 以画布中心为锚点缩小一档
    ZoomOut,
    /// 恢复到 1:1 缩放
    ResetZoom,
    /// 适应所有节点 (fit to content)
    FitToContent,
    /// 将指定图空间坐标居中显示
    CenterAt(egui::Pos2),
}

/// 某个 Control 页签的节点编辑器状态
pub struct ControlTabState {
    /// egui-snarl 图
    pub snarl: Snarl<GraphNode>,
    /// 自动生成节点 id 的计数器 (n{next_auto_id})
    pub next_auto_id: usize,
    /// 图自上次同步后有改动, 需要重新编译并提交后端
    pub dirty: bool,
    /// 下一帧待执行的视图操作 (缩放/minimap 导航)
    pending_view_action: Option<ViewAction>,
    /// 是否显示页面缩略图 (minimap)
    pub minimap_visible: bool,
    /// 最近一帧 snarl 渲染时捕获的图->屏变换 (供 minimap/缩放控件读取)
    captured_to_global: egui::emath::TSTransform,
    /// 最近一帧 snarl 占用的屏幕矩形 (供 minimap/缩放控件定位 overlay)
    captured_view_rect: egui::Rect,
}

impl ControlTabState {
    /// 创建默认图 - 仅含一个 ChannelSource 节点 (每 tab 一个)
    pub fn new(tab_id: u64) -> Self {
        let mut snarl = Snarl::new();
        snarl.insert_node(
            egui::pos2(40.0, 80.0),
            GraphNode::new(
                format!("__channel_source__-{}", backend_tab_id(tab_id)),
                NodeKind::ChannelSource { channels: 4 },
                "Channel Source",
            ),
        );
        Self {
            snarl,
            next_auto_id: 0,
            // 初始即 dirty, 首帧同步默认图到后端
            dirty: true,
            pending_view_action: None,
            minimap_visible: true,
            captured_to_global: egui::emath::TSTransform::default(),
            captured_view_rect: egui::Rect::NOTHING,
        }
    }
}

/// 新建节点的默认参数 (右键菜单 "Add" 使用)
fn default_node_kind(name: &str) -> Option<(NodeKind, &'static str)> {
    match name {
        "Input" => Some((NodeKind::Input, "Input")),
        "Math" => Some((
            NodeKind::Math {
                op: MathOp::Add,
                input_count: 2,
            },
            "Math (Add)",
        )),
        "Filter" => Some((
            NodeKind::Filter {
                kind: vofa_next_nodes::FilterKind::FIR { b: vec![0.5, 0.5] },
            },
            "Filter",
        )),
        "Sink" => Some((NodeKind::Sink, "Sink")),
        "SpectrumSink" => Some((
            NodeKind::SpectrumSink {
                window_size: 1024,
                window_type: vofa_next_nodes::WindowType::Hann,
                output: vofa_next_nodes::SpectrumOutput::Magnitude,
                sample_rate: 1000.0,
            },
            "Spectrum Sink",
        )),
        "FrameDecoder" => Some((
            NodeKind::FrameDecoder {
                blocks: vec![],
                enable_valid: true,
                enable_frame_count: false,
                enable_last_timestamp: false,
                enable_fps: false,
            },
            "Frame Decoder",
        )),
        "RawDataSink" => Some((
            NodeKind::RawDataSink {
                inputs: vec!["in0".into()],
            },
            "Raw Data Sink",
        )),
        "Custom" => Some((
            NodeKind::Custom {
                inputs: vec!["in0".into()],
                outputs: vec!["out0".into()],
            },
            "Custom",
        )),
        _ => None,
    }
}

/// 右键图空白处可添加的节点列表
const ADD_NODE_MENU: &[&str] = &[
    "Input",
    "Math",
    "Filter",
    "Sink",
    "SpectrumSink",
    "FrameDecoder",
    "RawDataSink",
    "Custom",
];

/// 控件库条目 id → 节点默认参数 + UI 控件默认值
///
/// 返回 (kind, label, Input 控件默认值, Sink 显示方式)。
/// 未支持的条目 (Radio/Checkbox/Label/PieChart/TableView/Command) 返回 None。
pub fn palette_node_defaults(
    palette_id: &str,
) -> Option<(
    NodeKind,
    &'static str,
    Option<InputControl>,
    Option<SinkDisplayKind>,
)> {
    match palette_id {
        // 输入控件 → Input 节点
        "knob" | "button" | "slider" => {
            let (kind, _) = default_node_kind("Input")?;
            let label = match palette_id {
                "knob" => "Knob",
                "button" => "Button",
                _ => "Slider",
            };
            Some((kind, label, Some(InputControl::default()), None))
        }
        // 显示控件 → Sink 节点 (指定显示方式)
        "number_display" => {
            let (kind, _) = default_node_kind("Sink")?;
            Some((kind, "NumberDisplay", None, Some(SinkDisplayKind::Number)))
        }
        "gauge" => {
            let (kind, _) = default_node_kind("Sink")?;
            Some((kind, "Gauge", None, Some(SinkDisplayKind::Gauge)))
        }
        "led" => {
            let (kind, _) = default_node_kind("Sink")?;
            Some((kind, "LED", None, Some(SinkDisplayKind::Led)))
        }
        "waveform" => {
            let (kind, _) = default_node_kind("Sink")?;
            Some((kind, "Waveform", None, None))
        }
        // 处理 / 汇聚节点
        "math" => {
            let (kind, label) = default_node_kind("Math")?;
            Some((kind, label, None, None))
        }
        "filter" => {
            let (kind, label) = default_node_kind("Filter")?;
            Some((kind, label, None, None))
        }
        "spectrum" => {
            let (kind, label) = default_node_kind("SpectrumSink")?;
            Some((kind, label, None, None))
        }
        "frame_decoder" => {
            let (kind, label) = default_node_kind("FrameDecoder")?;
            Some((kind, label, None, None))
        }
        "raw_data_sink" => {
            let (kind, label) = default_node_kind("RawDataSink")?;
            Some((kind, label, None, None))
        }
        "custom_sink" => {
            let (kind, label) = default_node_kind("Custom")?;
            Some((kind, label, None, None))
        }
        _ => None,
    }
}

/// 把控件库条目对应的节点加入画布 (点击控件库条目时调用)
///
/// 节点插入在默认图附近 (级联错开), 置 dirty 后由 [`sync_if_dirty`] 同步后端。
/// 返回是否成功加入 (未支持的条目返回 false)。
pub fn add_palette_node(tab_state: &mut ControlTabState, palette_id: &str) -> bool {
    let Some((kind, label, control, sink_display)) = palette_node_defaults(palette_id) else {
        return false;
    };
    let id = format!("n{}", tab_state.next_auto_id);
    tab_state.next_auto_id += 1;

    // 级联位置: 从默认图右侧开始, 按节点数量错开
    let n = tab_state.snarl.node_ids().count() as f32;
    let offset = (n % 8.0) * 32.0;
    let pos = egui::pos2(240.0 + offset, 100.0 + offset);

    let mut node = GraphNode::new(id, kind, label);
    if let Some(c) = control {
        node.control = c;
    }
    if let Some(d) = sink_display {
        node.sink_display = d;
    }
    tab_state.snarl.insert_node(pos, node);
    tab_state.dirty = true;
    true
}

/// 将 snarl 图编译为后端 NodeDef + Edge
///
/// 边 id 使用 `e{index}` 形式; source/target handle 取端口 id 字符串。
pub fn compile_graph(snarl: &Snarl<GraphNode>, tab_id: &str) -> (Vec<NodeDef>, Vec<Edge>) {
    // NodeId -> 后端节点 id
    let id_map: HashMap<NodeId, &str> = snarl
        .node_ids()
        .map(|(node_id, node)| (node_id, node.id.as_str()))
        .collect();

    let nodes: Vec<NodeDef> = snarl
        .node_ids()
        .map(|(_, node)| NodeDef {
            id: node.id.clone(),
            tab_id: tab_id.to_string(),
            kind: node.kind.clone(),
        })
        .collect();

    let mut edges: Vec<Edge> = Vec::new();
    for (i, (out_pin, in_pin)) in snarl.wires().enumerate() {
        let (Some(&source), Some(&target)) = (id_map.get(&out_pin.node), id_map.get(&in_pin.node))
        else {
            continue;
        };
        let src_node = &snarl[out_pin.node];
        let tgt_node = &snarl[in_pin.node];
        let source_handle = output_ports(&src_node.kind)
            .get(out_pin.output)
            .cloned()
            .unwrap_or_default();
        let target_handle = input_ports(&tgt_node.kind)
            .get(in_pin.input)
            .cloned()
            .unwrap_or_default();
        edges.push(Edge {
            id: format!("e{i}"),
            source: source.to_string(),
            source_handle,
            target: target.to_string(),
            target_handle,
        });
    }

    (nodes, edges)
}

/// 若 dirty, 编译图并通过 rt.spawn 调用后端 update_tab_graph, 然后清除 dirty
pub fn sync_if_dirty(
    tab_state: &mut ControlTabState,
    state: &Arc<AppState>,
    rt: &tokio::runtime::Runtime,
    tab_id: u64,
) {
    if !tab_state.dirty {
        return;
    }
    tab_state.dirty = false;
    let backend_id = backend_tab_id(tab_id);
    let (nodes, edges) = compile_graph(&tab_state.snarl, &backend_id);
    let state = state.clone();
    rt.spawn(async move {
        if let Err(e) = services::update_tab_graph(&state, backend_id, nodes, edges).await {
            tracing::warn!("update_tab_graph failed: {e}");
        }
    });
}

/// 页签关闭时调用 — 通知后端移除该 tab 的编译图
pub fn remove_backend_graph(state: &Arc<AppState>, rt: &tokio::runtime::Runtime, tab_id: u64) {
    let backend_id = backend_tab_id(tab_id);
    let state = state.clone();
    rt.spawn(async move {
        if let Err(e) = services::remove_tab_graph(&state, backend_id).await {
            tracing::warn!("remove_tab_graph failed: {e}");
        }
    });
}

/// 节点类别强调色 - 用于节点 header 着色, 区分不同类型节点
///
/// 取色来自 [`crate::theme`] 的 Catppuccin 强调色, 与全应用配色一致。
fn node_accent_color(kind: &NodeKind, dark: bool) -> egui::Color32 {
    match kind {
        // 数据源 - 绿色
        NodeKind::ChannelSource { .. } => theme::accent_green(dark),
        // 输入控件 - 蓝色
        NodeKind::Input => theme::accent_blue(dark),
        // 数学运算 - 黄色
        NodeKind::Math { .. } => theme::accent_yellow(dark),
        // 滤波 - 桃色
        NodeKind::Filter { .. } => theme::accent_peach(dark),
        // 频谱 Sink - 桃色 (信号处理族)
        NodeKind::SpectrumSink { .. } => theme::accent_peach(dark),
        // 帧解码 - 紫色
        NodeKind::FrameDecoder { .. } => theme::accent_mauve(dark),
        // 显示 Sink - 青色
        NodeKind::Sink => theme::accent_teal(dark),
        NodeKind::RawDataSink { .. } => theme::accent_teal(dark),
        // 自定义 - 红色
        NodeKind::Custom { .. } => theme::accent_red(dark),
    }
}

/// 把强调色降为 header 背景填充 (与主题 surface 协调, 不刺眼)
fn header_fill(accent: egui::Color32, dark: bool) -> egui::Color32 {
    if dark {
        accent.gamma_multiply(0.32)
    } else {
        accent.gamma_multiply(0.42)
    }
}

/// 构建与全应用 UI 尺寸/字体一致的 SnarlStyle
///
/// - 节点 frame 使用主题 surface0 填充 + 标准圆角/边框, 与侧栏面板/窗口视觉一致
/// - pin / wire 尺寸按 egui 标准 interact_size 推导, 与其它可交互控件高度对齐
/// - 允许缩放范围 0.2x ~ 2.5x
fn make_snarl_style(ui: &egui::Ui) -> SnarlStyle {
    let style = ui.style();
    let visuals = ui.visuals();
    let interact_h = style.spacing.interact_size.y;

    let mut snarl_style = SnarlStyle::default();

    // 节点 frame: 与主题窗口/面板一致的圆角 + 边框
    let node_frame = egui::Frame::group(style)
        .fill(visuals.widgets.inactive.bg_fill)
        .stroke(egui::Stroke::new(1.0, visuals.window_stroke.color))
        .corner_radius(visuals.widgets.noninteractive.corner_radius)
        .inner_margin(egui::Margin::same(6))
        .outer_margin(egui::Margin::ZERO);
    snarl_style.node_frame = Some(node_frame);

    // 缩放范围
    snarl_style.min_scale = Some(0.2);
    snarl_style.max_scale = Some(2.5);

    // pin 尺寸与可交互控件高度对齐 (默认 ~0.6 * interact_size)
    snarl_style.pin_size = Some(interact_h * 0.55);
    // 连线宽度随 pin 尺寸
    snarl_style.wire_width = Some(interact_h * 0.06);

    snarl_style
}

/// 围绕屏幕点 `anchor` 调整缩放, 保持 anchor 下的图空间点不动
fn rescale_around(
    t: &egui::emath::TSTransform,
    new_scale: f32,
    anchor: egui::Pos2,
) -> egui::emath::TSTransform {
    // 当前 anchor 处的图空间点
    let from = (anchor.to_vec2() - t.translation) / t.scaling;
    egui::emath::TSTransform {
        scaling: new_scale,
        translation: anchor.to_vec2() - from * new_scale,
    }
}

/// 计算所有节点位置的包围盒 (图空间), 供 FitToContent / minimap 使用
fn nodes_bounding_box(snarl: &Snarl<GraphNode>) -> egui::Rect {
    let mut bb = egui::Rect::NOTHING;
    for (pos, _node) in snarl.nodes_pos() {
        bb.extend_with(pos);
    }
    bb
}

/// SnarlViewer 实现 — 渲染节点/端口/菜单, 并把改动标 dirty
pub struct GraphViewer<'a> {
    state: &'a Arc<AppState>,
    rt: &'a tokio::runtime::Runtime,
    dirty: &'a mut bool,
    next_auto_id: &'a mut usize,
    /// 下一帧待执行的视图操作 (由 minimap/缩放控件写入, current_transform 消费)
    pending_view_action: &'a mut Option<ViewAction>,
    /// 本帧从 snarl 捕获的图->屏变换 (current_transform 写入, overlay 读取)
    captured_to_global: &'a mut egui::emath::TSTransform,
    /// 本帧 snarl 占用的屏幕矩形 (overlay 定位用)
    captured_view_rect: &'a mut egui::Rect,
    /// 当前主题是否深色 (header 着色取色用)
    dark: bool,
}

impl<'a> GraphViewer<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: &'a Arc<AppState>,
        rt: &'a tokio::runtime::Runtime,
        dirty: &'a mut bool,
        next_auto_id: &'a mut usize,
        pending_view_action: &'a mut Option<ViewAction>,
        captured_to_global: &'a mut egui::emath::TSTransform,
        captured_view_rect: &'a mut egui::Rect,
        dark: bool,
    ) -> Self {
        Self {
            state,
            rt,
            dirty,
            next_auto_id,
            pending_view_action,
            captured_to_global,
            captured_view_rect,
            dark,
        }
    }

    /// 缩放一档的比例
    const ZOOM_STEP: f32 = 1.25;
    /// 允许的缩放范围 (与 make_snarl_style 一致)
    const MIN_SCALE: f32 = 0.2;
    const MAX_SCALE: f32 = 2.5;

    /// 在 current_transform 中消费 pending_view_action, 调整 to_global.
    /// 以上一帧捕获的屏幕矩形 (captured_view_rect) 为参照.
    fn apply_view_action(
        &self,
        action: ViewAction,
        to_global: &mut egui::emath::TSTransform,
        snarl: &Snarl<GraphNode>,
    ) {
        let view_rect = *self.captured_view_rect;
        if !view_rect.is_finite() {
            return;
        }
        let center = view_rect.center();
        match action {
            ViewAction::ZoomIn => {
                let new_scale =
                    (to_global.scaling * Self::ZOOM_STEP).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
                *to_global = rescale_around(to_global, new_scale, center);
            }
            ViewAction::ZoomOut => {
                let new_scale =
                    (to_global.scaling / Self::ZOOM_STEP).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
                *to_global = rescale_around(to_global, new_scale, center);
            }
            ViewAction::ResetZoom => {
                *to_global = rescale_around(to_global, 1.0, center);
            }
            ViewAction::CenterAt(graph_pos) => {
                // 保持当前缩放, 平移使 graph_pos 映射到屏幕中心
                let s = to_global.scaling;
                to_global.translation = center.to_vec2() - graph_pos.to_vec2() * s;
            }
            ViewAction::FitToContent => {
                let bb = nodes_bounding_box(snarl);
                if !bb.is_finite() {
                    return;
                }
                let padded = bb.expand(80.0);
                let size = padded.size();
                if size.x <= 0.0 || size.y <= 0.0 {
                    return;
                }
                let scale = (view_rect.size() / size)
                    .min_elem()
                    .clamp(Self::MIN_SCALE, Self::MAX_SCALE);
                let graph_center = padded.center();
                to_global.scaling = scale;
                to_global.translation = center.to_vec2() - graph_center.to_vec2() * scale;
            }
        }
    }

    /// Input 节点体 — 可交互的 Slider + DragValue, 变化时写回 input_values 并按绑定下发
    fn show_input_body(&mut self, node: NodeId, ui: &mut egui::Ui, snarl: &mut Snarl<GraphNode>) {
        let (id, control) = {
            let n = &snarl[node];
            (n.id.clone(), n.control.clone())
        };

        let mut value = self
            .state
            .input_values
            .lock()
            .get(&id)
            .copied()
            .unwrap_or(0.0);

        let mut changed = false;
        ui.horizontal(|ui| {
            let slider = egui::Slider::new(&mut value, control.min..=control.max)
                .step_by(f64::from(control.step.max(1e-6)))
                .clamping(egui::SliderClamping::Always);
            // 滑块占据可用宽度, 与侧栏面板控件尺寸一致
            if ui
                .add_sized(
                    [ui.available_width() - 64.0, ui.spacing().interact_size.y],
                    slider,
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .add(egui::DragValue::new(&mut value).speed(f64::from(control.step)))
                .changed()
            {
                changed = true;
            }
        });

        if changed {
            self.state.input_values.lock().insert(id.clone(), value);
            let state = self.state.clone();
            let id2 = id.clone();
            self.rt.spawn(async move {
                if let Err(e) = services::set_input_value(&state, id2, value).await {
                    tracing::warn!("set_input_value failed: {e}");
                }
            });
            let state = self.state.clone();
            let binding = control.binding.clone();
            self.rt.spawn(async move {
                if let Err(e) = services::send_widget_value(&state, binding, value).await {
                    tracing::warn!("send_widget_value failed: {e}");
                }
            });
        }

        // 控件参数设置 (min/max/step/绑定)
        ui.menu_button("⚙", |ui| {
            let n = &mut snarl[node];
            ui.horizontal(|ui| {
                ui.label("min");
                ui.add(egui::DragValue::new(&mut n.control.min));
                ui.label("max");
                ui.add(egui::DragValue::new(&mut n.control.max));
                ui.label("step");
                ui.add(egui::DragValue::new(&mut n.control.step).speed(0.1));
            });
            ui.separator();
            ui.label("Binding");
            let mode = match n.control.binding {
                WidgetBinding::None => 0,
                WidgetBinding::Auto { .. } => 1,
                WidgetBinding::Manual { .. } => 2,
            };
            let mut new_mode = mode;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut new_mode, 0, "None");
                ui.selectable_value(&mut new_mode, 1, "Auto");
                ui.selectable_value(&mut new_mode, 2, "Manual");
            });
            if new_mode != mode {
                n.control.binding = match new_mode {
                    1 => WidgetBinding::Auto { channel: 0 },
                    2 => WidgetBinding::Manual {
                        template: String::new(),
                    },
                    _ => WidgetBinding::None,
                };
            }
            match &mut n.control.binding {
                WidgetBinding::Auto { channel } => {
                    ui.horizontal(|ui| {
                        ui.label("channel");
                        ui.add(egui::DragValue::new(channel));
                    });
                }
                WidgetBinding::Manual { template } => {
                    ui.horizontal(|ui| {
                        ui.label("template");
                        ui.text_edit_singleline(template);
                    });
                }
                WidgetBinding::None => {}
            }
        });
    }

    /// Sink 节点体 — 从上游节点解析输入值, 按显示方式渲染 Number/Gauge/Led
    fn show_sink_body(&mut self, node: NodeId, ui: &mut egui::Ui, snarl: &mut Snarl<GraphNode>) {
        let (display, sink_min, sink_max) = {
            let n = &snarl[node];
            (n.sink_display, n.sink_min, n.sink_max)
        };

        let value = {
            let snapshot = self.state.output_snapshot.lock();
            sink_input_value(snarl, node, &snapshot)
        };

        match value {
            Some(v) => match display {
                SinkDisplayKind::Number => controls::number_display(ui, v),
                // 表盘宽度跟随节点可用宽度
                SinkDisplayKind::Gauge => {
                    controls::gauge(ui, v, sink_min, sink_max, ui.available_width().max(120.0));
                }
                SinkDisplayKind::Led => {
                    ui.horizontal(|ui| {
                        controls::led(ui, v, 0.0, 10.0);
                        // 与其它控件一致的 Body 字号
                        ui.label(format!("{v:.2}"));
                    });
                }
            },
            None => {
                ui.label("value: —");
            }
        }

        // 显示设置 (显示方式 + Gauge 区间)
        ui.menu_button("⚙", |ui| {
            let n = &mut snarl[node];
            ui.horizontal(|ui| {
                ui.selectable_value(&mut n.sink_display, SinkDisplayKind::Number, "Number");
                ui.selectable_value(&mut n.sink_display, SinkDisplayKind::Gauge, "Gauge");
                ui.selectable_value(&mut n.sink_display, SinkDisplayKind::Led, "LED");
            });
            if n.sink_display == SinkDisplayKind::Gauge {
                ui.horizontal(|ui| {
                    ui.label("min");
                    ui.add(egui::DragValue::new(&mut n.sink_min));
                    ui.label("max");
                    ui.add(egui::DragValue::new(&mut n.sink_max));
                });
            }
        });
    }
}

/// 解析 Sink 节点 "value" 输入端口的当前值
///
/// 沿 snarl 连线找到上游节点/端口, 再从 output_snapshot 取该端口最新值
fn sink_input_value(
    snarl: &Snarl<GraphNode>,
    node: NodeId,
    snapshot: &crate::core::state::GraphOutputSnapshot,
) -> Option<f32> {
    let in_pin = snarl.in_pin(InPinId { node, input: 0 });
    let out_id = *in_pin.remotes.first()?;
    let src = snarl.get_node(out_id.node)?;
    let port = output_ports(&src.kind).get(out_id.output)?.clone();
    snapshot.values.get(&src.id)?.get(&port).copied()
}

impl SnarlViewer<GraphNode> for GraphViewer<'_> {
    fn title(&mut self, node: &GraphNode) -> String {
        node.label.clone()
    }

    /// 节点 frame: 保持默认 (由 SnarlStyle.node_frame 提供, 与全应用 UI 一致)
    fn node_frame(
        &mut self,
        default: egui::Frame,
        _node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        _snarl: &Snarl<GraphNode>,
    ) -> egui::Frame {
        default
    }

    /// 节点 header frame: 按节点类型着色, 使不同类型节点视觉上易于区分
    fn header_frame(
        &mut self,
        default: egui::Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<GraphNode>,
    ) -> egui::Frame {
        let dark = self.dark;
        let accent = snarl
            .get_node(node)
            .map(|n| node_accent_color(&n.kind, dark))
            .unwrap_or(default.fill);
        default.fill(header_fill(accent, dark))
    }

    /// 捕获 snarl 当前的图->屏变换, 供 minimap/缩放控件使用;
    /// 同时消费 pending_view_action (缩放/定位).
    fn current_transform(
        &mut self,
        to_global: &mut egui::emath::TSTransform,
        snarl: &mut Snarl<GraphNode>,
    ) {
        // 1) 消费待执行的视图操作
        if let Some(action) = self.pending_view_action.take() {
            self.apply_view_action(action, to_global, snarl);
        }
        // 2) 捕获本帧最终变换, 供 overlay 读取
        *self.captured_to_global = *to_global;
    }

    fn inputs(&mut self, node: &GraphNode) -> usize {
        input_ports(&node.kind).len()
    }

    fn outputs(&mut self, node: &GraphNode) -> usize {
        output_ports(&node.kind).len()
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let node = &snarl[pin.id.node];
        let name = input_ports(&node.kind)
            .get(pin.id.input)
            .cloned()
            .unwrap_or_else(|| format!("in{}", pin.id.input));
        ui.label(name);
        PinInfo::circle().with_fill(ui.visuals().widgets.active.bg_fill)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let node = &snarl[pin.id.node];
        let name = output_ports(&node.kind)
            .get(pin.id.output)
            .cloned()
            .unwrap_or_else(|| format!("out{}", pin.id.output));
        ui.label(name);
        PinInfo::circle().with_fill(ui.visuals().widgets.active.bg_fill)
    }

    fn has_body(&mut self, _node: &GraphNode) -> bool {
        true
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) {
        match &snarl[node].kind {
            NodeKind::Input => self.show_input_body(node, ui, snarl),
            NodeKind::Sink => self.show_sink_body(node, ui, snarl),
            _ => {
                // 其余节点: 展示各输出端口的当前值
                let graph_node = &snarl[node];
                let ports = output_ports(&graph_node.kind);
                let snapshot = self.state.output_snapshot.lock();
                let values = snapshot.values.get(&graph_node.id);
                for port in ports {
                    let text = match values.and_then(|m| m.get(&port)) {
                        Some(v) => format!("{port}: {v:.3}"),
                        None => format!("{port}: —"),
                    };
                    // 使用 Body 字号 (与其它面板控件一致), monospace 便于对齐数值
                    ui.label(egui::RichText::new(text).monospace());
                }
            }
        }
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<GraphNode>) -> bool {
        true
    }

    fn show_graph_menu(
        &mut self,
        pos: egui::Pos2,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) {
        ui.label("Add node");
        ui.separator();
        for name in ADD_NODE_MENU {
            if ui.button(*name).clicked() {
                if let Some((kind, label)) = default_node_kind(name) {
                    let id = format!("n{}", *self.next_auto_id);
                    *self.next_auto_id += 1;
                    snarl.insert_node(pos, GraphNode::new(id, kind, label));
                    *self.dirty = true;
                }
                ui.close();
            }
        }
    }

    fn has_node_menu(&mut self, _node: &GraphNode) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) {
        if ui.button("Delete Node").clicked() {
            snarl.remove_node(node);
            *self.dirty = true;
            ui.close();
        }
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<GraphNode>) {
        snarl.connect(from.id, to.id);
        *self.dirty = true;
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<GraphNode>) {
        snarl.disconnect(from.id, to.id);
        *self.dirty = true;
    }

    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<GraphNode>) {
        if snarl.drop_outputs(pin.id) > 0 {
            *self.dirty = true;
        }
    }

    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<GraphNode>) {
        if snarl.drop_inputs(pin.id) > 0 {
            *self.dirty = true;
        }
    }
}

/// 在指定 ui 中渲染节点编辑器 (占满整个页签区域), 并同步 dirty 图
pub fn show_node_editor(
    ui: &mut egui::Ui,
    tab_state: &mut ControlTabState,
    state: &Arc<AppState>,
    rt: &tokio::runtime::Runtime,
    tab_id: u64,
) {
    sync_if_dirty(tab_state, state, rt, tab_id);

    // Sink/Input 显示值随数据流刷新 (data_loop 推送新快照后重绘)
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));

    let dark = ui.visuals().dark_mode;
    // 本帧 snarl 将占据的屏幕矩形 (供 overlay 定位)
    let editor_rect = ui.max_rect();

    let ControlTabState {
        snarl,
        next_auto_id,
        dirty,
        pending_view_action,
        minimap_visible,
        captured_to_global,
        captured_view_rect,
    } = &mut *tab_state;

    let mut viewer = GraphViewer::new(
        state,
        rt,
        dirty,
        next_auto_id,
        pending_view_action,
        captured_to_global,
        captured_view_rect,
        dark,
    );
    let style = make_snarl_style(ui);
    snarl.show(&mut viewer, &style, format!("node-editor-{tab_id}"), ui);

    // 记录本帧 snarl 实际占据的屏幕矩形, 供下一帧 current_transform / overlay 使用
    *captured_view_rect = editor_rect;

    // ---------------- 页面缩略图 (minimap) + 缩放控制组合 overlay ----------------
    if editor_rect.is_finite() {
        draw_minimap_and_zoom(
            ui,
            editor_rect,
            *captured_to_global,
            snarl,
            dark,
            *minimap_visible,
            pending_view_action,
            minimap_visible,
        );
    }
}
// ===========================================================================
// 页面缩略图 (minimap) + 缩放控制组合 overlay
// ===========================================================================

/// minimap 默认尺寸
const MINIMAP_SIZE: egui::Vec2 = egui::vec2(168.0, 116.0);
/// overlay 与编辑区边缘的留白
const OVERLAY_MARGIN: f32 = 12.0;
/// minimap 中节点矩形的名义尺寸 (图空间, 仅用于缩略绘制, 真实尺寸由 snarl 内部管理)
const NODE_NOMINAL_SIZE: egui::Vec2 = egui::vec2(140.0, 64.0);

/// 绘制页面缩略图 + 缩放控制组合 (浮动在节点编辑器右上角)
///
/// - `editor_rect`: 本帧 snarl 占据的屏幕矩形
/// - `to_global`: 图->屏 变换 (本帧捕获)
/// - `snarl`: 节点图 (只读, 取节点位置/类型)
/// - `pending`: 写入待执行的视图操作 (缩放/定位)
/// - `minimap_visible_flag`: minimap 显隐开关 (可由用户切换)
#[allow(clippy::too_many_arguments)]
fn draw_minimap_and_zoom(
    ui: &mut egui::Ui,
    editor_rect: egui::Rect,
    to_global: egui::emath::TSTransform,
    snarl: &Snarl<GraphNode>,
    dark: bool,
    minimap_visible: bool,
    pending: &mut Option<ViewAction>,
    minimap_visible_flag: &mut bool,
) {
    // 仅在编辑区有限时绘制 (避免首帧 / 隐藏页签异常)
    if !editor_rect.is_finite() {
        return;
    }

    let panel_w = MINIMAP_SIZE.x + 16.0; // 含 frame 内边距
    let panel_pos = egui::pos2(
        editor_rect.right() - panel_w - OVERLAY_MARGIN,
        editor_rect.top() + OVERLAY_MARGIN,
    );

    let area_id = ui.id().with("node-editor-overlay");
    let visuals = ui.visuals().clone();

    egui::Area::new(area_id)
        .order(egui::Order::Foreground)
        .fixed_pos(panel_pos)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    // 标题栏: 标题 + 折叠按钮
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Overview")
                                .small()
                                .strong()
                                .color(visuals.widgets.inactive.fg_stroke.color),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (icon, new_vis) = if minimap_visible {
                                ("▾", false)
                            } else {
                                ("▸", true)
                            };
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(icon).small())
                                        .frame(false),
                                )
                                .clicked()
                            {
                                *minimap_visible_flag = new_vis;
                            }
                        });
                    });

                    if minimap_visible {
                        ui.add_space(2.0);
                        draw_minimap(ui, editor_rect, to_global, snarl, dark, pending, &visuals);
                        ui.add_space(4.0);
                    }
                    draw_zoom_controls(ui, to_global, pending, &visuals);
                });
        });
}

/// 绘制 minimap 缩略图, 并处理点击/拖拽导航
fn draw_minimap(
    ui: &mut egui::Ui,
    editor_rect: egui::Rect,
    to_global: egui::emath::TSTransform,
    snarl: &Snarl<GraphNode>,
    dark: bool,
    pending: &mut Option<ViewAction>,
    visuals: &egui::Visuals,
) {
    let nav_resp = ui.allocate_response(MINIMAP_SIZE, egui::Sense::click_and_drag());
    let minimap_rect = nav_resp.rect;

    // 图空间: 当前视口 + 所有节点
    let from_global = to_global.inverse();
    let graph_viewport = from_global * editor_rect;
    let content_bb = nodes_bounding_box(snarl);
    let world = if content_bb.is_finite() {
        content_bb.union(graph_viewport)
    } else {
        graph_viewport
    }
    .expand(60.0);

    if !world.is_finite() || world.size().min_elem() <= 0.0 {
        ui.painter()
            .rect_filled(minimap_rect, 4.0, visuals.extreme_bg_color);
        return;
    }

    // world -> minimap 等比映射 (居中)
    let ms = minimap_rect.size();
    let ws = world.size();
    let scale = (ms.x / ws.x).min(ms.y / ws.y).max(1e-6);
    let drawn_size = ws * scale;
    let offset = minimap_rect.min + (ms - drawn_size) / 2.0;
    let w2m = |gp: egui::Pos2| -> egui::Pos2 { offset + (gp - world.min) * scale };

    let painter = ui.painter();

    // 背景
    painter.rect_filled(minimap_rect, 4.0, visuals.extreme_bg_color);

    // 网格底纹 (淡淡的)
    painter.rect_stroke(
        minimap_rect,
        4.0,
        egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );

    // 节点缩略矩形
    for (pos, node) in snarl.nodes_pos() {
        let center = w2m(pos);
        let node_size = (NODE_NOMINAL_SIZE * scale).max(egui::vec2(2.5, 2.5));
        let accent = node_accent_color(&node.kind, dark);
        let rect = egui::Rect::from_center_size(center, node_size);
        painter.rect_filled(rect, 1.5, accent.gamma_multiply(0.85));
    }

    // 当前视口框
    let vp_min = w2m(graph_viewport.min);
    let vp_max = w2m(graph_viewport.max);
    let vp_rect = egui::Rect::from_min_max(vp_min, vp_max);
    if vp_rect.is_finite() {
        painter.rect_filled(vp_rect, 2.0, visuals.selection.bg_fill.gamma_multiply(0.35));
        painter.rect_stroke(
            vp_rect,
            2.0,
            egui::Stroke::new(1.5, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }

    // 点击/拖拽导航: 把 minimap 内指针位置映射回图空间, 居中视口
    if let Some(pos) = nav_resp
        .interact_pointer_pos()
        .filter(|p| minimap_rect.contains(*p))
    {
        // minimap 点 -> 图空间点
        let graph_point = world.min + (pos - offset) / scale;
        if graph_point.is_finite() {
            *pending = Some(ViewAction::CenterAt(graph_point));
        }
    }
}

/// 绘制缩放控制按钮组 (放大 / 缩小 / 重置 / 适应)
fn draw_zoom_controls(
    ui: &mut egui::Ui,
    to_global: egui::emath::TSTransform,
    pending: &mut Option<ViewAction>,
    _visuals: &egui::Visuals,
) {
    ui.horizontal(|ui| {
        let zoom_pct = (to_global.scaling * 100.0).round() as i32;
        if ui
            .add(egui::Button::new("−").min_size(egui::vec2(26.0, ui.spacing().interact_size.y)))
            .clicked()
        {
            *pending = Some(ViewAction::ZoomOut);
        }
        if ui
            .add(egui::Button::new("+").min_size(egui::vec2(26.0, ui.spacing().interact_size.y)))
            .clicked()
        {
            *pending = Some(ViewAction::ZoomIn);
        }
        // 当前缩放比例显示 + 重置按钮
        if ui
            .add(
                egui::Button::new(format!("{zoom_pct}%"))
                    .min_size(egui::vec2(48.0, ui.spacing().interact_size.y)),
            )
            .clicked()
        {
            *pending = Some(ViewAction::ResetZoom);
        }
        if ui
            .add(egui::Button::new("Fit").min_size(egui::vec2(40.0, ui.spacing().interact_size.y)))
            .clicked()
        {
            *pending = Some(ViewAction::FitToContent);
        }
    });
}
