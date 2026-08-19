import { beforeEach, describe, expect, test } from 'vitest';

// localStorage 内存桩 (与其他 store 测试一致, 本环境 jsdom 不提供 localStorage)
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value);
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
  };
})();
(globalThis as { localStorage?: unknown }).localStorage = localStorageMock;

import { NodeErrorKind } from '../../../types/errors';
import { withErrorGuidance, nodeErrorText } from '../errorGuidance';

const PORT_TAKEN = { kind: NodeErrorKind.PortAlreadyOpen, message: '端口已打开: COM3' };

describe('withErrorGuidance', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  test('某种错误第一次出现时追加引导文案', () => {
    const out = withErrorGuidance('zh', PORT_TAKEN, true);
    expect(out).toContain(PORT_TAKEN.message);
    expect(out.length).toBeGreaterThan(PORT_TAKEN.message.length);
    expect(out).not.toBe(PORT_TAKEN.message);
  });

  test('同一错误类型第二次出现不再追加引导', () => {
    withErrorGuidance('zh', PORT_TAKEN, true);
    const second = withErrorGuidance('zh', PORT_TAKEN, true);
    expect(second).toBe(PORT_TAKEN.message);
  });

  test('不同错误类型各自独立引导', () => {
    withErrorGuidance('zh', PORT_TAKEN, true);
    const other = withErrorGuidance(
      'zh',
      { kind: NodeErrorKind.PortNotFound, message: '端口未找到: COM4' },
      true
    );
    expect(other).not.toBe('端口未找到: COM4');
  });

  test('已引导记录从 localStorage 恢复 (跨会话持久)', () => {
    withErrorGuidance('zh', PORT_TAKEN, true);
    // 模拟新会话: 仅依赖 localStorage, 无内存缓存
    const again = withErrorGuidance('zh', PORT_TAKEN, true);
    expect(again).toBe(PORT_TAKEN.message);
  });

  test('showContextualTips 关闭时不追加引导', () => {
    const out = withErrorGuidance('zh', PORT_TAKEN, false);
    expect(out).toBe(PORT_TAKEN.message);
  });

  test('Unknown 类型不追加引导', () => {
    const out = withErrorGuidance('zh', { kind: NodeErrorKind.Unknown, message: 'boom' }, true);
    expect(out).toBe('boom');
  });

  test('英文语言下引导为英文文案', () => {
    const out = withErrorGuidance('en', PORT_TAKEN, true);
    expect(out).toContain(PORT_TAKEN.message);
    expect(out).not.toBe(PORT_TAKEN.message);
  });
});

describe('nodeErrorText', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  test('带标签后端错误: 解析类型并追加首次引导', () => {
    const out = nodeErrorText('zh', { kind: 'PortNotFound', message: '端口未找到: COM1' }, true);
    expect(out).toContain('端口未找到: COM1');
    expect(out).not.toBe('端口未找到: COM1');
  });

  test('纯字符串错误 (Unknown): 原样返回', () => {
    expect(nodeErrorText('zh', 'boom', true)).toBe('boom');
  });
});
