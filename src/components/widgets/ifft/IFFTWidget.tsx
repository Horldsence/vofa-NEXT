import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { useNumericOutput } from '../../../lib/hooks/useNumericPort';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';

interface IFFTWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'IFFT' }>;
}

/// 逆 FFT 求解器 — 输入频域 spectrum (来自上游 FFT 求解器), 输出时域 out0
///
/// 数据流 (全部图编译, 后端 Rust):
///   1. 本控件映射为后端 Ifft 节点, 输入端口 "spectrum" (频域) 在编译期
///      解析出上游 FFT (Fft) 节点 id
///   2. 后端 spectrum_ticker 每 33ms 读取该 FFT 的最新频谱, 用 IfftSynth
///      合成时域缓冲 (IfftState.buffer)
///   3. 本节点融入 eval_order, 逐帧从 IfftState 环形播放输出 out0 (时域),
///      下游时域控件 (波形/数字/滤波器等) 可直接连线消费
export const IFFTWidget = memo(function IFFTWidget({ widget }: IFFTWidgetProps) {
  const { id } = widget.params;
  // 输出端口值 (时域) — 由后端图编译逐帧播放
  const out = useNumericOutput(id, 'out0').latest?.value ?? 0;
  // 上游频域源 (spectrum 输入边指向的 FFT widget id)
  const edges = useAppStore((s) => s.rfEdges);
  const widgets = useAppStore((s) => s.widgets);
  const lang = useAppStore((s) => s.lang);
  const sourceEdge = edges.find((e) => e.target === id && e.targetHandle === 'spectrum');
  const sourceWidget = sourceEdge
    ? widgets.find(
        (w): w is Extract<WidgetConfig, { kind: 'FFT' }> =>
          w.kind === 'FFT' && w.params.id === sourceEdge.source
      )
    : undefined;

  return (
    <div className="flex flex-col gap-1 px-1.5 py-1">
      <div className="flex items-baseline justify-center gap-1 py-1">
        <span className="text-[22px] font-semibold text-[#ba68c8] font-mono">
          {out.toFixed(3)}
        </span>
      </div>
      <div className="flex justify-between items-center text-xs px-1 py-0.5 bg-bg-subtle rounded-sm">
        <span className="text-text-secondary">{t(lang, 'ifftSource')}</span>
        <span className="text-text-primary font-mono">
          {sourceWidget ? sourceWidget.params.label : t(lang, 'ifftNoSource')}
        </span>
      </div>
    </div>
  );
});
