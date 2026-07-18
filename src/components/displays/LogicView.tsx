import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { LogicTimingChart } from './LogicTimingChart';
import { DecodedEventList } from './DecodedEventList';
import { Activity, List } from 'lucide-react';

type ViewMode = 'timing' | 'events';

/// 逻辑分析仪综合视图 — 时序图 + 解码事件列表 两种视图模式切换
export function LogicView() {
  const lang = useAppStore((s) => s.lang);
  const [mode, setMode] = useState<ViewMode>('timing');

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex gap-1 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <button
          className={`px-2 py-1 text-xs rounded cursor-pointer flex items-center gap-1 transition-colors ${mode === 'timing' ? 'bg-accent text-text-bright' : 'text-text-secondary hover:bg-bg-hover'}`}
          onClick={() => setMode('timing')}
        >
          <Activity size={12} />
          {t(lang, 'timingDiagram')}
        </button>
        <button
          className={`px-2 py-1 text-xs rounded cursor-pointer flex items-center gap-1 transition-colors ${mode === 'events' ? 'bg-accent text-text-bright' : 'text-text-secondary hover:bg-bg-hover'}`}
          onClick={() => setMode('events')}
        >
          <List size={12} />
          {t(lang, 'decodedEvents')}
        </button>
      </div>
      <div className="flex-1 overflow-hidden min-h-0">
        {mode === 'timing' && <LogicTimingChart />}
        {mode === 'events' && <DecodedEventList />}
      </div>
    </div>
  );
}
