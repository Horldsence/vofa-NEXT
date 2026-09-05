import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useRawStringSamples } from '../useRawDataBuffer';
import { useAppStore } from '../../../../store/appStore';

/// 字符串平面是 latest-value 快照 (全局单订阅, 值变化才推送);
/// hook 按端口值变化累积历史行。测试直接以 setState 模拟快照推送。
function pushText(outputs: Record<string, Record<string, string>>) {
  act(() => {
    useAppStore.setState({ customTextOutputs: outputs });
  });
}

describe('useRawStringSamples - 字符串通道历史', () => {
  it('值变化累积历史行, 相同值不重复记录', () => {
    const { result } = renderHook(() =>
      useRawStringSamples({ sourceId: 'w-trigger', sourceHandle: 'text' })
    );
    expect(result.current.rows).toEqual([]);

    pushText({ 'w-trigger': { text: 'hello' } });
    expect(result.current.rows).toHaveLength(1);
    expect(result.current.rows[0].text).toBe('hello');
    expect(result.current.rows[0].ts).toBeGreaterThan(0);

    // latest-value 平面: 相同值再次推送 → 不重复入列
    pushText({ 'w-trigger': { text: 'hello' } });
    expect(result.current.rows).toHaveLength(1);

    pushText({ 'w-trigger': { text: 'world' } });
    expect(result.current.rows).toHaveLength(2);
    expect(result.current.rows.map((r) => r.text)).toEqual(['hello', 'world']);
  });

  it('未订阅端口 (sourceId 为空) 时不累积', () => {
    const { result } = renderHook(() => useRawStringSamples(undefined));
    pushText({ 'w-trigger': { text: 'hello' } });
    expect(result.current.rows).toEqual([]);
  });

  it('通道切换 → 历史清空, 新端口从当前值重建', () => {
    const { result, rerender } = renderHook(
      ({ sourceId, sourceHandle }) => useRawStringSamples({ sourceId, sourceHandle }),
      { initialProps: { sourceId: 'w-trigger', sourceHandle: 'text' } }
    );
    pushText({ 'w-trigger': { text: 'hello' }, 'w-str': { result: 'str-out' } });
    expect(result.current.rows.map((r) => r.text)).toEqual(['hello']);

    rerender({ sourceId: 'w-str', sourceHandle: 'result' });
    // 通道切换: 历史清空, 新端口当前值立即入列 (latest-value 平面的现状即历史起点)
    expect(result.current.rows.map((r) => r.text)).toEqual(['str-out']);

    pushText({ 'w-trigger': { text: 'hello' }, 'w-str': { result: 'str-next' } });
    expect(result.current.rows.map((r) => r.text)).toEqual(['str-out', 'str-next']);
  });

  it('clear 清空历史; 下次快照推送时当前值重新入列 (不永久错过)', () => {
    const { result } = renderHook(() =>
      useRawStringSamples({ sourceId: 'w-trigger', sourceHandle: 'text' })
    );
    pushText({ 'w-trigger': { text: 'hello' } });
    pushText({ 'w-trigger': { text: 'world' } });
    expect(result.current.rows).toHaveLength(2);

    act(() => {
      result.current.clear();
    });
    expect(result.current.rows).toEqual([]);

    // 当前值仍是 'world' — 任意字符串输出变化推送后重新入列
    pushText({ 'w-trigger': { text: 'world' }, 'w-other': { text: 'x' } });
    expect(result.current.rows.map((r) => r.text)).toEqual(['world']);
  });

  it('超过上限 (1000 行) 淘汰最旧行', () => {
    const { result } = renderHook(() =>
      useRawStringSamples({ sourceId: 'w-trigger', sourceHandle: 'text' })
    );
    for (let i = 0; i < 1005; i++) {
      pushText({ 'w-trigger': { text: `v${i}` } });
    }
    expect(result.current.rows).toHaveLength(1000);
    expect(result.current.rows[0].text).toBe('v5');
    expect(result.current.rows[999].text).toBe('v1004');
  });
});
