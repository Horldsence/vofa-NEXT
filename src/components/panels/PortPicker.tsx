import { useMemo, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { RefreshCw, Search, Usb, Cpu } from 'lucide-react';

/// 端口选择器 — 可复用组件, 供 Serial / Slcan 共享
/// 显示端口列表 (含筛选) + 刷新按钮, 选中时同步 selectedPortIndex + port_name
///
/// 与旧 PortConfig 不同:
/// - 不再包含串口参数表单 (由 TransportConfigPanel 按需渲染)
/// - 不再包含连接按钮 (统一在 TransportConfigPanel 底部)
/// - selectedPortIndex 默认 -1, 无 false 高亮
export function PortPicker() {
  const lang = useAppStore((s) => s.lang);
  const ports = useAppStore((s) => s.ports);
  const selectedPortIndex = useAppStore((s) => s.selectedPortIndex);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const selectPort = useAppStore((s) => s.selectPort);

  const [filter, setFilter] = useState('');

  // 筛选端口: 按名称 / 产品 / 厂商 / VID:PID 匹配
  const filteredPorts = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return ports.map((p, idx) => ({ port: p, idx }));
    return ports
      .map((p, idx) => ({ port: p, idx }))
      .filter(({ port }) => {
        const vidPid = `${port.vid ?? ''}:${port.pid ?? ''}`;
        return (
          port.name.toLowerCase().includes(q) ||
          (port.product ?? '').toLowerCase().includes(q) ||
          (port.manufacturer ?? '').toLowerCase().includes(q) ||
          vidPid.includes(q)
        );
      });
  }, [ports, filter]);

  return (
    <div className="mb-2.5">
      <div className="flex gap-2 items-center mb-1.5">
        <span className="block text-xs text-text-secondary m-0 flex-1">
          {t(lang, 'portName')}
        </span>
        <button
          className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
          title={t(lang, 'refresh')}
          onClick={() => refreshPorts()}
        >
          <RefreshCw size={14} />
        </button>
      </div>
      <div className="flex items-center gap-1.5 bg-bg-input border border-border rounded px-2 mb-1.5 text-text-secondary">
        <Search size={12} />
        <input
          type="text"
          className="bg-transparent border-none py-1 flex-1 focus:outline-none text-text-primary text-sm"
          placeholder={lang === 'zh' ? '筛选端口...' : 'Filter ports...'}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="flex flex-col gap-0.5">
        {ports.length === 0 ? (
          <div className="p-3 text-text-secondary text-xs text-center">
            {lang === 'zh' ? '未发现串口' : 'No ports found'}
          </div>
        ) : filteredPorts.length === 0 ? (
          <div className="p-3 text-text-secondary text-xs text-center">
            {lang === 'zh' ? '无匹配端口' : 'No matching ports'}
          </div>
        ) : (
          filteredPorts.map(({ port, idx }) => (
            <div
              key={port.name}
              className={`px-2 py-1.5 rounded cursor-pointer flex flex-col gap-0.5 transition-colors hover:bg-bg-hover ${idx === selectedPortIndex ? 'bg-bg-active' : ''}`}
              onClick={() => selectPort(idx)}
            >
              <div className="text-sm text-text-primary font-mono">
                <Usb size={10} className="inline mr-1" />
                {port.name}
              </div>
              {(port.product || port.manufacturer) && (
                <div className="text-[10px] text-text-secondary">
                  <Cpu size={9} className="inline mr-0.5 align-middle" />
                  {[port.product, port.manufacturer]
                    .filter(Boolean)
                    .join(' • ')}
                </div>
              )}
              {(port.vid !== null || port.pid !== null) && (
                <div className="text-[10px] font-mono text-blue">
                  VID:{port.vid !== null ? port.vid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                  {'  PID:'}
                  {port.pid !== null ? port.pid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                  {'  ('}
                  {port.port_type}
                  {')'}
                </div>
              )}
              {port.serial_number && (
                <div className="text-[10px] font-mono text-text-secondary">
                  S/N: {port.serial_number}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
