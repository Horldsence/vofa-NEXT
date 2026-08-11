import { useCallback, useState, useMemo } from 'react';

/// 通用列表选择状态管理
/// 支持: 单选 / 多选(Ctrl) / 范围选(Shift) / 全选 / 清空
export function useSelection(count: number) {
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [lastSelected, setLastSelected] = useState<number | null>(null);

  const clear = useCallback(() => {
    setSelected(new Set());
    setLastSelected(null);
  }, []);

  const selectOne = useCallback((index: number) => {
    setSelected(new Set([index]));
    setLastSelected(index);
  }, []);

  const toggle = useCallback((index: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
    setLastSelected(index);
  }, []);

  const selectRange = useCallback((start: number, end: number) => {
    const from = Math.max(0, Math.min(start, end));
    const to = Math.min(count - 1, Math.max(start, end));
    const next = new Set<number>();
    for (let i = from; i <= to; i++) next.add(i);
    setSelected(next);
  }, [count]);

  const selectAll = useCallback(() => {
    const next = new Set<number>();
    for (let i = 0; i < count; i++) next.add(i);
    setSelected(next);
  }, [count]);

  const handleClick = useCallback((index: number, event: React.MouseEvent) => {
    if (event.shiftKey && lastSelected !== null) {
      selectRange(lastSelected, index);
    } else if (event.ctrlKey || event.metaKey) {
      toggle(index);
    } else {
      selectOne(index);
    }
  }, [lastSelected, selectOne, selectRange, toggle]);

  const isSelected = useCallback((index: number) => selected.has(index), [selected]);

  const selectedSorted = useMemo(() => Array.from(selected).sort((a, b) => a - b), [selected]);

  return {
    selected,
    selectedSorted,
    lastSelected,
    isSelected,
    handleClick,
    selectOne,
    selectRange,
    selectAll,
    toggle,
    clear,
  };
}
