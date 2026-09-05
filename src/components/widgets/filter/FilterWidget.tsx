import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { useNumericInput, useNumericOutput } from '../../../lib/hooks/useNumericPort';
import { NumericPortStatus } from '../../displays/common/NumericPortStatus';

interface FilterWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Filter' }>;
}

/// 滤波器控件 — 显示后端图评估的滤波结果
///
/// 数据流 (后端逐点滤波, 60 FPS 推送):
///   1. 后端 CompiledGraph 在 eval_order 中评估 Filter 节点:
///      - 取输入 "in0" 上游值 → DigitalFilter.process(value) → 输出端口 "result"
///      - 滤波器状态 (FIR 延迟线 / IIR biquad state) 跨帧持久化
///   2. 后端 graph_output_ticker 每 16ms 将所有节点输出快照推送至前端
///   3. 本组件直接读 graphOutputs[id].result 显示结果
///
/// 配置变更 (preset/cutoff/sampleRate 等) 在节点属性面板编辑 → updateWidget
/// → syncTabGraph → 后端重建 DigitalFilter (kind 变化触发状态重置, 符合滤波器语义)。
/// 卡片内只保留结果 / 输入值显示。
export const FilterWidget = memo(function FilterWidget({ widget }: FilterWidgetProps) {
  const { precision, id } = widget.params;

  // 读取输入端口值 (用于显示)
  const input = useNumericInput(id, 'in0');
  const inputValue = input.latest?.value ?? 0;
  // 后端滤波后的结果
  const output = useNumericOutput(id, 'result');
  const result = output.latest?.value ?? 0;

  return (
    <div className="flex flex-col gap-1 px-1.5 py-1">
      <div className="flex items-baseline justify-center gap-1 py-1">
        <span className="text-[22px] font-semibold text-[#ff8c42] font-mono">
          {output.latest ? result.toFixed(precision) : '—'}
        </span>
      </div>
      <div className="flex justify-between items-center text-xs px-1 py-0.5 bg-bg-subtle rounded-sm">
        <span className="text-text-secondary">in</span>
        <span className="text-text-primary font-mono">{inputValue.toFixed(precision)}</span>
      </div>
      <div className="text-center"><NumericPortStatus state={output} /></div>
    </div>
  );
});
