import { useState, useEffect, useMemo } from 'react';
import { X, Settings2, Plus } from 'lucide-react';
import type { WidgetConfig, MathOp } from '../../types';
import { computeMathResult, UNARY_MATH_OPS } from '../../types';
import { useAppStore } from '../../store/appStore';
import { readAllInputs } from '../../lib/widgetDataFlow';

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

/// 算术控件 — 对多个通道输入做四则运算/数学函数, 输出单通道结果
///
/// 数据流:
///   1. 从 rfEdges 读取所有连到本 widget 的 edge
///   2. 对每条 edge:
///      - sourceHandle 形如 "ch0": 从 waveformWindow 读最新通道值
///      - sourceHandle 形如 "result"/"value": 从 widgetOutputCache 读上游 widget 输出
///   3. 调用 computeMathResult(op, inputs) 计算结果
///   4. 写入 widgetOutputCache 供下游 widget 读取
export function MathWidget({ widget, onRemove, onEdit }: MathWidgetProps) {
  const { op, unit, precision, inputCount, label, id } = widget.params;
  const [result, setResult] = useState<number>(0);
  const [inputs, setInputs] = useState<Record<string, number>>({});
  const rfEdges = useAppStore((s) => s.rfEdges);
  const widgetOutputCache = useAppStore((s) => s.widgetOutputCache);
  const setWidgetOutput = useAppStore((s) => s.setWidgetOutput);

  // 定时读取上游值 + 计算结果 + 写入 cache (50ms 节流)
  useEffect(() => {
    const tick = () => {
      const next = readAllInputs(id, rfEdges, widgetOutputCache);
      setInputs(next);
      const inputArr = Object.values(next);
      const r = computeMathResult(op, inputArr);
      setResult(r);
      // 写入 cache 供下游 widget 读取
      setWidgetOutput(id, 'result', r);
    };
    tick();
    const interval = setInterval(tick, 50);
    return () => clearInterval(interval);
  }, [op, id, rfEdges, widgetOutputCache, setWidgetOutput]);

  // 输入端口展示 (单目运算只显示 1 个端口)
  const inputPorts = useMemo(
    () =>
      Array.from({ length: inputCount }, (_, i) => ({
        id: `in${i}`,
        label: UNARY_MATH_OPS.includes(op) && i > 0 ? '' : `in${i}`,
      })),
    [inputCount, op]
  );

  const isConnected = Object.keys(inputs).length > 0;
  const symbol = OP_SYMBOLS[op];

  return (
    <div className="widget-card math-widget">
      <button className="btn-icon widget-remove" onClick={onRemove} title="Remove">
        <X size={12} />
      </button>
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
      <div className="widget-label">
        {label}
        <span className="math-widget-op-symbol">{symbol}</span>
      </div>
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
