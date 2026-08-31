import { memo, useState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { CompileErrorGroup } from './CompileErrorGroup';
import { AlertTriangle, RotateCw } from 'lucide-react';
import { t } from '../../../i18n';

export const CompileErrorsView = memo(function CompileErrorsView() {
  const lang = useAppStore((s) => s.lang);
  const errorTabs = useAppStore((s) => s.errorTabs);
  const tabErrors = useAppStore((s) => s.tabErrors);
  const tabStates = useAppStore((s) => s.tabStates);

  const [isRefreshing, setIsRefreshing] = useState(false);

  // 只展示当前处于 error 状态的 tab 错误
  const activeErrors = errorTabs.filter((id) => tabStates[id] === 'error' && tabErrors[id]);

  const handleRefresh = () => {
    setIsRefreshing(true);
    try {
      void useAppStore.getState().syncAllTabGraphs();
    } finally {
      // Small timeout to give visual feedback to the click
      setTimeout(() => {
        setIsRefreshing(false);
      }, 300);
    }
  };

  return (
    <div className="h-full w-full flex flex-col bg-bg-editor overflow-hidden" data-tour="errors-view">
      {/* Header */}
      <div className="shrink-0 px-4 py-2 border-b border-border flex items-center justify-between bg-bg-panel-header/50 select-none">
        <div className="flex items-center gap-2">
          <AlertTriangle size={14} className="text-yellow-500" />
          <span className="text-sm font-semibold text-text-primary">
            {t(lang, 'compileErrorsTitle')}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-text-secondary font-mono">
            {t(lang, 'compileErrorTabs').replace('{{count}}', String(activeErrors.length))}
          </span>
          <button
            type="button"
            onClick={handleRefresh}
            disabled={isRefreshing}
            className="p-1 text-text-secondary hover:text-text-primary rounded hover:bg-bg-hover active:bg-accent-active transition-colors flex items-center justify-center cursor-pointer disabled:opacity-50 disabled:pointer-events-none"
            title={t(lang, 'refresh')}
            aria-label={t(lang, 'refresh')}
          >
            <RotateCw size={13} className={isRefreshing ? 'animate-spin' : ''} />
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {activeErrors.length === 0 ? (
          <div className="h-full w-full flex flex-col items-center justify-center p-6 select-none text-text-secondary">
            <AlertTriangle size={32} className="text-border mb-3" />
            <span className="text-sm font-medium">{t(lang, 'noCompileErrors')}</span>
          </div>
        ) : (
          <div className="p-4 flex flex-col gap-3">
            {activeErrors.map((tabId) => (
              <CompileErrorGroup
                key={tabId}
                tabId={tabId}
                report={tabErrors[tabId]}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
});
