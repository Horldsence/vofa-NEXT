//! 命令发送控件 (Command) 的帧工具 — 多帧归一化 / 帧增删改 / 端口派生 / 字节拼接

import { nanoid } from 'nanoid';
import type { CommandConfig, CommandFrame, LegacyCommandConfig } from '../../types';
import { computeChecksum, type ChecksumKind } from './checksum';
import { parseHex, packField } from './commandParser';
import { concatChunks } from './commandParser';

/// 归一化命令发送配置: 旧版单帧配置 (blocks 在顶层) 包装为 frames[0]
/// 已是多帧配置时原样返回 (frames 为空时补一个空帧), 幂等
export function normalizeCommandConfig(raw: CommandConfig | LegacyCommandConfig): CommandConfig {
  const anyRaw = raw as Partial<CommandConfig> & Partial<LegacyCommandConfig>;
  if (Array.isArray(anyRaw.frames)) {
    return {
      id: anyRaw.id ?? '',
      label: anyRaw.label ?? '',
      frames: anyRaw.frames.length > 0 ? anyRaw.frames : [makeEmptyFrame(anyRaw.id ?? '')],
      loopbackEnabled: anyRaw.loopbackEnabled ?? false,
      loopbackHistory: anyRaw.loopbackHistory ?? [],
    };
  }
  // 旧格式: blocks/appendNewline/sendMode/timerMs 在顶层 → 单帧
  const frame: CommandFrame = {
    id: `${anyRaw.id ?? 'cmd'}-frame-1`,
    label: anyRaw.label ?? 'Frame 1',
    blocks: anyRaw.blocks ?? [],
    appendNewline: anyRaw.appendNewline ?? false,
    sendMode: anyRaw.sendMode ?? 'manual',
    timerMs: anyRaw.timerMs ?? 100,
  };
  return {
    id: anyRaw.id ?? '',
    label: anyRaw.label ?? '',
    frames: [frame],
    loopbackEnabled: anyRaw.loopbackEnabled ?? false,
    loopbackHistory: anyRaw.loopbackHistory ?? [],
  };
}

/// 取配置的帧列表 (防御: 未归一化的旧配置现场包装, 不落盘)
export function getCommandFrames(params: CommandConfig | LegacyCommandConfig): CommandFrame[] {
  return normalizeCommandConfig(params).frames;
}

/// 收集所有帧的 var_ref 块输入端口名 (并集, 去重, 保序) — 节点输入 Handle 派生
export function commandInputPortNames(params: CommandConfig | LegacyCommandConfig): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const frame of getCommandFrames(params)) {
    for (const b of frame.blocks) {
      if (b.type === 'var_ref' && b.portName && !seen.has(b.portName)) {
        seen.add(b.portName);
        names.push(b.portName);
      }
    }
  }
  return names;
}

/// 新空帧 (默认手动发送)
export function makeEmptyFrame(configId: string, label?: string): CommandFrame {
  return {
    id: `${configId || 'cmd'}-${nanoid(6)}`,
    label: label ?? 'Frame',
    blocks: [],
    appendNewline: false,
    sendMode: 'manual',
    timerMs: 100,
  };
}

/// 追加一帧
export function addCommandFrame(config: CommandConfig, frame: CommandFrame): CommandConfig {
  return { ...config, frames: [...config.frames, frame] };
}

/// 删除一帧 (至少保留一帧)
export function removeCommandFrame(config: CommandConfig, frameId: string): CommandConfig {
  if (config.frames.length <= 1) return config;
  return { ...config, frames: config.frames.filter((f) => f.id !== frameId) };
}

/// 更新指定帧的字段
export function updateCommandFrame(
  config: CommandConfig,
  frameId: string,
  changes: Partial<CommandFrame>
): CommandConfig {
  return {
    ...config,
    frames: config.frames.map((f) => (f.id === frameId ? { ...f, ...changes } : f)),
  };
}

/// 帧字节拼接结果
export interface ComputedFrame {
  bytes: Uint8Array | null;
  error: string | null;
  perBlock: Uint8Array[][];
}

/// 按帧定义拼接字节流 (var_ref 块从 graphInputs 取值, checksum 对前序累计字节计算)
export function computeFrameBytes(
  frame: CommandFrame,
  graphInputs: Record<string, number>
): ComputedFrame {
  try {
    const chunks: Uint8Array[] = [];
    const perBlock: Uint8Array[][] = [];
    for (const block of frame.blocks) {
      let chunk: Uint8Array;
      switch (block.type) {
        case 'const_hex':
          chunk = parseHex(block.hex ?? '');
          break;
        case 'var_ref': {
          const val = graphInputs[block.portName ?? 'value'] ?? 0;
          chunk = packField(block.fieldType ?? 'uint16LE', String(val));
          break;
        }
        case 'typed_const':
          chunk = packField(block.fieldType ?? 'uint8', block.value ?? '0');
          break;
        case 'checksum': {
          const prev = concatChunks(chunks);
          chunk = new Uint8Array(computeChecksum(
            prev,
            (block.checksum ?? 'sum8') as ChecksumKind,
            block.checksum === 'custom' ? block.customScript : undefined
          ));
          break;
        }
      }
      chunks.push(chunk);
      perBlock.push([chunk]);
    }
    let result = concatChunks(chunks);
    if (frame.appendNewline) {
      const withNl = new Uint8Array(result.length + 1);
      withNl.set(result, 0);
      withNl[result.length] = 0x0a;
      result = withNl;
    }
    return { bytes: result, error: null, perBlock };
  } catch (e) {
    return { bytes: null, error: (e as Error).message, perBlock: [] };
  }
}
