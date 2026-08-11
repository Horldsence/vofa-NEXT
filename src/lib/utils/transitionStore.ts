//! 将非紧急的 store 动作包装进 React `startTransition`.
//!
//! zustand 通过 useSyncExternalStore 订阅 store — 在 startTransition 作用域内
//! 触发的更新会被 React 视为 transition: 渲染可中断、可延迟, 不阻塞紧急交互
//! (连接 / 数据采集 / 画布拖拽 / 波形交互等)。
//!
//! 用法 (在调用点包装 store 动作):
//!   transitionStore(() => useAppStore.getState().setActiveDataTab(id))
//!   transitionStore(() => toggleSidebar('widgets'))
//!
//! 仅用于非紧急 UI 状态 (tab 切换 / 侧边栏视图与显隐 / 主题外观应用);
//! 紧急路径应直接调用 store 动作, 不要经过本工具。

import { startTransition } from 'react';

/// 在 startTransition 内同步调用 store 动作 — 动作本体立即执行, 但对应的
/// React 渲染被延迟为 transition 优先级
export function transitionStore(action: () => void): void {
  startTransition(action);
}
