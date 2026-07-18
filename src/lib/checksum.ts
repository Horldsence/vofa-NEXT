/// 校验和计算库 — 各类嵌入式通信常用校验算法
///
/// 所有函数输入为 Uint8Array, 输出为 number[] (1~4 字节, 按高位在前/低位在前视算法而定)
///
/// 算法参考:
/// - CRC: https://reveng.sourceforge.io/crc-catalogue/
/// - LRC: Modbus Over Serial Line — LRC 计算

/// CRC-8 (poly 0x07, init 0x00, reflIn/reflOut=false, xorOut=0x00)
/// 用于 SMBus / 一般 8-bit 校验
export function crc8(data: Uint8Array): number[] {
  let crc = 0x00;
  for (const b of data) {
    crc ^= b;
    for (let i = 0; i < 8; i++) {
      crc = (crc & 0x80) ? ((crc << 1) ^ 0x07) & 0xff : (crc << 1) & 0xff;
    }
  }
  return [crc];
}

/// CRC-16 Modbus (poly 0xA001, init 0xFFFF, reflIn/reflOut=true, xorOut=0x0000)
/// 用于 Modbus RTU
export function crc16Modbus(data: Uint8Array): number[] {
  let crc = 0xffff;
  for (const b of data) {
    crc ^= b;
    for (let i = 0; i < 8; i++) {
      crc = (crc & 0x0001) ? ((crc >> 1) ^ 0xa001) & 0xffff : (crc >> 1) & 0xffff;
    }
  }
  // Modbus 低字节在前
  return [crc & 0xff, (crc >> 8) & 0xff];
}

/// CRC-16 CCITT-FALSE (poly 0x1021, init 0xFFFF, reflIn/reflOut=false, xorOut=0x0000)
/// 用于 XMODEM / 一般通信帧
export function crc16CCITT(data: Uint8Array): number[] {
  let crc = 0xffff;
  for (const b of data) {
    crc ^= (b << 8);
    for (let i = 0; i < 8; i++) {
      crc = (crc & 0x8000) ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  // 高字节在前 (big-endian)
  return [(crc >> 8) & 0xff, crc & 0xff];
}

/// CRC-32 (ZIP poly 0xEDB88320 反射, init 0xFFFFFFFF, xorOut=0xFFFFFFFF)
/// 用于 ZIP / Ethernet
export function crc32(data: Uint8Array): number[] {
  let crc = 0xffffffff;
  for (const b of data) {
    crc ^= b;
    for (let i = 0; i < 8; i++) {
      crc = (crc & 1) ? ((crc >>> 1) ^ 0xedb88320) >>> 0 : (crc >>> 1);
    }
  }
  crc ^= 0xffffffff;
  // 低字节在前 (little-endian)
  return [
    crc & 0xff,
    (crc >>> 8) & 0xff,
    (crc >>> 16) & 0xff,
    (crc >>> 24) & 0xff,
  ];
}

/// Sum8 — 累加和 & 0xFF (校验和)
export function sum8(data: Uint8Array): number[] {
  let s = 0;
  for (const b of data) s = (s + b) & 0xff;
  return [s];
}

/// XOR8 — 逐字节异或
export function xor8(data: Uint8Array): number[] {
  let x = 0;
  for (const b of data) x ^= b;
  return [x];
}

/// LRC — Modbus ASCII Longitudinal Redundancy Check
/// 两倍补码和的低字节
export function lrc(data: Uint8Array): number[] {
  let s = 0;
  for (const b of data) s = (s + b) & 0xff;
  return [((-s) & 0xff)];
}

/// 自定义校验脚本 — 用户提供的 JS 函数体, 接收 bytes 数组, 返回 number[]
///
/// 警告: 使用 new Function 动态求值, 有安全风险 (运行在主进程), 性能不如内置算法
///
/// 示例脚本:
///   // 简单 sum + xor
///   let s = 0, x = 0;
///   for (const b of bytes) { s = (s + b) & 0xff; x ^= b; }
///   return [s, x];
///
/// 沙箱限制: 仅能访问 bytes 参数, 不能访问 window/document/eval
export function customChecksum(data: Uint8Array, script: string): number[] {
  try {
    // eslint-disable-next-line no-new-func
    const fn = new Function('bytes', script) as (bytes: number[]) => number[];
    const result = fn(Array.from(data));
    if (!Array.isArray(result)) {
      throw new Error('Custom checksum script must return an array of numbers');
    }
    return result.map((n) => {
      const num = Number(n);
      if (!Number.isFinite(num)) throw new Error(`Invalid number: ${n}`);
      return num & 0xff;
    });
  } catch (e) {
    throw new Error(`Custom checksum script error: ${(e as Error).message}`);
  }
}

/// 校验类型枚举 (与 types/index.ts 的 ChecksumType 对应)
export type ChecksumKind =
  | 'none'
  | 'sum8'
  | 'xor8'
  | 'crc8'
  | 'crc16Modbus'
  | 'crc16CCITT'
  | 'crc32'
  | 'lrc'
  | 'custom';

/// 计算指定类型的校验和
export function computeChecksum(
  data: Uint8Array,
  kind: ChecksumKind,
  customScript?: string
): number[] {
  switch (kind) {
    case 'none': return [];
    case 'sum8': return sum8(data);
    case 'xor8': return xor8(data);
    case 'crc8': return crc8(data);
    case 'crc16Modbus': return crc16Modbus(data);
    case 'crc16CCITT': return crc16CCITT(data);
    case 'crc32': return crc32(data);
    case 'lrc': return lrc(data);
    case 'custom':
      if (!customScript) return [];
      return customChecksum(data, customScript);
  }
}

/// 校验和字节长度 (用于显示)
export function checksumLength(kind: ChecksumKind): number {
  switch (kind) {
    case 'none': return 0;
    case 'crc32': return 4;
    case 'crc16Modbus':
    case 'crc16CCITT': return 2;
    case 'sum8':
    case 'xor8':
    case 'crc8':
    case 'lrc':
    case 'custom': return 1; // 自定义可能多字节, 显示为 1 (实际看脚本)
  }
}
