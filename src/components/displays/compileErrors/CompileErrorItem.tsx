import { memo, useCallback } from 'react';
import { MapPin } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { useDockStore } from '../../../store/dockStore';
import { transitionStore } from '../../../lib/utils/transitionStore';
import { compileErrorMessage } from '../../ui/CanvasErrorTooltip';
import { t } from '../../../i18n';
import type { CompileError } from '../../../store/slices/compileError';

interface CompileErrorItemProps {
  tabId: string;
  nodeId: string;
  error: CompileError;
}

export const CompileErrorItem = memo(function CompileErrorItem({
  tabId,
  nodeId,
  error,
}: CompileErrorItemProps) {
  const lang = useAppStore((s) => s.lang);
  const requestFlyTo = useAppStore((s) => s.requestFlyTo);

  const handleFlyTo = useCallback(() => {
    transitionStore(() => {
      // 1. 切到对应 control tab 的卡片
      const cards = useDockStore.getState().cards;
      const card = Object.values(cards).find(
        (c) => c.kind === 'control' && c.tabIds.includes(tabId)
      );
      if (card) {
        useDockStore.getState().setActiveTab(card.id, tabId);
        useDockStore.getState().setFocusedCard(card.id);
      }
      // 2. 排队 fly-to — NodeEditorInner useEffect 命中后 setCenter
      requestFlyTo(nodeId, tabId);
    });
  }, [tabId, nodeId, requestFlyTo]);

  const message = compileErrorMessage(error, nodeId);

  return (
    <div className="flex items-center justify-between gap-2 px-3 py-1.5 rounded-md hover:bg-bg-hover transition-colors group">
      <div className="flex items-center gap-2 min-w-0">
        <span className="text-xs font-mono px-1.5 py-0.5 rounded bg-bg-editor text-text-secondary border border-border-subtle shrink-0 select-none">
          {nodeId}
        </span>
        <span className="text-sm text-text-primary truncate" title={message}>
          {message}
        </span>
      </div>
      <button
        type="button"
        onClick={handleFlyTo}
        className="p-1 text-text-secondary hover:text-text-primary rounded hover:bg-bg-editor active:bg-accent-active transition-colors shrink-0"
        title={t(lang, 'flyToNode')}
        aria-label={t(lang, 'flyToNode')}
      >
        <MapPin size={14} />
      </button>
    </div>
  );
});
