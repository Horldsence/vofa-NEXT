// ============ Widget 注册表类型契约 ============
//
// 所有 widget 组件与属性编辑器的统一 props 契约, 以及单控件声明式定义
// WidgetDef — registry.ts 聚合全部 WidgetDef, 成为唯一的 kind→实现 分发点。
//
// 组件契约: widget 组件是「纯内容」— 不含卡片 chrome (节点框/端口/删除按钮由
// WidgetNode 提供, 数据窗口外壳由 WidgetSurface 提供), 统一 memo。

import type { ComponentType } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DomainType, WidgetConfig } from '../../types';

export type WidgetKind = WidgetConfig['kind'];

export interface WidgetComponentProps<K extends WidgetKind = WidgetKind> {
  widget: Extract<WidgetConfig, { kind: K }>;
}

/// 属性面板编辑器 — update 收同 kind 的新配置 (写穿 store.updateWidget)
export interface WidgetPropertiesProps<K extends WidgetKind = WidgetKind> {
  widget: Extract<WidgetConfig, { kind: K }>;
  update: (next: Extract<WidgetConfig, { kind: K }>) => void;
}

export interface WidgetPortDef {
  id: string;
  label: string;
  domain: DomainType;
}

export interface WidgetPortSet {
  inputs: WidgetPortDef[];
  outputs: WidgetPortDef[];
}

/// 单个控件 kind 的声明式定义
export interface WidgetDef<K extends WidgetKind = WidgetKind> {
  kind: K;
  /// 节点内 / 数据窗口共用的内容组件
  Component: ComponentType<WidgetComponentProps<K>>;
  /// 属性面板专用编辑节; 缺省 = 仅通用字段 (名称/尺寸)
  Properties?: ComponentType<WidgetPropertiesProps<K>>;
  /// 端口表 (输入/输出); 缺省 = 单 value 输入
  ports?(widget: Extract<WidgetConfig, { kind: K }>): WidgetPortSet;
  /// palette / 历史面板图标
  icon: LucideIcon;
  /// i18n key (widget.<labelKey>) — palette 与窗口标题共用
  labelKey: string;
  /// 节点头部 ⚙ 编辑入口 (如 Custom JS 编辑器)
  customEditor?: 'custom-js';
}
