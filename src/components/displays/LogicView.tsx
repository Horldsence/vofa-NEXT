import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { LogicTimingChart } from './LogicTimingChart';
import { DecodedEventList } from './DecodedEventList';
import { PanelTabs } from '../ui/PanelTabs';
import { Activity, List } from 'lucide-react';

type ViewMode = 'timing' | 'events';

/// 逻辑分析仪综合视图 — 时序图 + 解码事件列表 两种视图模式切换
export function LogicView() {
  const lang = useAppStore((s) => s.lang);
  const [mode, setMode] = useState<ViewMode>('timing');

  const tabs = [
    { value: 'timing' as const, label: t(lang, 'timingDiagram'), icon: <Activity /> },
    { value: 'events' as const, label: t(lang, 'decodedEvents'), icon: <List /> },
  ];

  return (
    <div className="h-full w-full flex flex-col overflow-hidden bg-bg-editor">
      <PanelTabs tabs={tabs} active={mode} onChange={setMode} />
      <div className="flex-1 overflow-hidden min-h-0">
        {mode === 'timing' && <LogicTimingChart />}
        {mode === 'events' && <DecodedEventList />}
      </div>
    </div>
  );
}
