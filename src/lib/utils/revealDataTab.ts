//! 数据面板入口的统一跳转副作用。
//!
//! 所有「打开某个数据面板」的入口 (侧边栏 DataPanelsPanel / Windows MenuBar /
//! 原生 menu_shell 的 panel-open-* 事件) 都先经 store action 写 appStore 的
//! activeDataTabId 镜像; 但 Dock 卡片实际显示哪个 Tab 由 dockStore 卡片自身的
//! card.activeTabId 决定。本工具在打开动作之后同步把目标 Tab 所在 data 卡片
//! 切到该 Tab 并聚焦 — 缺了这一步, 目标卡片即使被聚焦也仍显示旧 Tab。
//!
//! 目标 Tab 尚未被任何卡片安置时 (刚新建, DockLayout reconcile 未跑), 这里
//! 静默跳过 — 新 Tab 由 reconcile 安置进焦点卡片并设为激活。

import { useAppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { NODE_PROPERTIES_TAB_ID } from '../../types';
import { transitionStore } from './transitionStore';

/// 执行 trigger (打开/轮转 store 动作), 随后激活目标 Tab 所在的 data 卡片。
/// 复用 CompileErrorItem / WidgetNode 跳画布的同一套 setActiveTab 原语:
/// 同时完成切可见 Tab、聚焦卡片、镜像全局 activeDataTabId。
export function openDataPanelAndReveal(trigger: () => void): void {
  transitionStore(() => {
    trigger();
    const tabId = useAppStore.getState().activeDataTabId;
    const cards = useDockStore.getState().cards;
    const card = Object.values(cards).find((c) => c.kind === 'data' && c.tabIds.includes(tabId));
    if (card) useDockStore.getState().setActiveTab(card.id, tabId);
  });
}

/// 打开/激活节点属性面板 — 创建 tab 时同步在画布右侧拆分专属小卡
/// (dockPropertiesTab 对已有归属的 tab 是 no-op, 尊重用户手动安排)。
/// 选中画布节点与数据面板菜单/侧边栏入口共用此路径。
export function openNodePropertiesPanel(): void {
  openDataPanelAndReveal(() => {
    useAppStore.getState().addNodePropertiesTab();
    useDockStore.getState().dockPropertiesTab(NODE_PROPERTIES_TAB_ID);
  });
}
