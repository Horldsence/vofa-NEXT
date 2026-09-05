// ============ 显示控件自动量程 Hook ============
//
// 把 useNumericInput 的样本历史增量喂进滑动窗口 store (lib/data/autoRange),
// 返回显示控件当前应使用的量程:
// - manual 模式: 直接返回配置量程, 不推送样本;
// - auto 模式: 窗口内有样本 → 取 1-2-5 取整后的自适应量程;
//   无样本 (未连接/刚清空) → 回退配置量程。

import { useEffect, useMemo, useRef, useSyncExternalStore } from 'react';
import { getAutoRangeStore, releaseAutoRangeStore } from '../data/autoRange';
import type { NumericPortState } from '../data/numericTypes';
import type { DisplayRange } from '../utils/valueRange';

export interface AutoRangeConfig {
  mode: 'manual' | 'auto';
  min: number;
  max: number;
  windowSec: number;
  majorTicks: number;
}

export function useAutoRange(
  widgetId: string,
  input: NumericPortState,
  config: AutoRangeConfig,
): DisplayRange {
  const store = useMemo(() => getAutoRangeStore(widgetId), [widgetId]);
  const auto = config.mode === 'auto';

  useEffect(() => {
    store.setWindow(config.windowSec);
    store.setTicks(config.majorTicks);
  }, [store, config.windowSec, config.majorTicks]);

  useEffect(() => () => releaseAutoRangeStore(widgetId), [widgetId, store]);

  // 排水: input.history 快照引用变化 (新样本批次) 时, 把 seq 大于上次水位的
  // 增量行喂给 store; 序列整体回退 (切换数据源) 时全量重喂。
  const lastSeqRef = useRef(-1);
  useEffect(() => {
    if (!auto) {
      lastSeqRef.current = -1;
      return;
    }
    const rows = input.history;
    if (rows.length === 0) {
      lastSeqRef.current = -1;
      return;
    }
    const lastSeq = rows[rows.length - 1].seq;
    const prev = lastSeqRef.current;
    let from = 0;
    if (prev >= 0 && lastSeq > prev) {
      from = rows.length - 1;
      while (from > 0 && rows[from - 1].seq > prev) from--;
    }
    lastSeqRef.current = lastSeq;
    if (prev < 0 || lastSeq !== prev) store.push(rows.slice(from));
  }, [auto, input.history, store]);

  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  if (!auto) return { min: config.min, max: config.max };
  return store.sampleCount() > 0 ? snapshot.range : { min: config.min, max: config.max };
}
