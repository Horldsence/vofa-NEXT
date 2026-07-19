import { useMemo, useRef, useEffect } from 'react';
import { useAppStore } from '../../store/appStore';
import { useGraphInputs } from '../../lib/useGraphInput';
import { t } from '../../i18n';
import type { WidgetConfig, LoopbackResult } from '../../types';

interface TableViewProps {
  widget: Extract<WidgetConfig, { kind: 'TableView' }>;
  onRemove: () => void;
  /// 可选: 外部注入的回环历史 (来自 CommandSender loopbackHistory)
  loopbackHistory?: LoopbackResult[];
}

/// 通用表格显示控件
///
/// 将上游节点的输出以表格形式展示, 支持:
/// - 列定义 (从 widget.params.columns 推导)
/// - 时间戳列 (可配置开关)
/// - 原始字节列 (可配置开关, 每值显示为 4 字节 HEX)
/// - 最大行数限制
/// - 自动滚动到底部
///
/// 数据来源:
/// - graphInputs (从连线获取实时值)
/// - loopbackHistory (从 CommandSender 回环模式获取历史)
export function TableView({ widget, loopbackHistory }: TableViewProps) {
  const params = widget.params;
  const lang = useAppStore((s) => s.lang);

  // 从列定义推导输入端口名
  const portNames = useMemo(
    () => params.columns.map((c) => c.portName),
    [params.columns]
  );
  const graphInputs = useGraphInputs(params.id, portNames, 0);

  // 表格行历史 (graphInputs 变化时追加)
  const rowsRef = useRef<Array<{ ts: number; values: number[] }>>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  // 监听 graphInputs 变化, 追加新行
  const prevInputsRef = useRef<string>('');
  const currentInputs = portNames.map((p) => graphInputs[p] ?? 0).join(',');
  if (currentInputs !== prevInputsRef.current) {
    prevInputsRef.current = currentInputs;
    const values = portNames.map((p) => graphInputs[p] ?? 0);
    rowsRef.current.push({ ts: Date.now(), values });
    // 裁剪
    if (rowsRef.current.length > params.maxRows) {
      rowsRef.current = rowsRef.current.slice(-params.maxRows);
    }
  }

  // 自动滚动到底部
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [rowsRef.current.length]);

  // 合并 graph 行和回环历史行
  const allRows = useMemo(() => {
    const graphRows = rowsRef.current.map((r) => ({
      ts: r.ts,
      values: r.values,
      isLoopback: false,
    }));
    const loopRows = (loopbackHistory ?? []).map((lr) => ({
      ts: Date.now(),
      values: lr.channels,
      sentHex: lr.sentHex,
      rxBytes: lr.rxBytes,
      frameCount: lr.frameCount,
      canCount: lr.canCount,
      isLoopback: true,
    }));
    // graphRows 先, loopbackRows 后
    return [...graphRows, ...loopRows].slice(-params.maxRows);
  }, [rowsRef.current.length, loopbackHistory]);

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}.${d.getMilliseconds().toString().padStart(3, '0')}`;
  };

  // 将 f32 值转为 4 字节 LE hex
  const floatToHex = (v: number): string => {
    const buf = new ArrayBuffer(4);
    const view = new DataView(buf);
    view.setFloat32(0, v, true); // LE
    const bytes = new Uint8Array(buf);
    return Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join(' ').toUpperCase();
  };

  if (params.columns.length === 0) {
    return (
      <div className="bg-bg-sidebar border border-border rounded flex-1 flex items-center justify-center text-text-secondary text-xs p-4">
        {t(lang, 'tableViewNoColumns')}
      </div>
    );
  }

  return (
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden">
      {/* 表头 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border flex-shrink-0">
        <span className="text-sm font-semibold text-text-bright">{params.label}</span>
        <button
          className="bg-transparent border border-border text-text-secondary px-2 py-0.5 text-xs rounded cursor-pointer hover:text-text-primary hover:border-accent transition-colors"
          onClick={() => {
            // 通过 updateWidget 切换 showRawData
            const updateWidget = useAppStore.getState().updateWidget;
            updateWidget(params.id, {
              kind: 'TableView',
              params: { ...params, showRawData: !params.showRawData },
            });
          }}
        >
          {t(lang, params.showRawData ? 'tableViewHideRaw' : 'tableViewShowRaw')}
        </button>
      </div>

      {/* 表格 */}
      <div ref={scrollRef} className="flex-1 overflow-auto">
        <table className="w-full text-xs border-collapse">
          <thead className="sticky top-0 bg-bg-sidebar z-10">
            <tr className="border-b border-border">
              <th className="px-2 py-1.5 text-left text-text-secondary font-medium w-8">#</th>
              {params.showTimestamp && (
                <th className="px-2 py-1.5 text-left text-text-secondary font-medium whitespace-nowrap">
                  {t(lang, 'tableViewTimestamp')}
                </th>
              )}
              {params.columns.map((col, i) => (
                <th key={i} className="px-2 py-1.5 text-left text-text-secondary font-medium">
                  {col.label}
                  {params.showRawData && col.showRaw !== false && (
                    <span className="ml-1 text-[10px] opacity-50">(HEX)</span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {allRows.length === 0 ? (
              <tr>
                <td
                  colSpan={1 + (params.showTimestamp ? 1 : 0) + params.columns.length}
                  className="px-2 py-6 text-center text-text-secondary opacity-60 italic"
                >
                  {t(lang, 'tableViewEmpty')}
                </td>
              </tr>
            ) : (
              allRows.map((row, ri) => (
                <tr
                  key={ri}
                  className={`border-b border-border/30 hover:bg-bg-hover transition-colors ${
                    'isLoopback' in row && row.isLoopback ? 'bg-blue/5' : ''
                  }`}
                >
                  <td className="px-2 py-1 text-text-secondary font-mono text-[10px]">
                    {ri + 1}
                  </td>
                  {params.showTimestamp && (
                    <td className="px-2 py-1 text-text-secondary font-mono text-[10px] whitespace-nowrap">
                      {formatTime(row.ts)}
                    </td>
                  )}
                  {params.columns.map((col, ci) => {
                    const val = row.values[ci] ?? 0;
                    return (
                      <td key={ci} className="px-2 py-1">
                        <div className="flex items-center gap-1.5">
                          <span className="text-text-primary font-mono text-xs">
                            {Number.isInteger(val) ? val.toFixed(0) : val.toFixed(4)}
                          </span>
                          {params.showRawData && col.showRaw !== false && (
                            <span className="text-text-secondary font-mono text-[9px] opacity-60">
                              {floatToHex(val)}
                            </span>
                          )}
                        </div>
                      </td>
                    );
                  })}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* 状态栏 */}
      <div className="flex items-center justify-between px-3 py-1 border-t border-border flex-shrink-0 text-[10px] text-text-secondary">
        <span>{allRows.length} / {params.maxRows} {t(lang, 'tableViewRows')}</span>
        {loopbackHistory && loopbackHistory.length > 0 && (
          <span className="text-blue">
            {loopbackHistory.length} {t(lang, 'tableViewLoopbackRows')}
          </span>
        )}
      </div>
    </div>
  );
}
