import { useMemo } from 'react';
import { Plus } from 'lucide-react';
import { WidgetCard } from '../ui/WidgetCard';
import type { WidgetConfig, MathOp } from '../../types';
import { UNARY_MATH_OPS } from '../../types';
import { useAppStore } from '../../store/appStore';
import { useGraphInputs } from '../../lib/useGraphInput';

interface MathWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Math' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 运算符显示符号
const OP_SYMBOLS: Record<MathOp, string> = {
  add: '+',
  sub: '−',
  mul: '×',
  div: '÷',
  avg: 'avg',
  min: 'min',
  max: 'max',
  abs: '|x|',
  neg: '−x',
  square: 'x²',
  sqrt: '√x',
  sin: 'sin',
  cos: 'cos',
  tan: 'tan',
  log: 'ln',
};

/// 算术控件 — 显示后端图评估的运算结果
///
/// 数据流 (后端评估, 60 FPS 推送):
///   1. 后端 CompiledGraph 按拓扑序评估: 收集本节点的输入 → 调用 MathOp::evaluate → 写入输出端口 "result"
///   2. 后端 graph_output_ticker 每 16ms 将所有节点输出快照推送至前端
///   3. 本组件直接读 graphOutputs[id].result 显示结果
///   4. 输入端口值 (用于展开显示) 通过 useGraphInputs 读上游输出
export function MathWidget({ widget, onEdit }: MathWidgetProps) {
  const { op, unit, precision, inputCount, id } = widget.params;
  // 只订阅本 widget 的结果, 避免 graphOutputs 全局更新时所有 MathWidget 重渲染
  const result = useAppStore((s) => s.graphOutputs[id]?.result ?? 0);

  // 输入端口展示 (单目运算只显示 1 个端口)
  const inputPorts = useMemo(
    () =>
      Array.from({ length: inputCount }, (_, i) => ({
        id: `in${i}`,
        label: UNARY_MATH_OPS.includes(op) && i > 0 ? '' : `in${i}`,
      })),
    [inputCount, op]
  );

  // 读取各输入端口的值 (用于显示, 后端已用这些值算出 result)
  const inputs = useGraphInputs(id, inputPorts.map((p) => p.id), 0);

  const isConnected = Object.keys(inputs).length > 0 &&
    Object.values(inputs).some((v) => v !== 0);
  const symbol = OP_SYMBOLS[op];

  return (
    <WidgetCard badge={symbol} badgeColor="yellow" onEdit={onEdit}>
      <div className="flex flex-col gap-1.5">
        <div className="flex items-baseline gap-1 justify-center py-1.5">
          <span
            className="font-semibold text-text-primary font-mono tracking-[-0.5px]"
            style={{ fontSize: result.toFixed(precision).length > 8 ? 16 : 22 }}
          >
            {result.toFixed(precision)}
          </span>
          {unit && <span className="text-xs text-text-secondary">{unit}</span>}
        </div>
        {!isConnected && (
          <div className="flex items-center gap-1 justify-center p-1 text-[10px] text-text-secondary opacity-70">
            <Plus size={10} />
            <span>连接输入</span>
          </div>
        )}
        {isConnected && (
          <div className="flex flex-col gap-0.5 border-t border-dashed border-border pt-1 mt-0.5">
            {inputPorts.map((p) => (
              <div key={p.id} className="flex justify-between items-center text-[10px] px-0.5 py-px">
                <span className="text-text-secondary font-mono">{p.label}</span>
                <span className="text-text-primary font-mono">
                  {inputs[p.id] !== undefined ? inputs[p.id].toFixed(precision) : '—'}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </WidgetCard>
  );
}
