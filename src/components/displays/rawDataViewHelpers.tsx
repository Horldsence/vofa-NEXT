import { RAWDATA_BYTES_PER_ROW } from '../../lib/dataBuffer';

export type AppendMode = 'none' | 'nl' | 'tab' | 'nl_tab';
export type SendPanelMode = 'bottom' | 'separate';
export type HexColorMode = 'none' | 'printable' | 'range';

export const ROW_HEIGHT = 22;
export const GROUP_SIZE = 8;

/// 格式化时间戳为 HH:MM:SS.mmm
export function formatTime(ts: number): string {
  if (!ts) return '--:--:--.---';
  const d = new Date(ts);
  const h = d.getHours().toString().padStart(2, '0');
  const m = d.getMinutes().toString().padStart(2, '0');
  const s = d.getSeconds().toString().padStart(2, '0');
  const ms = d.getMilliseconds().toString().padStart(3, '0');
  return `${h}:${m}:${s}.${ms}`;
}

/// 格式化单字节为 2 位大写 hex
export function byteToHex(b: number): string {
  return b.toString(16).padStart(2, '0').toUpperCase();
}

/// 格式化单字节为 ascii（不可打印显示为 .）
export function byteToAscii(b: number): string {
  return b >= 32 && b < 127 ? String.fromCharCode(b) : '.';
}

/// 判断字节是否为可打印 ASCII
export function isPrintable(b: number): boolean {
  return b >= 32 && b < 127;
}

/// Hex 字节颜色类（根据颜色模式）
export function hexColorClass(b: number, mode: HexColorMode): string {
  if (mode === 'none') return 'text-text-primary';
  if (mode === 'printable') {
    if (b === 0) return 'text-text-disabled';
    return isPrintable(b) ? 'text-text-primary' : 'text-text-secondary';
  }
  // range 模式
  if (b === 0x00) return 'text-text-disabled';
  if (b === 0xff) return 'text-red';
  if (isPrintable(b)) return 'text-blue';
  if (b < 0x20) return 'text-yellow';
  return 'text-text-secondary';
}

/// 表头：00 01 02 ... 0F
export function HeaderBytes({ width }: { width: number }) {
  return (
    <>
      {Array.from({ length: RAWDATA_BYTES_PER_ROW }, (_, i) => {
        const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== RAWDATA_BYTES_PER_ROW - 1;
        return (
          <span
            key={i}
            className={`inline-flex items-center justify-center text-text-secondary text-xs font-mono ${isGroupEnd ? 'mr-2' : ''}`}
            style={{ width }}
          >
            {byteToHex(i)}
          </span>
        );
      })}
    </>
  );
}
