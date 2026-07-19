import { useEffect, useMemo, useRef, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { useCanLoadAlarmStore } from '../../store/canLoadAlarmStore';
import { api } from '../../lib/tauri';
import { notify, formatError } from '../../lib/notifications';
import { t } from '../../i18n';
import { Trash2, Activity, Download, Bell, BellOff } from 'lucide-react';
import type { CanLoadSnapshot, CanIdLoadHistory } from '../../types';

/// 窗口预设
const WINDOW_PRESETS: { label: string; value: number }[] = [
  { label: '100ms', value: 100_000 },
  { label: '500ms', value: 500_000 },
  { label: '1s', value: 1_000_000 },
  { label: '5s', value: 5_000_000 },
  { label: '10s', value: 10_000_000 },
];

/// 波特率预设
const BITRATE_PRESETS: { label: string; value: number }[] = [
  { label: '100k', value: 100_000 },
  { label: '125k', value: 125_000 },
  { label: '250k', value: 250_000 },
  { label: '500k', value: 500_000 },
  { label: '1M', value: 1_000_000 },
];

/// 告警阈值预设
const THRESHOLD_PRESETS: { label: string; value: number }[] = [
  { label: '50%', value: 0.5 },
  { label: '60%', value: 0.6 },
  { label: '70%', value: 0.7 },
  { label: '80%', value: 0.8 },
  { label: '90%', value: 0.9 },
];

/// 负载率颜色 (绿 → 黄 → 红)
function loadColor(ratio: number): string {
  if (ratio < 0.6) return '#89d185';
  if (ratio < 0.85) return '#d1c04d';
  return '#d18585';
}

/// 格式化百分比
function formatPercent(ratio: number): string {
  return (ratio * 100).toFixed(2) + '%';
}

/// 格式化帧率
function formatFps(fps: number): string {
  return fps.toFixed(1) + ' fps';
}

/// CAN 负载分析视图
///
/// - 仪表盘 (当前负载率 + 帧率 + 总帧数)
/// - 时序图 (history 折线, 负载率随时间变化)
/// - ID 负载分布 (按 total_bits 排序)
/// - 窗口大小 / 波特率选择器
export function CanLoadView() {
  const lang = useAppStore((s) => s.lang);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const threshold = useCanLoadAlarmStore((s) => s.threshold);
  const setThreshold = useCanLoadAlarmStore((s) => s.setThreshold);
  const alarmEnabled = useCanLoadAlarmStore((s) => s.enabled);
  const setAlarmEnabled = useCanLoadAlarmStore((s) => s.setEnabled);

  const [snapshot, setSnapshot] = useState<CanLoadSnapshot | null>(null);
  const [windowUs, setWindowUs] = useState<number>(1_000_000);
  const [bitrateOverride, setBitrateOverride] = useState<number | null>(null);
  const [detectedBitrate, setDetectedBitrate] = useState<{ bps: number; source: string }>({
    bps: 500_000,
    source: 'default',
  });
  const [autoBitrate, setAutoBitrate] = useState(true);
  const [exporting, setExporting] = useState(false);
  /// 选中的 ID (用于时序图叠加显示); null = 不选中
  const [selectedId, setSelectedId] = useState<{ id: number; extended: boolean } | null>(null);

  // 拉取当前 bitrate (用于显示 source + 自动模式默认值)
  useEffect(() => {
    api
      .getCurrentCanBitrate()
      .then(([bps, source]) => setDetectedBitrate({ bps, source }))
      .catch(() => {});
  }, [transportConfig]);

  // 设置窗口大小时同步后端
  useEffect(() => {
    void api.setCanLoadWindow(windowUs);
  }, [windowUs]);

  // 订阅 CAN 负载推送
  useEffect(() => {
    const effectiveBitrate = autoBitrate ? null : bitrateOverride;
    const sub = api.subscribeCanLoad(setSnapshot, {
      intervalMs: 500,
      bitrateBps: effectiveBitrate,
    });
    return () => sub.cancel();
  }, [autoBitrate, bitrateOverride]);

  // 初始拉取一次
  useEffect(() => {
    const effectiveBitrate = autoBitrate ? null : bitrateOverride;
    api.getCanLoadStats(effectiveBitrate).then(setSnapshot).catch(() => {});
  }, [autoBitrate, bitrateOverride]);

  const handleClear = () => {
    void api.clearCanLoadStats();
    if (snapshot) {
      setSnapshot({ ...snapshot, frame_count: 0, total_bits: 0, total_bytes: 0, load_ratio: 0, history: [], per_id: [], per_id_history: [] });
    }
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const effectiveBitrate = autoBitrate ? null : bitrateOverride;
      const path = await api.exportCanLoadCsv(effectiveBitrate);
      notify.info(t(lang, 'canLoadExportSuccess'), path, { source: 'can-load-export' });
    } catch (e) {
      notify.error(t(lang, 'canLoadExportFailed'), formatError(e), { source: 'can-load-export' });
    } finally {
      setExporting(false);
    }
  };

  const effectiveBitrate = autoBitrate ? detectedBitrate.bps : (bitrateOverride ?? 500_000);
  const loadRatio = snapshot?.load_ratio ?? 0;
  const fps = snapshot && snapshot.window_us > 0
    ? (snapshot.frame_count * 1_000_000) / snapshot.window_us
    : 0;

  return (
    <div className="h-full w-full flex flex-col overflow-hidden bg-bg-editor">
      {/* 工具栏 */}
      <div className="flex items-center gap-4 px-3 py-2 border-b border-border bg-bg-panel-header text-xs">
        <div className="flex items-center gap-2">
          <span className="text-text-secondary">{t(lang, 'canLoadWindow')}</span>
          <select
            className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary text-xs"
            value={windowUs}
            onChange={(e) => setWindowUs(Number(e.target.value))}
          >
            {WINDOW_PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
        </div>

        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1 cursor-pointer">
            <input
              type="checkbox"
              checked={autoBitrate}
              onChange={(e) => setAutoBitrate(e.target.checked)}
              className="cursor-pointer"
            />
            <span className="text-text-secondary">{t(lang, 'canLoadAutoBitrate')}</span>
            {autoBitrate && (
              <span className="text-text-bright font-mono text-[10px] px-1.5 py-0.5 rounded bg-bg-input">
                {detectedBitrate.bps >= 1_000_000
                  ? `${detectedBitrate.bps / 1_000_000}M (${detectedBitrate.source})`
                  : `${detectedBitrate.bps / 1000}k (${detectedBitrate.source})`}
              </span>
            )}
          </label>
          {!autoBitrate && (
            <select
              className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary text-xs"
              value={bitrateOverride ?? 500_000}
              onChange={(e) => setBitrateOverride(Number(e.target.value))}
            >
              {BITRATE_PRESETS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          )}
        </div>

        <div className="flex-1" />

        {/* 告警阈值选择器 + 启用开关 */}
        <div className="flex items-center gap-1">
          <button
            className={`flex items-center gap-1 px-1.5 py-1 rounded transition-colors ${
              alarmEnabled
                ? 'text-accent hover:bg-bg-hover'
                : 'text-text-secondary hover:bg-bg-hover'
            }`}
            onClick={() => setAlarmEnabled(!alarmEnabled)}
            title={t(lang, 'canLoadThresholdAlarm')}
          >
            {alarmEnabled ? <Bell size={12} /> : <BellOff size={12} />}
          </button>
          <span className="text-text-secondary">{t(lang, 'canLoadThreshold')}</span>
          <select
            className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary text-xs"
            value={threshold}
            disabled={!alarmEnabled}
            onChange={(e) => setThreshold(Number(e.target.value))}
          >
            {THRESHOLD_PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
        </div>

        <button
          className="flex items-center gap-1 px-2 py-1 text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors disabled:opacity-50"
          onClick={handleExport}
          disabled={exporting || !snapshot}
          title={t(lang, 'canLoadExport')}
        >
          <Download size={12} />
          <span>{t(lang, 'canLoadExport')}</span>
        </button>

        <button
          className="flex items-center gap-1 px-2 py-1 text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors"
          onClick={handleClear}
          title={t(lang, 'canLoadClear')}
        >
          <Trash2 size={12} />
          <span>{t(lang, 'canLoadClear')}</span>
        </button>
      </div>

      {/* 主体内容 */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {/* 仪表盘 + 时序图 */}
        <div className="grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-4">
          <LoadGauge loadRatio={loadRatio} fps={fps} frameCount={snapshot?.frame_count ?? 0} />
          <LoadHistoryChart
            history={snapshot?.history ?? []}
            windowUs={windowUs}
            selectedIdHistory={selectedId ? findIdHistory(snapshot, selectedId) : null}
          />
        </div>

        {/* 概要统计 */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
          <StatCard label={t(lang, 'canLoadBitrate')} value={formatBitrate(effectiveBitrate)} />
          <StatCard label={t(lang, 'canLoadTotalBits')} value={(snapshot?.total_bits ?? 0).toLocaleString()} />
          <StatCard label={t(lang, 'canLoadTotalBytes')} value={(snapshot?.total_bytes ?? 0).toLocaleString()} />
          <StatCard label={t(lang, 'canLoadFps')} value={formatFps(fps)} />
        </div>

        {/* ID 负载分布 */}
        <IdLoadDistribution
          snapshot={snapshot}
          selectedId={selectedId}
          onSelectId={(id, extended) => {
            setSelectedId((cur) =>
              cur && cur.id === id && cur.extended === extended ? null : { id, extended }
            );
          }}
        />
      </div>
    </div>
  );
}

/// 从 snapshot.per_id_history 中查找指定 ID 的历史
function findIdHistory(
  snapshot: CanLoadSnapshot | null,
  sel: { id: number; extended: boolean }
): CanIdLoadHistory | null {
  if (!snapshot) return null;
  return (
    snapshot.per_id_history.find((h) => h.id === sel.id && h.extended === sel.extended) ?? null
  );
}

/// 格式化波特率
function formatBitrate(bps: number): string {
  if (bps >= 1_000_000) return `${bps / 1_000_000} Mbps`;
  return `${bps / 1000} kbps`;
}

/// 仪表盘 — 半圆指针式负载率
function LoadGauge({ loadRatio, fps, frameCount }: { loadRatio: number; fps: number; frameCount: number }) {
  const ratio = Math.min(loadRatio, 1.5); // 钳制到 1.5 上限
  const angle = -90 + Math.min(ratio / 1.0, 1.0) * 180; // -90° (左) → 90° (右)
  const color = loadColor(loadRatio);

  return (
    <div className="bg-bg-panel-header rounded border border-border p-4 flex flex-col items-center">
      <div className="text-xs text-text-secondary mb-2 uppercase tracking-wider">
        Bus Load
      </div>
      <svg viewBox="0 0 200 120" className="w-full max-w-[260px]">
        {/* 背景弧 */}
        <path
          d="M 20 100 A 80 80 0 0 1 180 100"
          fill="none"
          stroke="#3c3c3c"
          strokeWidth="12"
          strokeLinecap="round"
        />
        {/* 负载弧 */}
        <path
          d="M 20 100 A 80 80 0 0 1 180 100"
          fill="none"
          stroke={color}
          strokeWidth="12"
          strokeLinecap="round"
          strokeDasharray={`${Math.min(ratio / 1.0, 1.0) * 251.3} 251.3`}
        />
        {/* 指针 */}
        <g transform={`rotate(${angle} 100 100)`}>
          <line x1="100" y1="100" x2="100" y2="35" stroke="#e0e0e0" strokeWidth="2" strokeLinecap="round" />
          <circle cx="100" cy="100" r="5" fill="#e0e0e0" />
        </g>
        {/* 中心数值 */}
        <text
          x="100"
          y="80"
          textAnchor="middle"
          fill={color}
          fontSize="18"
          fontWeight="bold"
          fontFamily="monospace"
        >
          {formatPercent(loadRatio)}
        </text>
      </svg>
      <div className="grid grid-cols-2 gap-4 w-full mt-2 text-xs">
        <div className="flex flex-col items-center">
          <span className="text-text-secondary text-[10px]">FPS</span>
          <span className="text-text-bright font-mono">{formatFps(fps)}</span>
        </div>
        <div className="flex flex-col items-center">
          <span className="text-text-secondary text-[10px]">Frames</span>
          <span className="text-text-bright font-mono">{frameCount.toLocaleString()}</span>
        </div>
      </div>
    </div>
  );
}

/// 时序图 — 负载率历史折线 + 选中 ID 叠加
function LoadHistoryChart({
  history,
  windowUs,
  selectedIdHistory,
}: {
  history: CanLoadSnapshot['history'];
  windowUs: number;
  selectedIdHistory: CanIdLoadHistory | null;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // 绘制时序图
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const dpr = window.devicePixelRatio || 1;
    const w = container.clientWidth;
    const h = container.clientHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    // 背景
    ctx.fillStyle = '#1e1e1e';
    ctx.fillRect(0, 0, w, h);

    const padding = { top: 16, right: 12, bottom: 24, left: 36 };
    const plotW = w - padding.left - padding.right;
    const plotH = h - padding.top - padding.bottom;

    // 网格 (Y: 0%, 50%, 100%; X: 时间)
    ctx.strokeStyle = '#3c3c3c';
    ctx.lineWidth = 1;
    ctx.fillStyle = '#8a8a8a';
    ctx.font = '10px monospace';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';

    for (const [ratio, label] of [[0, '0%'], [0.5, '50%'], [1.0, '100%']] as const) {
      const y = padding.top + (1 - ratio) * plotH;
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(padding.left + plotW, y);
      ctx.stroke();
      ctx.fillText(label, padding.left - 4, y);
    }

    // 100% 警戒线 (红色虚线)
    ctx.strokeStyle = '#d18585';
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(padding.left, padding.top);
    ctx.lineTo(padding.left + plotW, padding.top);
    ctx.stroke();
    ctx.setLineDash([]);

    if (history.length === 0) {
      ctx.fillStyle = '#8a8a8a';
      ctx.textAlign = 'center';
      ctx.fillText('No data', w / 2, h / 2);
      return;
    }

    // X 轴: 时间 (历史点数), 旧 → 新
    const n = history.length;
    const xStep = n > 1 ? plotW / (n - 1) : 0;

    // 主折线 (总负载率, 蓝色)
    ctx.strokeStyle = '#75beff';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const x = padding.left + i * xStep;
      const y = padding.top + (1 - Math.min(history[i].load_ratio, 1.0)) * plotH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // 主折线填充
    ctx.lineTo(padding.left + (n - 1) * xStep, padding.top + plotH);
    ctx.lineTo(padding.left, padding.top + plotH);
    ctx.closePath();
    ctx.fillStyle = 'rgba(117, 190, 255, 0.15)';
    ctx.fill();

    // 选中 ID 的叠加曲线 (橙色)
    if (selectedIdHistory && selectedIdHistory.history.length > 0) {
      const idHist = selectedIdHistory.history;
      const m = idHist.length;
      const xStepId = m > 1 ? plotW / (m - 1) : 0;
      ctx.strokeStyle = '#ffb86c';
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 2]);
      ctx.beginPath();
      for (let i = 0; i < m; i++) {
        const x = padding.left + i * xStepId;
        const y = padding.top + (1 - Math.min(idHist[i].load_ratio, 1.0)) * plotH;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.setLineDash([]);

      // 图例
      ctx.fillStyle = '#75beff';
      ctx.fillRect(padding.left, 4, 10, 2);
      ctx.fillStyle = '#8a8a8a';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
      ctx.fillText('Total', padding.left + 14, 0);
      ctx.fillStyle = '#ffb86c';
      ctx.fillRect(padding.left + 60, 4, 10, 2);
      ctx.fillStyle = '#8a8a8a';
      ctx.fillText(
        `ID 0x${selectedIdHistory.extended
          ? selectedIdHistory.id.toString(16).toUpperCase().padStart(8, '0')
          : selectedIdHistory.id.toString(16).toUpperCase().padStart(3, '0')}${selectedIdHistory.extended ? 'X' : ''}`,
        padding.left + 74,
        0
      );
    }

    // X 轴标签
    ctx.fillStyle = '#8a8a8a';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    const windowLabel =
      windowUs >= 1_000_000
        ? `${windowUs / 1_000_000}s`
        : `${windowUs / 1000}ms`;
    ctx.fillText(`-${history.length} samples`, padding.left, h - 18);
    ctx.textAlign = 'right';
    ctx.fillText(`now (${windowLabel} window)`, w - padding.right, h - 18);
  }, [history, windowUs, selectedIdHistory]);

  return (
    <div className="bg-bg-panel-header rounded border border-border p-3 flex flex-col">
      <div className="flex items-center justify-between mb-2">
        <span className="text-xs text-text-secondary uppercase tracking-wider">
          {t(useAppStore.getState().lang, 'canLoadHistory')}
        </span>
        <Activity size={12} className="text-text-secondary" />
      </div>
      <div ref={containerRef} className="flex-1 min-h-[160px] relative">
        <canvas ref={canvasRef} className="absolute inset-0" />
      </div>
    </div>
  );
}

/// 统计卡片
function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-bg-panel-header rounded border border-border p-3 flex flex-col gap-1">
      <span className="text-text-secondary text-[10px] uppercase tracking-wider">{label}</span>
      <span className="text-text-bright font-mono text-sm">{value}</span>
    </div>
  );
}

/// ID 负载分布
function IdLoadDistribution({
  snapshot,
  selectedId,
  onSelectId,
}: {
  snapshot: CanLoadSnapshot | null;
  selectedId: { id: number; extended: boolean } | null;
  onSelectId: (id: number, extended: boolean) => void;
}) {
  const lang = useAppStore.getState().lang;
  const perId = snapshot?.per_id ?? [];
  const maxBits = perId.length > 0 ? perId[0].total_bits : 1;

  const totalBits = useMemo(() => perId.reduce((sum, s) => sum + s.total_bits, 0), [perId]);

  return (
    <div className="bg-bg-panel-header rounded border border-border p-3">
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs text-text-secondary uppercase tracking-wider">
          {t(lang, 'canLoadPerId')}
        </span>
        <span className="text-xs text-text-secondary font-mono">
          {perId.length} IDs{selectedId ? ` · ${t(lang, 'canLoadIdFilterHint')}` : ''}
        </span>
      </div>
      {perId.length === 0 ? (
        <div className="flex items-center justify-center h-32 text-text-secondary text-xs rounded border border-dashed border-border">
          {t(lang, 'noCanFrames')}
        </div>
      ) : (
        <div className="space-y-1.5 max-h-[280px] overflow-y-auto">
          {perId.map((s) => {
            const pct = (s.total_bits / maxBits) * 100;
            const sharePct = totalBits > 0 ? (s.total_bits / totalBits) * 100 : 0;
            const isSelected =
              selectedId !== null &&
              selectedId.id === s.id &&
              selectedId.extended === s.extended;
            return (
              <div
                key={`${s.id}-${s.extended}`}
                className={`grid grid-cols-[6rem_1fr_4rem] items-center gap-3 text-xs font-mono cursor-pointer rounded px-1 py-0.5 transition-colors ${
                  isSelected
                    ? 'bg-accent/20 border border-accent'
                    : 'border border-transparent hover:bg-bg-hover'
                } ${selectedId && !isSelected ? 'opacity-50' : ''}`}
                onClick={() => onSelectId(s.id, s.extended)}
                title={t(lang, 'canLoadIdFilterHint')}
              >
                <span className="text-text-bright truncate">
                  0x{s.extended
                    ? s.id.toString(16).toUpperCase().padStart(8, '0')
                    : s.id.toString(16).toUpperCase().padStart(3, '0')}
                  {s.extended && <span className="ml-1 text-accent text-[10px]">X</span>}
                </span>
                <div className="bg-bg-input rounded h-5 overflow-hidden relative">
                  <div
                    className={`absolute inset-y-0 left-0 ${isSelected ? 'bg-orange/80' : 'bg-blue/70'}`}
                    style={{ width: `${pct}%` }}
                  />
                  <div className="absolute inset-0 flex items-center px-2 text-[10px] text-text-bright gap-2">
                    <span>{s.frame_count}f</span>
                    <span className="text-text-secondary">{sharePct.toFixed(1)}%</span>
                  </div>
                </div>
                <span className="text-text-secondary text-right">{s.total_bits}b</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
