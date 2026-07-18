/// 命令输入解析 — 4 种格式转换为 Uint8Array
///
/// 1. HEX: "AA 01 02 BB" → [0xAA, 0x01, 0x02, 0xBB]
/// 2. ASCII + 转义: "Hello\n\xAA" → [..., 0x0A, 0xAA]
/// 3. 模板字符串: "SET ${CH0} ${VALUE}\n" → 用 variables 替换后再按 ASCII 解析
/// 4. 结构化字段: [{type, value}, ...] → 按字节序打包

import type { CommandField, FieldType } from '../types';

/// 解析 HEX 字符串为字节数组
/// 支持 "AA 01 02 BB" / "AA0102BB" / "AA, 01, 02" 等格式
/// 长度必须为偶数 (每个字节 2 个十六进制字符)
export function parseHex(input: string): Uint8Array {
  // 移除所有非 hex 字符 (空格/逗号/0x 前缀等)
  const clean = input.replace(/0x/gi, '').replace(/[^0-9a-fA-F]/g, '');
  if (clean.length === 0) return new Uint8Array(0);
  if (clean.length % 2 !== 0) {
    throw new Error('HEX 长度必须为偶数 (每字节 2 个十六进制字符)');
  }
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < clean.length; i += 2) {
    const byte = parseInt(clean.slice(i, i + 2), 16);
    if (Number.isNaN(byte)) throw new Error(`无效的 HEX 字节: ${clean.slice(i, i + 2)}`);
    bytes[i / 2] = byte;
  }
  return bytes;
}

/// 解析 ASCII 文本 + 转义字符
/// 支持的转义: \n \r \t \\ \xHH (HEX 字节) \0 (null)
export function parseAscii(input: string): Uint8Array {
  const bytes: number[] = [];
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (ch === '\\') {
      const next = input[i + 1];
      if (!next) {
        // 末尾单独的反斜杠, 当作字面量
        bytes.push(0x5c);
        break;
      }
      switch (next) {
        case 'n': bytes.push(0x0a); i++; break;
        case 'r': bytes.push(0x0d); i++; break;
        case 't': bytes.push(0x09); i++; break;
        case '\\': bytes.push(0x5c); i++; break;
        case '0': bytes.push(0x00); i++; break;
        case 'x': {
          const hex = input.slice(i + 2, i + 4);
          if (hex.length === 2 && /^[0-9a-fA-F]{2}$/.test(hex)) {
            bytes.push(parseInt(hex, 16));
            i += 3;
          } else {
            // 无效的 \x 转义, 当作字面量
            bytes.push(0x5c, 0x78);
          }
          break;
        }
        default:
          // 未知转义, 保留反斜杠 + 字符
          bytes.push(0x5c, next.charCodeAt(0));
          i++;
      }
    } else {
      // 非 ASCII 字符 (>127) 用 UTF-8 编码
      const code = ch.charCodeAt(0);
      if (code < 0x80) {
        bytes.push(code);
      } else {
        // UTF-8 多字节编码
        const encoded = new TextEncoder().encode(ch);
        for (const b of encoded) bytes.push(b);
      }
    }
  }
  return new Uint8Array(bytes);
}

/// 模板变量插值 — ${VAR} 替换为 variables[VAR]
///
/// variables 是 string -> string 的映射, 若变量不存在则替换为空字符串
/// 替换后按 ASCII + 转义解析
export function parseTemplate(
  input: string,
  variables: Record<string, string | number>
): Uint8Array {
  const replaced = input.replace(/\$\{(\w+)\}/g, (_match, name) => {
    const v = variables[name];
    return v !== undefined ? String(v) : '';
  });
  return parseAscii(replaced);
}

