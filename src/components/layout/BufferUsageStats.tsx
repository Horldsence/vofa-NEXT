import { useEffect, useState } from 'react';
import { canFrameBuffer } from '../../lib/buffers/canBuffer';
import { waveformWindow } from '../../lib/buffers/dataBuffer';
import { subscribeRawDataPreviewStats } from '../../lib/buffers/rawDataPreviewRegistry';
import { logicSampleBuffer, decodedEventBuffer } from '../../lib/buffers/logicBuffer';

interface BufferStats {
  usage: number; // 0-1
  length: number;
  capacity: number;
}

const empty: BufferStats = { usage: 0, length: 0, capacity: 1 };

/// 缓存使用量颜色 (VSCode 风格: 绿→黄→红)
function usageColor(usage: number): string {
  if (usage < 0.6) return 'bg-green';
  if (usage < 0.85) return 'bg-yellow';
  return 'bg-red';
}

/// 与 usageColor 同语义的文字色 (compact 百分比模式用)
function usageTextColor(usage: number): string {
  if (usage < 0.6) return 'text-green';
  if (usage < 0.85) return 'text-yellow';
  return 'text-red';
}

/// 格式化容量数字 (k/M)
function formatCount(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1000000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1000000).toFixed(2)}M`;
}

/// 单个缓存使用量指示器 — 标签 + 进度条 + 数字; compact 模式仅显示纯文字百分比
function BufferIndicator({
  label,
  stats,
  compact = false,
}: {
  label: string;
  stats: BufferStats;
  compact?: boolean;
}) {
  const pct = Math.min(100, Math.max(0, stats.usage * 100));
  if (compact) {
    return (
      <span
        className={`text-[10px] font-mono tabular-nums whitespace-nowrap ${usageTextColor(stats.usage)}`}
        title={`${label}: ${stats.length}/${stats.capacity}`}
      >
        {label} {Math.round(pct)}%
      </span>
    );
  }
  return (
    <div className="flex items-center gap-1.5" title={`${label}: ${stats.length}/${stats.capacity}`}>
      <span className="text-[11px] opacity-80">{label}</span>
      <div className="w-12 h-2 bg-text-inverse/20 rounded-sm overflow-hidden flex-shrink-0">
        <div
          className={`h-full ${usageColor(stats.usage)} transition-[width] duration-150`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="text-[10px] font-mono tabular-nums">
        {formatCount(stats.length)}/{formatCount(stats.capacity)}
      </span>
    </div>
  );
}

/// 状态栏缓存使用量组件 — 订阅三个 buffer 的 stats, RAF 节流后更新
/// compact: 状态栏收缩档, 仅显示纯文字百分比 (Wave 12%)
export function BufferUsageStats({ compact = false }: { compact?: boolean }) {
  const [canStats, setCanStats] = useState<BufferStats>(empty);
  const [rawStats, setRawStats] = useState<BufferStats>(empty);
  const [logicStats, setLogicStats] = useState<BufferStats>(empty);
  const [decodedStats, setDecodedStats] = useState<BufferStats>(empty);
  const [waveformStats, setWaveformStats] = useState<BufferStats>(empty);

  useEffect(() => {
    const unsubCan = canFrameBuffer.subscribeStats((usage, length, capacity) =>
      setCanStats({ usage, length, capacity })
    );
    const unsubRaw = subscribeRawDataPreviewStats((usage, length, capacity) =>
      setRawStats({ usage, length, capacity })
    );
    const unsubLogic = logicSampleBuffer.subscribeStats((usage, length, capacity) =>
      setLogicStats({ usage, length, capacity })
    );
    const unsubDecoded = decodedEventBuffer.subscribeStats((usage, length, capacity) =>
      setDecodedStats({ usage, length, capacity })
    );
    const unsubWaveform = waveformWindow.subscribeStats((usage, length, capacity) =>
      setWaveformStats({ usage, length, capacity })
    );
    return () => {
      unsubCan();
      unsubRaw();
      unsubLogic();
      unsubDecoded();
      unsubWaveform();
    };
  }, []);

  return (
    <div className="flex items-center gap-3">
      <BufferIndicator label="Wave" stats={waveformStats} compact={compact} />
      <BufferIndicator label="Raw" stats={rawStats} compact={compact} />
      <BufferIndicator label="CAN" stats={canStats} compact={compact} />
      <BufferIndicator label="Logic" stats={logicStats} compact={compact} />
      <BufferIndicator label="Decoded" stats={decodedStats} compact={compact} />
    </div>
  );
}
