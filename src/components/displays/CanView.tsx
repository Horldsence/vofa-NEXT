import { useAppStore } from '../../store/appStore';
import { canFrameBuffer } from '../../lib/canBuffer';
import { useState, useEffect, useMemo } from 'react';
import { t } from '../../i18n';
import { CanFrameList } from './CanFrameList';
import { CanSender } from './CanSender';
import { CanLoadView } from './CanLoadView';
import { PanelTabs } from '../ui/PanelTabs';
import { ContextualHint } from '../onboarding/ContextualHint';
import { List, Send, BarChart3, Gauge } from 'lucide-react';
import type { CanFrame } from '../../types';

type ViewMode = 'list' | 'send' | 'chart' | 'load';

/// CAN 综合视图 — 帧列表 / 发送器 / 总线活动 / 负载分析 四种视图模式切换
export function CanView() {
  const lang = useAppStore((s) => s.lang);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const setSidebarView = useAppStore((s) => s.setSidebarView);
  const [mode, setMode] = useState<ViewMode>('list');

  const tabs = [
    { value: 'list' as const, label: t(lang, 'frameList'), icon: <List /> },
    { value: 'send' as const, label: t(lang, 'canSender'), icon: <Send /> },
    { value: 'chart' as const, label: t(lang, 'busActivity'), icon: <BarChart3 /> },
    { value: 'load' as const, label: t(lang, 'canLoadAnalysis'), icon: <Gauge /> },
  ];

  const isCanProtocol = protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight';

  return (
    <div className="h-full w-full flex flex-col overflow-hidden bg-bg-editor">
      {!isCanProtocol && (
        <ContextualHint
          id="can-protocol-mismatch"
          message={t(lang, 'canHintMessage')}
          action={{
            label: t(lang, 'canHintAction'),
            onClick: () => setSidebarView('protocol'),
          }}
        />
      )}
      <PanelTabs tabs={tabs} active={mode} onChange={setMode} />
      <div className="flex-1 overflow-hidden min-h-0">
        {mode === 'list' && <CanFrameList />}
        {mode === 'send' && <CanSender />}
        {mode === 'chart' && <CanBusChart />}
        {mode === 'load' && <CanLoadView />}
      </div>
    </div>
  );
}

/// CAN 总线活动图 — ID 分布统计 (横向条形图, 取出现次数 Top 20)
function CanBusChart() {
  const lang = useAppStore((s) => s.lang);
  const [frames, setFrames] = useState<CanFrame[]>([]);

  useEffect(() => {
    const unsub = canFrameBuffer.subscribe(() => setFrames(canFrameBuffer.getRecent(500)));
    setFrames(canFrameBuffer.getRecent(500));
    return unsub;
  }, []);

  // 统计 ID 分布
  const idStats = useMemo(() => {
    const map = new Map<string, { count: number; rx: number; tx: number; extended: boolean }>();
    for (const f of frames) {
      const key = f.extended
        ? f.id.toString(16).toUpperCase().padStart(8, '0')
        : f.id.toString(16).toUpperCase().padStart(3, '0');
      const entry = map.get(key) ?? { count: 0, rx: 0, tx: 0, extended: f.extended };
      entry.count++;
      if (f.direction === 'Rx') entry.rx++;
      else entry.tx++;
      map.set(key, entry);
    }
    return Array.from(map.entries())
      .map(([id, stats]) => ({ id, ...stats }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 20);
  }, [frames]);

  const maxCount = Math.max(...idStats.map((s) => s.count), 1);

  return (
    <div className="h-full overflow-y-auto p-4">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-semibold text-text-primary">{t(lang, 'idDistribution')}</h3>
        <span className="text-xs text-text-secondary font-mono">{canFrameBuffer.length} frames</span>
      </div>
      {idStats.length === 0 ? (
        <div className="flex items-center justify-center h-48 text-text-secondary text-xs rounded border border-dashed border-border bg-bg-panel-header/50">
          {t(lang, 'noCanFrames')}
        </div>
      ) : (
        <div className="space-y-1.5">
          {idStats.map((s) => {
            const rxPct = (s.rx / maxCount) * 100;
            const txPct = (s.tx / maxCount) * 100;
            return (
              <div key={s.id} className="grid grid-cols-[4rem_1fr] sm:grid-cols-[5rem_1fr] items-center gap-3 text-xs font-mono">
                <span className="text-text-bright truncate">
                  0x{s.id}
                  {s.extended && <span className="ml-1 text-accent text-[10px]">X</span>}
                </span>
                <div className="flex items-center gap-2 min-w-0">
                  <div className="flex-1 bg-bg-input rounded h-5 overflow-hidden relative min-w-0">
                    <div className="absolute inset-y-0 left-0 flex">
                      <div
                        className="h-full bg-blue/70"
                        style={{ width: `${rxPct}px` }}
                      />
                      <div
                        className="h-full bg-purple/70"
                        style={{ width: `${txPct}px` }}
                      />
                    </div>
                    <div className="absolute inset-0 flex items-center px-1.5 text-[10px] text-text-bright gap-2">
                      <span>{s.count}</span>
                      <span className="text-blue hidden sm:inline">Rx:{s.rx}</span>
                      <span className="text-purple hidden sm:inline">Tx:{s.tx}</span>
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
