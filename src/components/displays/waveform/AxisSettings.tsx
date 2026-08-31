import { useState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import {
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type ChannelAxisConfig,
} from '../../../types';
import { AllTabContent } from '../widgets/AllTabContent';
import { ChannelTabContent } from '../widgets/ChannelTabContent';
import { AnimatedSwitch } from '../../ui/AnimatedSwitch';
import { useSlidingPill, SlidingPill } from '../../ui/SlidingPill';
import { CHANNEL_TAB_COLORS, type RenderStepSelect } from '../common/scopeShared';

interface ScopePanelProps {
  config: ScopeAxisConfig;
  onChange: (next: ScopeAxisConfig) => void;
  channelCount: number;
  measurements?: ScopeMeasurements | null;
  onAutoSet?: () => void;
}

type TabId = 'all' | `ch${number}`;

/// 示波器风格设置面板 — 左侧竖排通道 Tab + 右侧内容
/// - "全部" Tab: 全局控件 (Run/Stop/AutoSet/Grid/水平/所有通道/游标/测量)
/// - "CHn" Tab: 仅该通道的 V/div/Position/耦合/Show
export function AxisSettings({
  config,
  onChange,
  channelCount,
  measurements,
  onAutoSet,
}: ScopePanelProps) {
  const lang = useAppStore((s) => s.lang);
  const [activeTab, setActiveTab] = useState<TabId>('all');

  // Tab 滑动指示器 (竖向)
  const { containerRef: tabColRef, pill: tabPill } = useSlidingPill(activeTab);

  // channels 数组与 channelCount 对齐
  const channels: ChannelAxisConfig[] = Array.from({ length: channelCount }, (_, i) =>
    config.channels[i] ?? { vPerDiv: 1, position: 0, show: true, coupling: 'DC' }
  );

  const patch = (p: Partial<ScopeAxisConfig>) => onChange({ ...config, ...p });
  const patchChannel = (idx: number, p: Partial<ChannelAxisConfig>) => {
    const next = channels.slice();
    next[idx] = { ...next[idx], ...p };
    onChange({ ...config, channels: next });
  };

  // 通用档位下拉
  const renderStepSelect: RenderStepSelect = (steps, value, onPick, format) => (
    <select
      className="form-select flex-1 min-w-0 text-xs"
      value={value}
      onChange={(e) => onPick(parseFloat(e.target.value))}
    >
      {steps.map((v) => (
        <option key={v} value={v}>{format(v)}</option>
      ))}
    </select>
  );

  return (
    <div className="flex h-full w-full overflow-hidden">
      {/* 左侧竖排通道 Tab — 圆角分段风格 + 滑动指示器 */}
      <div ref={tabColRef} className="relative flex-none w-12 flex flex-col gap-1 p-1.5 bg-bg-panel-header border-r border-border overflow-y-auto">
        <SlidingPill pill={tabPill} variant="panel" />
        <button
          data-tab-key="all"
          className={`relative flex flex-col items-center justify-center gap-0.5 px-0.5 py-1.5 rounded-md bg-transparent border-none text-[10px] font-mono cursor-pointer transition-colors duration-150 ${activeTab === 'all' ? 'text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'}`}
          onClick={() => setActiveTab('all')}
          title={t(lang, 'channels')}
        >
          {t(lang, 'channels')}
        </button>
        {channels.map((ch, idx) => (
          <button
            key={idx}
            data-tab-key={`ch${idx}`}
            className={`relative flex flex-col items-center justify-center gap-0.5 px-0.5 py-1.5 rounded-md bg-transparent border-none text-[10px] font-mono cursor-pointer transition-colors duration-150 ${activeTab === `ch${idx}` ? 'text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'} ${!ch.show ? 'opacity-50' : ''}`}
            onClick={() => setActiveTab(`ch${idx}` as TabId)}
            title={`CH${idx}`}
          >
            <span
              className="w-2 h-2 rounded-full inline-block"
              style={{ background: CHANNEL_TAB_COLORS[idx % CHANNEL_TAB_COLORS.length] }}
            />
            <span>CH{idx}</span>
          </button>
        ))}
      </div>

      {/* 右侧内容 */}
      <div className="flex-1 min-w-0 overflow-y-auto overflow-x-hidden">
        <AnimatedSwitch
          switchKey={activeTab}
          order={['all', ...channels.map((_, i) => `ch${i}`)]}
          axis="y"
        >
          {activeTab === 'all' ? (
            <AllTabContent
              config={config}
              channels={channels}
              measurements={measurements}
              onAutoSet={onAutoSet}
              lang={lang}
              patch={patch}
              patchChannel={patchChannel}
              renderStepSelect={renderStepSelect}
            />
          ) : (
            <ChannelTabContent
              idx={Number(activeTab.replace('ch', ''))}
              ch={channels[Number(activeTab.replace('ch', ''))]}
              yUnit={config.yUnit}
              sharedY={config.sharedY}
              onPatchChannel={patchChannel}
              renderStepSelect={renderStepSelect}
              lang={lang}
            />
          )}
        </AnimatedSwitch>
      </div>
    </div>
  );
}
