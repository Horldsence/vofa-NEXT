import { useCallback } from 'react';
import { Bug } from 'lucide-react';
import { useContextMenuStore } from '../store/contextMenuStore';
import { useSettingsStore } from '../store/settingsStore';
import { useAppStore } from '../store/appStore';
import { api } from './tauri';
import { t } from '../i18n';
import type { Lang } from '../i18n';
import type { ContextMenuEntry, ContextMenuItem } from '../types';

/// 保留原生右键的元素标签，避免破坏输入体验
const NATIVE_RIGHT_CLICK_TAGS = new Set(['INPUT', 'TEXTAREA']);

/// 构建检查元素菜单项
function buildInspectItem(lang: Lang): ContextMenuItem {
  return {
    id: 'inspect-element',
    label: t(lang, 'contextMenuInspectElement'),
    icon: <Bug size={14} />,
    onClick: () => {
      api.inspectElement().catch((e: unknown) => {
        console.warn('[context-menu] 打开开发者工具失败:', e);
      });
    },
  };
}

/// 在 debug 模式下向菜单列表追加检查元素项
function appendInspectElementIfDebug(items: ContextMenuEntry[], lang: Lang): void {
  const debug = useSettingsStore.getState().settings.general.debug;
  if (!debug) return;
  if (items.length > 0) {
    items.push({ kind: 'separator' });
  }
  items.push(buildInspectItem(lang));
}

export function useContextMenu(items: ContextMenuEntry[] | (() => ContextMenuEntry[])) {
  const open = useContextMenuStore((s) => s.open);
  const debug = useSettingsStore((s) => s.settings.general.debug);
  const lang = useAppStore((s) => s.lang);

  return useCallback(
    (e: React.MouseEvent) => {
      const target = e.target as HTMLElement;
      const tag = target.tagName;
      const isContentEditable = target.isContentEditable;

      // 输入类元素保留浏览器默认右键菜单
      if (NATIVE_RIGHT_CLICK_TAGS.has(tag) || isContentEditable) {
        return;
      }

      e.preventDefault();
      e.stopPropagation();

      const resolved = typeof items === 'function' ? items() : items;

      if (debug) {
        // 输出调试数据到控制台
        console.log('[context-menu] debug', {
          clientX: e.clientX,
          clientY: e.clientY,
          targetTag: tag,
          targetClass: target.className,
          targetId: target.id,
          itemCount: resolved.length,
          items: resolved,
        });
      }

      appendInspectElementIfDebug(resolved, lang);

      if (resolved.length === 0) return;

      open(e.clientX, e.clientY, resolved);
    },
    [items, open, debug, lang]
  );
}

/// 手动触发右键菜单（用于非 React 事件或测试）
export function showContextMenu(
  x: number,
  y: number,
  items: ContextMenuEntry[]
) {
  const lang = useAppStore.getState().lang;
  appendInspectElementIfDebug(items, lang);
  useContextMenuStore.getState().open(x, y, items);
}
