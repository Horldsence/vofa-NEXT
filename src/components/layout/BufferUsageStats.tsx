import { useEffect, useState } from 'react';
import { canFrameBuffer } from '../../lib/canBuffer';
import { logicSampleBuffer, decodedEventBuffer } from '../../lib/logicBuffer';

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

/// 单个缓存使用量指示器 — 标签 + 进度条 + 数字
function BufferIndicator({
  label,
  stats,
}: {
  label: string;
  stats: BufferStats;
}) {
  const pct = Math.min(100, Math.max(0, stats.usage * 100));
  return (
    <div className="flex items-center gap-1.5" title={`${label}: ${stats.length}/${stats.capacity}`}>
      <span className="text-text-secondary text-[11px]">{label}</span>
      <div className="w-12 h-2 bg-bg-input rounded-sm overflow-hidden flex-shrink-0">
        <div
          className={`h-full ${usageColor(stats.usage)} transition-[width] duration-150`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className={`text-[10px] font-mono ${usageTextColor(stats.usage)} tabular-nums`}>
        {formatCount(stats.length)}/{formatCount(stats.capacity)}
      </span>
    </div>
  );
}

/// 状态栏缓存使用量组件 — 订阅三个 buffer 的 stats, RAF 节流后更新
export function BufferUsageStats() {
  const [canStats, setCanStats] = useState<BufferStats>(empty);
  const [logicStats, setLogicStats] = useState<BufferStats>(empty);
  const [decodedStats, setDecodedStats] = useState<BufferStats>(empty);

  useEffect(() => {
    const unsubCan = canFrameBuffer.subscribeStats((usage, length, capacity) =>
      setCanStats({ usage, length, capacity })
    );
    const unsubLogic = logicSampleBuffer.subscribeStats((usage, length, capacity) =>
      setLogicStats({ usage, length, capacity })
    );
    const unsubDecoded = decodedEventBuffer.subscribeStats((usage, length, capacity) =>
      setDecodedStats({ usage, length, capacity })
    );
    return () => {
      unsubCan();
      unsubLogic();
      unsubDecoded();
    };
  }, []);

  return (
    <div className="flex items-center gap-3">
      <BufferIndicator label="CAN" stats={canStats} />
      <BufferIndicator label="Logic" stats={logicStats} />
      <BufferIndicator label="Decoded" stats={decodedStats} />
    </div>
  );
}
