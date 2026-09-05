// ============ 端口域配色 ============
//
// 端口圆点/手柄描边与域标签的共享常量 — 组件文件只导出组件 (react-refresh),
// 故独立成模块供 WidgetNode / 占位组件共用。

import type { DomainType } from '../../../types';

/// 端口域颜色 — 频域紫色, 时域蓝色, 字节域黄色, 字符串域橙色
export function domainColor(domain: DomainType): string {
  return domain === 'freq' ? '#ba68c8' : domain === 'bytes' ? '#e5c07b' : domain === 'string' ? '#ffa726' : '#75beff';
}
