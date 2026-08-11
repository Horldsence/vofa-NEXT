//! 帮助中心内容配置
//!
//! 章节数据集中管理，便于帮助中心和引导复用。

import {
  Lightbulb,
  Cable,
  Binary,
  LayoutGrid,
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

/// 引导步骤顺序（从 HELP_SECTIONS 中选取）
export const WIZARD_STEPS = [
  'quick-start',
  'transport',
  'protocol',
  'widgets',
  'can',
  'logic',
];
