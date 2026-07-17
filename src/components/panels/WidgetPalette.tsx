import { useState } from 'react';
import { useAppStore, createWidget } from '../../store/appStore';
import { t } from '../../i18n';
import {
  Gauge as KnobIcon,
  Square,
  CheckSquare,
  Sliders,
  Tag,
  LineChart,
  PieChart as PieIcon,
  Image as ImageIcon,
  Radio as RadioIcon,
  Gauge as GaugeIcon,
  Lightbulb,
  Hash,
  Code2,
  Plus,
  Minus,
  Divide,
  Sigma,
} from 'lucide-react';
import type { WidgetConfig, WidgetCategory, MathOp } from '../../types';
import { UNARY_MATH_OPS } from '../../types';

/// 控件面板 — 按 tab 分组分类, 不同类别颜色不同
///
/// 4 个分类 Tab:
///   - input:   输入控件 (Knob/Button/Radio/Checkbox/Slider) — 蓝色
///   - display: 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label) — 绿色
///   - math:    算术控件 (Math) — 橙色
///   - custom:  自定义控件 (Custom JS) — 紫色
export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  const [activeCategory, setActiveCategory] = useState<WidgetCategory>('input');

  /// 算术控件子项 — 每种 op 一个快捷入口
  const mathItems: {
    op: MathOp;
    icon: React.ReactNode;
    label: string;
    isUnary: boolean;
  }[] = [
    { op: 'add', icon: <Plus />, label: t(lang, 'mathAdd'), isUnary: false },
    { op: 'sub', icon: <Minus />, label: t(lang, 'mathSub'), isUnary: false },
    { op: 'mul', icon: <Square size={14} />, label: t(lang, 'mathMul'), isUnary: false },
    { op: 'div', icon: <Divide />, label: t(lang, 'mathDiv'), isUnary: false },
    { op: 'avg', icon: <Sigma />, label: t(lang, 'mathAvg'), isUnary: false },
    { op: 'min', icon: <Sigma />, label: t(lang, 'mathMin'), isUnary: false },
    { op: 'max', icon: <Sigma />, label: t(lang, 'mathMax'), isUnary: false },
    { op: 'abs', icon: <Sigma />, label: t(lang, 'mathAbs'), isUnary: true },
    { op: 'neg', icon: <Minus />, label: t(lang, 'mathNeg'), isUnary: true },
    { op: 'square', icon: <Square size={14} />, label: t(lang, 'mathSquare'), isUnary: true },
    { op: 'sqrt', icon: <Sigma />, label: t(lang, 'mathSqrt'), isUnary: true },
    { op: 'sin', icon: <Sigma />, label: t(lang, 'mathSin'), isUnary: true },
    { op: 'cos', icon: <Sigma />, label: t(lang, 'mathCos'), isUnary: true },
    { op: 'tan', icon: <Sigma />, label: t(lang, 'mathTan'), isUnary: true },
    { op: 'log', icon: <Sigma />, label: t(lang, 'mathLog'), isUnary: true },
  ];

  const inputItems: { kind: WidgetConfig['kind']; icon: React.ReactNode; label: string }[] = [
    { kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob') },
    { kind: 'Button', icon: <Square />, label: t(lang, 'button') },
    { kind: 'Radio', icon: <RadioIcon />, label: t(lang, 'radio') },
    { kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox') },
    { kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider') },
  ];

  const displayItems: { kind: WidgetConfig['kind']; icon: React.ReactNode; label: string }[] = [
    { kind: 'Waveform', icon: <LineChart />, label: t(lang, 'waveform') },
    { kind: 'PieChart', icon: <PieIcon />, label: t(lang, 'pieChart') },
    { kind: 'Image', icon: <ImageIcon />, label: t(lang, 'image') },
    { kind: 'Gauge', icon: <GaugeIcon />, label: t(lang, 'gauge') },
    { kind: 'LED', icon: <Lightbulb />, label: t(lang, 'led') },
    { kind: 'NumberDisplay', icon: <Hash />, label: t(lang, 'numberDisplay') },
    { kind: 'Label', icon: <Tag />, label: t(lang, 'label') },
  ];

  const customItems: {
    kind: WidgetConfig['kind'];
    icon: React.ReactNode;
    label: string;
    onAdd?: () => void;
  }[] = [
    {
      kind: 'Custom',
      icon: <Code2 />,
      label: t(lang, 'custom'),
      onAdd: () => openCustomEditor(),
    },
  ];

  const categories: {
    id: WidgetCategory;
    label: string;
    color: string;
  }[] = [
    { id: 'input', label: t(lang, 'catInput'), color: '#4fc3f7' },
    { id: 'display', label: t(lang, 'catDisplay'), color: '#81c784' },
    { id: 'math', label: t(lang, 'catMath'), color: '#ffb74d' },
    { id: 'custom', label: t(lang, 'catCustom'), color: '#ba68c8' },
  ];

  const handleDragStart = (e: React.DragEvent, kind: WidgetConfig['kind'], op?: MathOp) => {
    e.dataTransfer.setData('application/widget-kind', kind);
    if (op) e.dataTransfer.setData('application/widget-op', op);
    e.dataTransfer.effectAllowed = 'copy';
    e.stopPropagation();
  };

  const handleClickAdd = (kind: WidgetConfig['kind'], op?: MathOp, onAdd?: () => void) => {
    if (onAdd) {
      onAdd();
      return;
    }
    const widget = createWidget(kind);
    // 算术控件: 应用所选 op
    if (kind === 'Math' && op) {
      const mathWidget = widget as Extract<WidgetConfig, { kind: 'Math' }>;
      mathWidget.params.op = op;
      if (UNARY_MATH_OPS.includes(op)) {
        mathWidget.params.inputCount = 1;
        mathWidget.params.label = `Math ${op}`;
      } else {
        mathWidget.params.label = `Math ${op}`;
      }
    }
    addWidget(widget, activeControlTabId, { x: 280, y: 80 + Math.random() * 100 });
  };

  // 当前类别对应的项列表
  const activeItems =
    activeCategory === 'input' ? inputItems :
    activeCategory === 'display' ? displayItems :
    activeCategory === 'custom' ? customItems :
    []; // math 类别特殊处理

  return (
    <div className="palette-container">
      {/* 分类 Tab */}
      <div className="palette-category-tabs">
        {categories.map((cat) => (
          <button
            key={cat.id}
            className={`palette-category-tab ${activeCategory === cat.id ? 'active' : ''}`}
            data-category={cat.id}
            onClick={() => setActiveCategory(cat.id)}
            style={{
              borderBottomColor: activeCategory === cat.id ? cat.color : 'transparent',
              color: activeCategory === cat.id ? cat.color : undefined,
            }}
          >
            <span className="palette-category-dot" style={{ background: cat.color }} />
            {cat.label}
          </button>
        ))}
      </div>

      {/* 控件网格 */}
      <div className="palette-grid">
        {activeCategory === 'math' ? (
          // 算术控件: 每个 op 一个项
          mathItems.map((item) => (
            <div
              key={item.op}
              className="palette-item palette-item-math"
              draggable
              onDragStart={(e) => handleDragStart(e, 'Math', item.op)}
              onClick={() => handleClickAdd('Math', item.op)}
              title={`${item.label} (${item.isUnary ? t(lang, 'mathUnary') : t(lang, 'mathBinary')})`}
            >
              {item.icon}
              <span>{item.label}</span>
            </div>
          ))
        ) : (
          activeItems.map((item) => (
            <div
              key={item.kind}
              className={`palette-item palette-item-${activeCategory}`}
              draggable
              onDragStart={(e) => handleDragStart(e, item.kind)}
              onClick={() => {
                const onAdd = (item as { onAdd?: () => void }).onAdd;
                if (onAdd) handleClickAdd(item.kind, undefined, onAdd);
                else handleClickAdd(item.kind);
              }}
              title={item.label}
            >
              {item.icon}
              <span>{item.label}</span>
            </div>
          ))
        )}
      </div>

      {/* 当前类别说明 */}
      <div className="palette-category-help">
        {activeCategory === 'input' && t(lang, 'catInputHelp')}
        {activeCategory === 'display' && t(lang, 'catDisplayHelp')}
        {activeCategory === 'math' && t(lang, 'catMathHelp')}
        {activeCategory === 'custom' && t(lang, 'catCustomHelp')}
      </div>
    </div>
  );
}
