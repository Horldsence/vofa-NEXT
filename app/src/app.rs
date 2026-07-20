//! VofaApp — eframe 应用根, 组合菜单栏/状态栏/活动栏/侧栏/停靠区

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui;
use egui_dock::{DockArea, DockState, TabPath};

use crate::core::AppState;
use crate::settings::AppSettings;
use crate::ui::displays::DataTabState;
use crate::ui::dock::{DataKind, DockViewer, Tab};
use crate::ui::layout::activity_bar::{self, ActivityItem};
use crate::ui::layout::{menu, sidebar, status_bar};
use crate::ui::node_editor::{self, ControlTabState};
use crate::ui::panels::{Panels, SettingsPanel};
use crate::ui::toasts::ToastQueue;
use vofa_next_core::ConnectionState;

/// 新手引导步骤 (标题, 正文)
const ONBOARDING_STEPS: [(&str, &str); 3] = [
    (
        "Welcome",
        "VOFA-NEXT is a cross-platform serial data debugging tool.\n\
         Connect a transport, pick a protocol, and visualize your data.",
    ),
    (
        "Transport",
        "Open the Transport panel in the sidebar to configure a serial port,\n\
         UDP/TCP socket, CAN adapter, or the built-in test data generator,\n\
         then press Connect.",
    ),
    (
        "Node Editor",
        "Each Control tab is a node canvas. Click widgets in the Widgets palette\n\
         to add nodes, wire inputs to displays, and drive your outputs.",
    ),
];

pub struct VofaApp {
    /// 后端核心状态 (de-Taurified), services 与 UI 共享
    state: Arc<AppState>,
    /// Tokio 多线程运行时, 用于 spawn 异步 service 调用
    rt: Arc<tokio::runtime::Runtime>,

    /// 中央停靠区状态
    dock_state: DockState<Tab>,
    /// 页签 id 自增计数器
    next_tab_id: u64,
    /// Control 页签 id → 节点编辑器状态
    control_tabs: HashMap<u64, ControlTabState>,
    /// Data 页签 id → 数据显示状态
    data_tabs: HashMap<u64, DataTabState>,
    /// 当前活动 (聚焦) 的 Control 页签 id — 控件库点击添加的目标
    active_control_tab: Option<u64>,

    /// 侧栏是否可见
    sidebar_visible: bool,
    /// 活动栏当前选中的面板
    active_panel: ActivityItem,
    /// 侧栏面板实例 (Transport / Protocol / Widgets / Settings)
    panels: Panels,

    /// 应用内 Toast 通知队列
    toasts: ToastQueue,
    /// 上一帧的连接状态 (用于检测变化并推送 Toast)
    last_connection_state: ConnectionState,
    /// 启动时主题是否已应用 (set_visuals 需要 egui Context)
    theme_applied: bool,
    /// 本次会话是否已显示过新手引导
    onboarding_shown_this_session: bool,
    /// 新手引导当前步骤
    onboarding_step: usize,
}

impl VofaApp {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
        let state = Arc::new(AppState::new());
        let settings = AppSettings::load();

        // 应用持久化的缓冲区容量
        {
            let state = state.clone();
            let s = settings.clone();
            rt.spawn(async move {
                use crate::core::services;
                let _ = services::set_waveform_buffer_capacity(&state, s.waveform_buffer_capacity)
                    .await;
                let _ =
                    services::set_rawdata_buffer_capacity(&state, s.rawdata_buffer_capacity).await;
                let _ = services::set_can_buffer_capacity(&state, s.can_buffer_capacity).await;
                let _ = services::set_logic_buffer_capacity(&state, s.logic_buffer_capacity).await;
            });
        }

