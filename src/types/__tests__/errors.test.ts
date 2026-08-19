import { describe, expect, test } from 'vitest';
import { NodeErrorKind, parseNodeError } from '../errors';

describe('parseNodeError', () => {
  test('解析后端带标签枚举错误 { kind, message }', () => {
    const err = parseNodeError({ kind: 'PortNotFound', message: '端口未找到: /dev/ttyUSB0' });
    expect(err.kind).toBe(NodeErrorKind.PortNotFound);
    expect(err.message).toBe('端口未找到: /dev/ttyUSB0');
  });

  test('未知 kind 字符串归为 Unknown 并保留 message', () => {
    const err = parseNodeError({ kind: 'WhateverNew', message: 'something' });
    expect(err.kind).toBe(NodeErrorKind.Unknown);
    expect(err.message).toBe('something');
  });

  test('兼容旧版纯字符串错误 → Unknown', () => {
    const err = parseNodeError('端口未找到: /dev/ttyUSB0');
    expect(err.kind).toBe(NodeErrorKind.Unknown);
    expect(err.message).toBe('端口未找到: /dev/ttyUSB0');
  });

  test('Error 实例 → Unknown, 取 message', () => {
    const err = parseNodeError(new Error('boom'));
    expect(err.kind).toBe(NodeErrorKind.Unknown);
    expect(err.message).toBe('boom');
  });

  test('无法识别的值 → Unknown, 不抛异常', () => {
    const err = parseNodeError({ foo: 1 });
    expect(err.kind).toBe(NodeErrorKind.Unknown);
    expect(typeof err.message).toBe('string');
    expect(parseNodeError(undefined).kind).toBe(NodeErrorKind.Unknown);
  });
});
