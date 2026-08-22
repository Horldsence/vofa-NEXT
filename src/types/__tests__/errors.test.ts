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

  test('Graph 错误解析为 Graph kind', () => {
    const err = parseNodeError({ kind: 'Graph', message: '数值平面检测到循环连接' });
    expect(err.kind).toBe(NodeErrorKind.Graph);
    expect(err.message).toBe('数值平面检测到循环连接');
  });

  test('Automotive 错误解析为 Automotive kind', () => {
    const err = parseNodeError({ kind: 'Automotive', message: 'ISO-TP 会话已关闭' });
    expect(err.kind).toBe(NodeErrorKind.Automotive);
  });

  test('Plugin 错误解析为 Plugin kind', () => {
    const err = parseNodeError({ kind: 'Plugin', message: '插件错误 [updater]: net' });
    expect(err.kind).toBe(NodeErrorKind.Plugin);
  });
});