        Self {
            state,
            rt,
            dock_state: DockState::new(vec![Tab::control(1)]),
            next_tab_id: 2,
            control_tabs: HashMap::from([(1, ControlTabState::new(1))]),
            data_tabs: HashMap::new(),
            active_control_tab: Some(1),
            sidebar_visible: true,
            active_panel: ActivityItem::Transport,
            panels: Panels {
                settings: SettingsPanel::new(settings),
                ..Default::default()
            },
            toasts: ToastQueue::default(),
            last_connection_state: ConnectionState::Disconnected,
            theme_applied: false,
            onboarding_shown_this_session: false,
            onboarding_step: 0,
        }
    }

    /// 新建一个 Control 页签
    fn new_control_tab(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.control_tabs.insert(id, ControlTabState::new(id));
        self.dock_state.push_to_focused_leaf(Tab::control(id));
    }

    /// 新建一个 Data 页签
    fn new_data_tab(&mut self, kind: DataKind) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.data_tabs.insert(id, DataTabState::new());
        self.dock_state.push_to_focused_leaf(Tab::data(kind, id));
    }

    /// 回收已关闭 Data 页签的状态
    fn reconcile_data_tabs(&mut self) {
        let open_ids: std::collections::HashSet<u64> = self
            .dock_state
            .iter_all_tabs()
            .filter_map(|(_, tab)| match tab {
                Tab::Data { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        self.data_tabs.retain(|id, _| open_ids.contains(id));
    }

    /// 回收已关闭 Control 页签的状态, 并通知后端移除对应编译图
    fn reconcile_control_tabs(&mut self) {
        let open_ids: std::collections::HashSet<u64> = self
            .dock_state
            .iter_all_tabs()
            .filter_map(|(_, tab)| match tab {
                Tab::Control { id } => Some(*id),
                _ => None,
            })
            .collect();
        let closed: Vec<u64> = self
            .control_tabs
            .keys()
            .copied()
            .filter(|id| !open_ids.contains(id))
            .collect();
        for id in closed {
            self.control_tabs.remove(&id);
            node_editor::remove_backend_graph(&self.state, &self.rt, id);
        }
    }

    /// 把控件库条目添加到活动 Control 页签画布
    fn add_palette_widget(&mut self, palette_id: &'static str) {
        // 解析目标页签: 优先活动 Control 页签, 否则任一打开的 Control 页签
        let tab_id = match self.active_control_tab {
            Some(id) if self.control_tabs.contains_key(&id) => Some(id),
            _ => self
                .dock_state
                .iter_all_tabs()
                .find_map(|(_, tab)| match tab {
                    Tab::Control { id } => Some(*id),
                    _ => None,
                }),
        };
        let Some(tab_id) = tab_id else {
            return;
        };
        self.active_control_tab = Some(tab_id);

        let tab_state = self
            .control_tabs
            .entry(tab_id)
            .or_insert_with(|| ControlTabState::new(tab_id));
        if node_editor::add_palette_node(tab_state, palette_id) {
            node_editor::sync_if_dirty(tab_state, &self.state, &self.rt, tab_id);
        }
    }

    /// 记录当前聚焦的 Control 页签 (控件库点击添加的目标)
    fn track_active_control_tab(&mut self) {
        if let Some((_, Tab::Control { id })) = self.dock_state.find_active_focused() {
            self.active_control_tab = Some(*id);
            return;
        }
        // 聚焦的不是 Control 页签: 校验当前记录, 失效则回退到任一打开的 Control 页签
        let valid =
            matches!(self.active_control_tab, Some(id) if self.control_tabs.contains_key(&id));
        if !valid {
            self.active_control_tab =
                self.dock_state
                    .iter_all_tabs()
                    .find_map(|(_, tab)| match tab {
                        Tab::Control { id } => Some(*id),
                        _ => None,
                    });
        }
    }

    /// 关闭当前聚焦的页签
    fn close_active_tab(&mut self) {
        let Some(node_path) = self.dock_state.focused_leaf() else {
            return;
        };
        let Ok(leaf) = self.dock_state.leaf(node_path) else {
            return;
        };
        if leaf.tabs.is_empty() {
            return;
        }
        let path = TabPath::from((node_path, leaf.active));
        self.dock_state.remove_tab(path);
    }

    /// 打开设置 (当前占位: 切到活动栏 Settings 面板并展开侧栏)
    fn open_settings(&mut self) {
        self.active_panel = ActivityItem::Settings;
        self.sidebar_visible = true;
    }

    /// 处理全局快捷键 (Ctrl/Cmd 取决于平台)
    fn handle_shortcuts(&mut self, ui: &mut egui::Ui) {
        use egui::{Key, KeyboardShortcut, Modifiers};

        let (settings, new_tab, close_tab, toggle_sidebar) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::T)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::B)),
            )
        });

        if settings {
            self.open_settings();
        }
        if new_tab {
            self.new_control_tab();
        }
        if close_tab {
            self.close_active_tab();
        }
        if toggle_sidebar {
            self.sidebar_visible = !self.sidebar_visible;
        }
    }

    fn apply_menu_actions(&mut self, actions: menu::MenuActions) {
        if actions.open_settings {
            self.open_settings();
        }
        if actions.new_tab {
            self.new_control_tab();
        }
        if actions.close_tab {
            self.close_active_tab();
        }
        if actions.toggle_sidebar {
            self.sidebar_visible = !self.sidebar_visible;
        }
        if let Some(kind) = actions.add_data_view {
            self.new_data_tab(kind);
        }
    }

    /// 监测连接状态变化, 推送 Toast
    fn watch_connection_state(&mut self) {
        let conn = *self.state.connection_state.lock();
        if conn == self.last_connection_state {
            return;
        }
        match conn {
            ConnectionState::Connected => self.toasts.success("Transport connected"),
            ConnectionState::Disconnected => self.toasts.info("Transport disconnected"),
            ConnectionState::Error => self.toasts.error("Transport error"),
            ConnectionState::Connecting => {}
        }
        self.last_connection_state = conn;
    }

    /// 首次运行的新手引导向导 (每次会话只显示一次)
    fn render_onboarding(&mut self, ctx: &egui::Context) {
        if self.onboarding_shown_this_session || !self.panels.settings.show_onboarding() {
            return;
        }
        let mut open = true;
        let step = self.onboarding_step.min(ONBOARDING_STEPS.len() - 1);
        let (title, body) = ONBOARDING_STEPS[step];
        egui::Window::new(format!("Welcome to VOFA-NEXT — {title}"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(body);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(format!("{} / {}", step + 1, ONBOARDING_STEPS.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let last = step + 1 == ONBOARDING_STEPS.len();
                        if ui.button(if last { "Done" } else { "Next" }).clicked() {
                            if last {
                                self.onboarding_shown_this_session = true;
                            } else {
                                self.onboarding_step += 1;
                            }
                        }
                        if step > 0 && ui.button("Back").clicked() {
                            self.onboarding_step -= 1;
                        }
                    });
                });
            });
        // 右上角关闭 = 跳过引导
        if !open {
            self.onboarding_shown_this_session = true;
        }
    }
}

