//! 控件 → 数据窗口 Tab 的映射
//!
//! 部分控件 (波形/原始数据/CAN 解码等) 在画布上是节点占位, 实际内容渲染在
//! 数据区 Dock 窗口 (DataTab) 中。此模块集中管理「控件 ↔ 窗口」的对应关系,
//! 供 addWidget (拖入自动开窗) 与 WidgetNode 双击重开窗口共用。

import type { DataTab, DataTabType, WidgetConfig } from '../../types';

/// 控件是否拥有可独立开合的窗口, 以及对应的 Tab 类型
/// 与 addWidget 的建 Tab 逻辑保持一致: 仅这些控件会创建数据窗口
export function widgetTabType(kind: WidgetConfig['kind']): DataTabType | null {
  switch (kind) {
    case 'Waveform':
      return 'waveform-extra';
    case 'PieChart':
      return 'pie';
    case 'Image':
      return 'image';
    case 'Model3D':
      return 'model3d';
    case 'Spectrum':
      return 'spectrum';
    case 'Command':
      return 'command';
    case 'FrameDecoder':
      return 'frame-decoder';
    case 'RawData':
      return 'raw';
    case 'Trigger':
      return 'trigger';
    default:
      return null;
  }
}

/// 窗口 Tab 显示名 — switch 收窄 union 以便安全访问各 params
function windowTabName(widget: WidgetConfig): string {
  switch (widget.kind) {
    case 'Waveform':
      return 'Waveform';
    case 'PieChart':
    case 'Image':
    case 'Model3D':
    case 'Spectrum':
    case 'Command':
    case 'FrameDecoder':
    case 'RawData':
    case 'Trigger':
      return widget.params.label;
    default:
      return widget.kind;
  }
}

/// 由控件构造数据窗口 Tab; 无窗口的控件返回 null
/// Tab id 与控件 id 相同, 保证「关闭窗口后双击节点可重新打开」
export function widgetToTab(widget: WidgetConfig): DataTab | null {
  const type = widgetTabType(widget.kind);
  if (!type) return null;
  return {
    id: widget.params.id,
    type,
    name: windowTabName(widget),
    widgetId: widget.params.id,
    closable: true,
  };
}
