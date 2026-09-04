// ============ 节点属性数据面板 ============
//
// 数据面板系统中的「节点属性」tab (type: node-properties, 单例)。
// 选中画布节点 (widget / Transport / Protocol) 时由 NodeEditor 自动激活;
// 面板内容自派生选中节点 — data 卡片渲染在 ReactFlowProvider 之外,
// 不能依赖画布上下文, 直接读 appStore.rfNodes 的 selected 标记。

import { memo, useCallback } from 'react';
import type { Node } from '@xyflow/react';
import { X } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { t, type Lang } from '../../i18n';
import type { WidgetConfig } from '../../types';
import { WidgetProperties } from './WidgetProperties';
import { GlobalNodeProperties } from './GlobalNodeProperties';

/// 面板标题 — 控件节点显示 kind, 全局节点显示 数据接口/协议引擎
function panelTitle(lang: Lang, node: Node): string {
  if (node.type === 'widget') {
    const widget = node.data.widget as WidgetConfig | undefined;
    return widget?.kind ?? 'Widget';
  }
  return node.type === 'transport' ? t(lang, 'dataInterface') : t(lang, 'protocolEngine');
}

/// 面板副标题 — 控件节点的用户命名 label
function panelSubtitle(node: Node): string | undefined {
  if (node.type !== 'widget') return undefined;
  const widget = node.data.widget as WidgetConfig | undefined;
  const label = widget?.params.label;
  return label != null && label !== '' ? label : undefined;
}

export const NodePropertiesPanel = memo(function NodePropertiesPanel() {
  const lang = useAppStore((s) => s.lang);
  // 与 NodeEditor 的画布过滤一致: 全局节点跨 tab 可见, widget 节点按活跃控制页签
  const node = useAppStore((s) =>
    s.rfNodes.find(
      (n) =>
        n.selected &&
        (n.data?.global === true ||
          (n.type === 'widget' && n.data?.tabId === s.activeControlTabId)),
    ),
  );
  // 关闭按钮 = 取消选中 (select change 不入撤销历史); tab 本体由 tab 条 X 关闭
  const deselect = useCallback(() => {
    const st = useAppStore.getState();
    const selected = st.rfNodes.filter((n) => n.selected);
    if (selected.length) {
      st.onNodesChange(
        selected.map((n) => ({ id: n.id, type: 'select' as const, selected: false })),
      );
    }
  }, []);

  return (
    <div className="flex h-full w-full flex-col bg-bg-sidebar">
      {node ? (
        <>
          <div className="flex items-center gap-1.5 px-3 py-2 border-b border-border shrink-0">
            <span
              className="flex-1 min-w-0 truncate text-[10px] font-semibold uppercase tracking-wider text-text-secondary"
              title={panelTitle(lang, node)}
            >
              {panelTitle(lang, node)}
              {panelSubtitle(node) && (
                <span className="ml-1.5 font-normal normal-case tracking-normal text-text-muted">
                  {panelSubtitle(node)}
                </span>
              )}
            </span>
            <button
              type="button"
              className="w-5 h-5 shrink-0 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover transition-colors"
              onClick={deselect}
              title={t(lang, 'propertiesClose')}
              aria-label={t(lang, 'propertiesClose')}
            >
              <X size={12} />
            </button>
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto p-3">
            {node.type === 'widget' ? <WidgetProperties node={node} /> : <GlobalNodeProperties node={node} />}
          </div>
        </>
      ) : (
        <div className="flex-1 flex items-center justify-center px-4 text-center text-xs text-text-secondary">
          {t(lang, 'nodePropertiesEmpty')}
        </div>
      )}
    </div>
  );
});
