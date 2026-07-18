import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import {
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type ChannelAxisConfig,
  type Coupling,
} from '../../types';
import { AllTabContent } from './AllTabContent';
import { ChannelTabContent } from './ChannelTabContent';
import { CHANNEL_TAB_COLORS, type RenderStepSelect } from './scopeShared';

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

  // channels 数组与 channelCount 对齐
  const channels: ChannelAxisConfig[] = Array.from({ length: channelCount }, (_, i) =>
    config.channels[i] ?? { vPerDiv: 1, position: 0, show: true, coupling: 'DC' as Coupling }
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
      className="form-select"
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
      {/* 左侧竖排通道 Tab */}
      <div className="flex-none w-14 flex flex-col gap-[1px] py-1 bg-bg-panel-header border-r border-border overflow-y-auto">
        <button
          className={`flex flex-col items-center justify-center gap-0.5 px-0.5 py-1.5 bg-transparent border-none border-l-2 border-transparent text-[10px] font-mono cursor-pointer transition-all duration-150 hover:bg-bg-hover hover:text-text-primary ${activeTab === 'all' ? 'bg-bg-active text-text-bright border-l-text-bright' : 'text-text-secondary'}`}
          onClick={() => setActiveTab('all')}
          title={t(lang, 'channels')}
        >
          {t(lang, 'channels')}
        </button>
        {channels.map((ch, idx) => (
          <button
            key={idx}
            className={`flex flex-col items-center justify-center gap-0.5 px-0.5 py-1.5 bg-transparent border-none border-l-2 border-transparent text-[10px] font-mono cursor-pointer transition-all duration-150 hover:bg-bg-hover hover:text-text-primary ${activeTab === `ch${idx}` ? 'bg-bg-active text-text-bright border-l-text-bright' : 'text-text-secondary'} ${!ch.show ? 'opacity-50' : ''}`}
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
      </div>
    </div>
  );
}
