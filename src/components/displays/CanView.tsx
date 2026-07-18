import { useAppStore } from '../../store/appStore';
import { canFrameBuffer } from '../../lib/canBuffer';
import { useState, useEffect, useMemo } from 'react';
import { t } from '../../i18n';
import { CanFrameList } from './CanFrameList';
import { CanSender } from './CanSender';
import { List, Send, BarChart3 } from 'lucide-react';
import type { CanFrame } from '../../types';

type ViewMode = 'list' | 'send' | 'chart';

/// CAN 综合视图 — 帧列表 / 发送器 / 总线活动 三种视图模式切换
export function CanView() {
  const lang = useAppStore((s) => s.lang);
  const [mode, setMode] = useState<ViewMode>('list');

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex gap-1 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <button
          className={`px-2 py-1 text-xs rounded cursor-pointer flex items-center gap-1 transition-colors ${mode === 'list' ? 'bg-accent text-text-bright' : 'text-text-secondary hover:bg-bg-hover'}`}
          onClick={() => setMode('list')}
        >
          <List size={12} />
          {t(lang, 'frameList')}
        </button>
        <button
          className={`px-2 py-1 text-xs rounded cursor-pointer flex items-center gap-1 transition-colors ${mode === 'send' ? 'bg-accent text-text-bright' : 'text-text-secondary hover:bg-bg-hover'}`}
          onClick={() => setMode('send')}
        >
          <Send size={12} />
          {t(lang, 'canSender')}
        </button>
        <button
          className={`px-2 py-1 text-xs rounded cursor-pointer flex items-center gap-1 transition-colors ${mode === 'chart' ? 'bg-accent text-text-bright' : 'text-text-secondary hover:bg-bg-hover'}`}
          onClick={() => setMode('chart')}
        >
          <BarChart3 size={12} />
          {t(lang, 'busActivity')}
        </button>
      </div>
      <div className="flex-1 overflow-hidden min-h-0">
        {mode === 'list' && <CanFrameList />}
        {mode === 'send' && <CanSender />}
        {mode === 'chart' && <CanBusChart />}
      </div>
    </div>
  );
}

/// CAN 总线活动图 — ID 分布统计 (横向条形图, 取出现次数 Top 20)
function CanBusChart() {
  const lang = useAppStore((s) => s.lang);
  const [frames, setFrames] = useState<CanFrame[]>([]);

  useEffect(() => {
    const unsub = canFrameBuffer.subscribe((recent) => setFrames(recent));
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
    <div className="h-full overflow-y-auto p-3">
      <h3 className="text-sm font-semibold text-text-primary mb-3">{t(lang, 'idDistribution')}</h3>
      {idStats.length === 0 ? (
        <div className="flex items-center justify-center h-32 text-text-secondary text-xs">
          {t(lang, 'noCanFrames')}
        </div>
      ) : (
        <div className="space-y-1">
          {idStats.map((s) => (
            <div key={s.id} className="flex items-center gap-2 text-xs font-mono">
              <span className="text-text-bright w-20 flex-shrink-0">{s.id}{s.extended ? 'X' : ''}</span>
              <div className="flex-1 bg-bg-input rounded h-4 overflow-hidden relative">
                <div
                  className="h-full bg-blue/60 flex items-center"
                  style={{ width: `${(s.count / maxCount) * 100}%` }}
                />
                <div className="absolute inset-0 flex items-center px-1.5 text-[10px] text-text-bright">
                  {s.count} (Rx:{s.rx} Tx:{s.tx})
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
