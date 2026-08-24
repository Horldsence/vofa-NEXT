import { memo, useState } from 'react';
import { ChevronDown, ChevronRight, AlertTriangle } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { CompileErrorItem } from './CompileErrorItem';
import { t } from '../../../i18n';
import type { CompileReport } from '../../../store/slices/compileError';

interface CompileErrorGroupProps {
  tabId: string;
  report: CompileReport;
}

export const CompileErrorGroup = memo(function CompileErrorGroup({
  tabId,
  report,
}: CompileErrorGroupProps) {
  const lang = useAppStore((s) => s.lang);
  const controlTab = useAppStore((s) => s.controlTabs.find((t) => t.id === tabId));

  const [isOpen, setIsOpen] = useState(true);

  const tabName = controlTab?.name ?? tabId;
  const nodeCount = report.nodes?.length ?? 0;

  return (
    <div className="border border-border rounded-md overflow-hidden bg-bg-panel-header/30">
      {/* Group Header */}
      <div
        onClick={() => setIsOpen((prev) => !prev)}
        className="flex items-center justify-between px-3 py-2 cursor-pointer bg-bg-panel-header hover:bg-bg-hover transition-colors select-none"
      >
        <div className="flex items-center gap-2 min-w-0">
          {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <AlertTriangle size={14} className="text-red-500 shrink-0" />
          <span className="text-sm font-semibold truncate text-text-primary">
            {tabName}
          </span>
          <span className="text-xs text-text-secondary font-mono">
            {`(${t(lang, 'compileErrorNodes').replace('{{count}}', String(nodeCount))})`}
          </span>
        </div>
      </div>

      {/* Group Content */}
      {isOpen && (
        <div className="p-1.5 flex flex-col gap-1 bg-bg-editor/50 border-t border-border-subtle">
          {report.nodes && report.nodes.length > 0 ? (
            report.nodes.map((nodeId) => (
              <CompileErrorItem
                key={nodeId}
                tabId={tabId}
                nodeId={nodeId}
                error={report.error}
              />
            ))
          ) : (
            <div className="text-center py-4 text-xs text-text-secondary italic">
              {t(lang, 'noCompileErrors')}
            </div>
          )}
        </div>
      )}
    </div>
  );
});
