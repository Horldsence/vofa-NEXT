import { useMemo } from 'react';
import { Settings2, Plus } from 'lucide-react';
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
  const graphOutputs = useAppStore((s) => s.graphOutputs);

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
  // 后端计算的结果
  const result = graphOutputs[id]?.result ?? 0;

  const isConnected = Object.keys(inputs).length > 0 &&
    Object.values(inputs).some((v) => v !== 0);
  const symbol = OP_SYMBOLS[op];

  return (
    <div className="widget-card math-widget">
      {onEdit && (
        <button
          className="btn-icon widget-edit"
          onClick={onEdit}
          title="Edit"
          style={{ right: 24 }}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="math-widget-op-symbol">{symbol}</div>
      <div className="math-widget-body">
        <div className="math-widget-result">
          <span
            className="math-widget-result-value"
            style={{ fontSize: result.toFixed(precision).length > 8 ? 16 : 22 }}
          >
            {result.toFixed(precision)}
          </span>
          {unit && <span className="math-widget-unit">{unit}</span>}
        </div>
        {!isConnected && (
          <div className="math-widget-hint">
            <Plus size={10} />
            <span>连接输入</span>
          </div>
        )}
        {isConnected && (
          <div className="math-widget-inputs">
            {inputPorts.map((p) => (
              <div key={p.id} className="math-widget-input-row">
                <span className="math-widget-input-label">{p.label}</span>
                <span className="math-widget-input-value">
                  {inputs[p.id] !== undefined ? inputs[p.id].toFixed(precision) : '—'}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
