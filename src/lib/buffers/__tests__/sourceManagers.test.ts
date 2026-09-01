import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../tauri/tauri';
import {
  acquireWaveformBuffer,
  cleanupSourceManagers,
  releaseWaveformBuffer,
  setPrimaryWaveformSource,
  setWaveformOverviewActive,
} from '../sourceManagers';

describe('sourceManagers 概览订阅', () => {
  let cancellations: ReturnType<typeof vi.fn>[];
  let subscribe: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    cancellations = [];
    subscribe = vi.spyOn(api, 'subscribeWaveform').mockImplementation(() => {
      const cancel = vi.fn();
      cancellations.push(cancel);
      return { cancel };
    });
  });

  afterEach(() => {
    cleanupSourceManagers();
    vi.restoreAllMocks();
  });

  it('概览推送降频为 100ms 间隔', () => {
    acquireWaveformBuffer('src');
    expect(subscribe).toHaveBeenCalledWith('src', expect.any(Function), {
      intervalMs: 100,
      maxPoints: 2000,
    });
  });

  it('停止时暂停推送 (保留缓存与引用计数), 恢复时重订阅', () => {
    acquireWaveformBuffer('src');
    acquireWaveformBuffer('src'); // 引用计数 2
    expect(subscribe).toHaveBeenCalledTimes(1);

    setWaveformOverviewActive('src', false);
    expect(cancellations[0]).toHaveBeenCalledTimes(1);

    // 暂停中释放一个引用不崩溃、不重复退订
    releaseWaveformBuffer('src');
    expect(cancellations[0]).toHaveBeenCalledTimes(1);

    setWaveformOverviewActive('src', true);
    expect(subscribe).toHaveBeenCalledTimes(2);
  });

  it('主波形源同样支持暂停/恢复', () => {
    setPrimaryWaveformSource('primary');
    expect(subscribe).toHaveBeenCalledTimes(1);

    setWaveformOverviewActive('primary', false);
    expect(cancellations[0]).toHaveBeenCalledTimes(1);

    setWaveformOverviewActive('primary', true);
    expect(subscribe).toHaveBeenCalledTimes(2);
  });

  it('暂停中切换主波形源不建立订阅, 恢复后订阅新源', () => {
    setPrimaryWaveformSource('a');
    expect(subscribe).toHaveBeenCalledTimes(1);
    setWaveformOverviewActive('a', false);

    setPrimaryWaveformSource('b');
    expect(subscribe).toHaveBeenCalledTimes(1); // 暂停中不订阅新源

    setWaveformOverviewActive('b', true);
    expect(subscribe).toHaveBeenCalledTimes(2);
    expect(subscribe).toHaveBeenLastCalledWith('b', expect.any(Function), {
      intervalMs: 100,
      maxPoints: 2000,
    });
  });
});