/// 结构化字段打包 — 按 type 字节序打包为 Uint8Array
///
/// 字段类型:
///   uint8/int8:        1 字节
///   uint16LE/BE 等:    2 字节
///   uint32LE/BE 等:    4 字节
///   float32LE/BE:      4 字节 (IEEE 754)
///   bytes:             value 解析为 HEX 字节流
export function parseStructured(fields: CommandField[]): Uint8Array {
  const chunks: Uint8Array[] = [];
  for (const f of fields) {
    chunks.push(packField(f.type, f.value));
  }
  // 合并所有 chunks
  const total = chunks.reduce((s, c) => s + c.length, 0);
  const result = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    result.set(c, offset);
    offset += c.length;
  }
  return result;
}

/// 单字段打包
function packField(type: FieldType, value: string): Uint8Array {
  switch (type) {
    case 'uint8': {
      const n = parseNumber(value, 0, 0xff);
      return new Uint8Array([n & 0xff]);
    }
    case 'int8': {
      const n = parseNumber(value, -0x80, 0x7f);
      return new Uint8Array([n & 0xff]);
    }
    case 'uint16LE':
    case 'uint16BE': {
      const n = parseNumber(value, 0, 0xffff);
      const buf = new Uint8Array(2);
      const view = new DataView(buf.buffer);
      view.setUint16(0, n & 0xffff, type === 'uint16LE');
      return buf;
    }
    case 'int16LE':
    case 'int16BE': {
      const n = parseNumber(value, -0x8000, 0x7fff);
      const buf = new Uint8Array(2);
      const view = new DataView(buf.buffer);
      view.setInt16(0, n, type === 'int16LE');
      return buf;
    }
    case 'uint32LE':
    case 'uint32BE': {
      const n = parseNumber(value, 0, 0xffffffff);
      const buf = new Uint8Array(4);
      const view = new DataView(buf.buffer);
      view.setUint32(0, n >>> 0, type === 'uint32LE');
      return buf;
    }
    case 'int32LE':
    case 'int32BE': {
      const n = parseNumber(value, -0x80000000, 0x7fffffff);
      const buf = new Uint8Array(4);
      const view = new DataView(buf.buffer);
      view.setInt32(0, n, type === 'int32LE');
      return buf;
    }
    case 'float32LE':
    case 'float32BE': {
      const n = parseFloat(value);
      if (!Number.isFinite(n)) throw new Error(`无效的浮点数: ${value}`);
      const buf = new Uint8Array(4);
      const view = new DataView(buf.buffer);
      view.setFloat32(0, n, type === 'float32LE');
      return buf;
    }
    case 'bytes': {
      // value 作为 HEX 字节流解析
      return parseHex(value);
    }
  }
}

/// 解析数字字符串 (支持十进制/0xHEX/0bBIN)
function parseNumber(value: string, min: number, max: number): number {
  const trimmed = value.trim();
  let n: number;
  if (/^0x[0-9a-fA-F]+$/i.test(trimmed)) {
    n = parseInt(trimmed, 16);
  } else if (/^0b[01]+$/i.test(trimmed)) {
    n = parseInt(trimmed.slice(2), 2);
  } else if (/^-?\d+$/.test(trimmed)) {
    n = parseInt(trimmed, 10);
  } else if (/^-?\d*\.\d+$/.test(trimmed)) {
    n = parseFloat(trimmed);
  } else {
    throw new Error(`无效的数字: ${value}`);
  }
  if (!Number.isFinite(n)) throw new Error(`无效的数字: ${value}`);
  if (n < min || n > max) {
    throw new Error(`数值 ${n} 超出范围 [${min}, ${max}]`);
  }
  return n;
}

/// 字节数组转 HEX 字符串 (用于预览显示)
export function bytesToHex(bytes: Uint8Array, separator: string = ' '): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
    .join(separator);
}

/// 字节数组转 ASCII 字符串 (不可打印字符显示为 .)
export function bytesToAscii(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => (b >= 0x20 && b < 0x7f) ? String.fromCharCode(b) : '.')
    .join('');
}