impl eframe::App for VofaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 启动时应用一次持久化主题
        if !self.theme_applied {
            self.theme_applied = true;
            crate::theme::apply(ui.ctx(), self.panels.settings.theme());
        }

        self.handle_shortcuts(ui);
        self.watch_connection_state();

        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            let actions = menu::menu_bar(ui);
            self.apply_menu_actions(actions);
        });

        egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
            status_bar::status_bar(ui, &self.state);
        });

        egui::Panel::left("activity_bar")
            .resizable(false)
            .exact_size(48.0)
            .show_inside(ui, |ui| {
                activity_bar::activity_bar(ui, &mut self.active_panel, &mut self.sidebar_visible);
            });

        sidebar::sidebar(
            ui,
            self.sidebar_visible,
            self.active_panel,
            &mut self.panels,
            &self.state,
            &self.rt,
        );

        // 控件库点击 → 添加节点到活动 Control 页签画布
        if let Some(palette_id) = self.panels.widget_palette.take_pending_add() {
            self.add_palette_widget(palette_id);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                DockArea::new(&mut self.dock_state).show_inside(
                    ui,
                    &mut DockViewer {
                        control_tabs: &mut self.control_tabs,
                        data_tabs: &mut self.data_tabs,
                        state: &self.state,
                        rt: &self.rt,
                    },
                );
            });

        self.track_active_control_tab();

        // 页签渲染后回收已关闭的页签状态 (含右上角关闭按钮路径)
        self.reconcile_control_tabs();
        self.reconcile_data_tabs();

        // 新手引导 + Toast 浮层 (最后渲染, 覆盖在停靠区之上)
        self.render_onboarding(ui.ctx());
        self.toasts.render(ui.ctx());
    }
}
