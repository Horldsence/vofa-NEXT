//! 帮助中心内容配置
//!
//! 章节数据集中管理，便于帮助中心和引导复用。
//! 与首次使用向导 (OnboardingWizard) 的叙事口径保持一致:
//! 模板起步 → 控件库 → 连线建卡 → 编译反馈 (错误定位 + 结果表增删连接)
//! → CAN / 逻辑分析仪 → 自定义控件 → 窗口组织。

import {
  Lightbulb,
  Cable,
  Binary,
  LayoutGrid,
  ListChecks,
  Cpu,
  CircuitBoard,
  BookOpen,
  PanelsTopLeft,
  Scaling,
  type LucideIcon,
} from 'lucide-react';

export interface HelpSection {
  id: string;
  icon: LucideIcon;
  titleKey: string;
  descKey: string;
  stepsKey: string;
}

export const HELP_SECTIONS: HelpSection[] = [
  {
    id: 'quick-start',
    icon: Lightbulb,
    titleKey: 'helpCenterQuickStart',
    descKey: 'helpCenterQuickStartDesc',
    stepsKey: 'helpCenterQuickStartSteps',
  },
  {
    id: 'transport',
    icon: Cable,
    titleKey: 'helpTransport',
    descKey: 'helpTransportDesc',
    stepsKey: 'helpTransportSteps',
  },
  {
    id: 'protocol',
    icon: Binary,
    titleKey: 'helpProtocol',
    descKey: 'helpProtocolDesc',
    stepsKey: 'helpProtocolSteps',
  },
  {
    id: 'widgets',
    icon: LayoutGrid,
    titleKey: 'helpWidgets',
    descKey: 'helpWidgetsDesc',
    stepsKey: 'helpWidgetsSteps',
  },
  {
    /// 新设计一等公民: 编译错误面板 (定位/高亮) + 编译结果表 (HIR 连接的增删改查)
    id: 'compile',
    icon: ListChecks,
    titleKey: 'helpCompile',
    descKey: 'helpCompileDesc',
    stepsKey: 'helpCompileSteps',
  },
  {
    id: 'can',
    icon: Cpu,
    titleKey: 'helpCan',
    descKey: 'helpCanDesc',
    stepsKey: 'helpCanSteps',
  },
  {
    id: 'logic',
    icon: CircuitBoard,
    titleKey: 'helpLogic',
    descKey: 'helpLogicDesc',
    stepsKey: 'helpLogicSteps',
  },
  {
    id: 'custom',
    icon: BookOpen,
    titleKey: 'helpCustom',
    descKey: 'helpCustomDesc',
    stepsKey: 'helpCustomSteps',
  },
  {
    id: 'window-organize',
    icon: PanelsTopLeft,
    titleKey: 'helpWindowOrganize',
    descKey: 'helpWindowOrganizeDesc',
    stepsKey: 'helpWindowOrganizeSteps',
  },
  {
    id: 'window-resize',
    icon: Scaling,
    titleKey: 'helpWindowResize',
    descKey: 'helpWindowResizeDesc',
    stepsKey: 'helpWindowResizeSteps',
  },
];
