import { useMemo, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { RefreshCw, Search, Usb } from 'lucide-react';
import type { PortInfo } from '../../types';
import { activateOnKeyboard } from '../../lib/utils/a11y';

/// 端口条目 (端口信息 + 在原始列表中的索引)
interface PortEntry {
  port: PortInfo;
  idx: number;
}

/// 端口分组 — macOS 上同一设备的 /dev/tty.X 与 /dev/cu.X 合并为一组
interface PortGroup {
  key: string;
  cu?: PortEntry;
  tty?: PortEntry;
  single?: PortEntry;
}

/// 解析 /dev/tty.X /dev/cu.X 形式的端口名, 返回 { key, variant }
function parseDevicePath(name: string): { key: string; variant: 'cu' | 'tty' } | null {
  const m = /^\/dev\/(tty|cu)\.(.+)$/.exec(name);
  if (!m) return null;
  return { variant: m[1] as 'cu' | 'tty', key: m[2] };
}

/// 端口选择器 — 可复用受控组件, 供 Serial / Slcan 共享
/// 显示端口列表 (含筛选) + 刷新按钮, 选中时通过 onSelect 回报端口名
///
/// macOS 优化: 同一设备的 tty.X / cu.X 成对出现时合并为一张卡片,
/// 默认使用 cu (打开时不等待载波信号), 可通过卡片内的 cu/tty 切换器更换
export function PortPicker({
  selectedPortName,
  onSelect,
}: {
  /// 当前选中的端口名 (受控)
  selectedPortName: string;
  /// 选中端口回调
  onSelect: (portName: string) => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const ports = useAppStore((s) => s.ports);
  const refreshPorts = useAppStore((s) => s.refreshPorts);

  const [filter, setFilter] = useState('');
  /// 每个分组当前选用的变体 (默认 cu)
  const [variantByKey, setVariantByKey] = useState<Record<string, 'cu' | 'tty'>>({});

  // 筛选端口: 按名称 / 产品 / 厂商 / VID:PID 匹配, 随后按设备合并 tty/cu 对
  const filteredGroups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const entries = ports
      .map((port, idx) => ({ port, idx }))
      .filter(({ port }) => {
        if (!q) return true;
        const vidPid = `${port.vid ?? ''}:${port.pid ?? ''}`;
        return (
          port.name.toLowerCase().includes(q) ||
          (port.product ?? '').toLowerCase().includes(q) ||
          (port.manufacturer ?? '').toLowerCase().includes(q) ||
          (port.description ?? '').toLowerCase().includes(q) ||
          vidPid.includes(q)
        );
      });

    const groups: PortGroup[] = [];
    const byKey = new Map<string, PortGroup>();
    for (const entry of entries) {
      const parsed = parseDevicePath(entry.port.name);
      if (!parsed) {
        groups.push({ key: entry.port.name, single: entry });
        continue;
      }
      let group = byKey.get(parsed.key);
      if (!group) {
        group = { key: parsed.key };
        byKey.set(parsed.key, group);
        groups.push(group);
      }
      group[parsed.variant] = entry;
    }
    return groups;
  }, [ports, filter]);

  return (
    <div className="mb-2">
      <div className="flex gap-2 items-center mb-2 px-0.5">
        <span className="block text-[11px] font-medium uppercase tracking-wider text-text-secondary m-0 flex-1">
          {t(lang, 'portName')}
        </span>
        <button
          className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
          title={t(lang, 'refresh')}
          onClick={() => { void refreshPorts(); }}
        >
          <RefreshCw size={13} />
        </button>
      </div>
      <div className="flex items-center gap-1.5 h-7 bg-bg-input border border-border rounded-md px-2 mb-2 text-text-secondary focus-within:border-accent transition-colors">
        <Search size={12} className="flex-shrink-0" />
        <input
          type="text"
          className="search-input bg-transparent border-none h-full flex-1 focus:outline-none text-text-primary text-xs"
          placeholder={lang === 'zh' ? '筛选端口...' : 'Filter ports...'}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="flex flex-col gap-1">
        {ports.length === 0 ? (
          <div className="p-3 text-text-secondary text-xs text-center">
            {lang === 'zh' ? '未发现串口' : 'No ports found'}
          </div>
        ) : filteredGroups.length === 0 ? (
          <div className="p-3 text-text-secondary text-xs text-center">
            {lang === 'zh' ? '无匹配端口' : 'No matching ports'}
          </div>
        ) : (
          filteredGroups.map((group) => {
            const paired = group.cu && group.tty;
            // 当前选用的条目: 成对时按 variantByKey (默认 cu), 否则取仅有的一个
            const active =
              group.single ??
              (paired
                ? (variantByKey[group.key] ?? 'cu') === 'tty'
                  ? group.tty!
                  : group.cu!
                : (group.cu ?? group.tty)!);
            const port = active.port;
            // 元数据取自同组任一条目 (同一硬件, cu/tty 的 VID/PID 等一致)
            const meta = (group.cu ?? group.tty ?? group.single!).port;
            const selected =
              port.name === selectedPortName ||
              (!!paired &&
                (group.cu!.port.name === selectedPortName ||
                  group.tty!.port.name === selectedPortName));
            const vidPid =
              meta.vid !== null || meta.pid !== null
                ? `VID ${meta.vid !== null ? meta.vid.toString(16).toUpperCase().padStart(4, '0') : '----'}  PID ${meta.pid !== null ? meta.pid.toString(16).toUpperCase().padStart(4, '0') : '----'}`
                : null;
            // 显示名省略 /dev/ 前缀 (完整路径见 title)
            const displayName = port.name.replace(/^\/dev\//, '');
            return (
              <div
                key={group.key}
                className={`group px-2 py-2 rounded-md cursor-pointer flex items-start gap-2 border transition-all duration-150 ${
                  selected
                    ? 'bg-bg-active border-accent/50 shadow-[0_1px_4px_rgba(0,0,0,0.3)]'
                    : 'border-transparent hover:bg-bg-hover hover:border-border'
                }`}
                onClick={() => onSelect(port.name)}
                onKeyDown={activateOnKeyboard}
                role="button"
                tabIndex={0}
              >
                {/* 端口图标 */}
                <div
                  className={`w-7 h-7 rounded-sm flex items-center justify-center flex-shrink-0 border transition-colors ${
                    selected
                      ? 'bg-accent/15 border-accent/30 text-accent'
                      : 'bg-bg-input border-border text-text-secondary group-hover:text-text-primary'
                  }`}
                >
                  <Usb size={13} />
                </div>
                {/* 端口信息 — 全部内容允许换行, 不截断 */}
                <div className="flex-1 min-w-0 flex flex-col gap-1">
                  <div className="flex items-start gap-2">
                    <span
                      className="text-xs font-medium text-text-primary font-mono break-all leading-snug"
                      title={port.name}
                    >
                      {displayName}
                    </span>
                    <span className="ml-auto flex-shrink-0 text-[9px] leading-4 px-1.5 rounded-full bg-accent/15 text-accent font-medium">
                      {port.port_type}
                    </span>
                  </div>
                  {/* macOS tty/cu 成对端口: 变体切换器 (cu 打开即返回, tty 等待载波) */}
                  {paired && (
                    <div
                      className="flex items-center gap-0.5 w-fit p-0.5 rounded-md bg-bg-editor border border-border"
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={activateOnKeyboard}
                      role="button"
                      tabIndex={0}
                    >
                      {(['cu', 'tty'] as const).map((v) => {
                        const entry = v === 'cu' ? group.cu! : group.tty!;
                        const isActive = active.idx === entry.idx;
                        return (
                          <button
                            key={v}
                            type="button"
                            className={`px-1.5 h-4.5 text-[9px] font-mono font-medium rounded-sm cursor-pointer transition-colors ${
                              isActive
                                ? 'bg-accent/20 text-accent'
                                : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover'
                            }`}
                            title={`/dev/${v}.${group.key}`}
                            onClick={() => {
                              setVariantByKey((s) => ({ ...s, [group.key]: v }));
                              onSelect(entry.port.name);
                            }}
                          >
                            {v}
                          </button>
                        );
                      })}
                    </div>
                  )}
                  {meta.description && meta.description !== meta.product && (
                    <div className="text-xs text-text-primary leading-snug break-words">
                      {meta.description}
                    </div>
                  )}
                  {(meta.product ?? meta.manufacturer) && (
                    <div className="text-[11px] text-text-secondary leading-snug break-words">
                      {[meta.product, meta.manufacturer].filter(Boolean).join(' · ')}
                    </div>
                  )}
                  {(vidPid ?? meta.serial_number) && (
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] font-mono text-text-secondary/70 leading-snug">
                      {vidPid && <span>{vidPid}</span>}
                      {meta.serial_number && (
                        <span className="break-all">S/N {meta.serial_number}</span>
                      )}
                    </div>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
