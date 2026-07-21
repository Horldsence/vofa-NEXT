import { useState } from 'react';
import clsx from 'clsx';
import { useAppStore } from '../../store/appStore';
import { createWidget } from '../../lib/createWidget';
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
  Filter as FilterIcon,
  Activity,
  ArrowDownToLine,
  ArrowUpToLine,
  ArrowRightLeft,
  Ban,
  Box,
  Send,
  ScanText,
} from 'lucide-react';
import type { WidgetConfig, WidgetCategory, MathOp, FilterPresetKind } from '../../types';
import { UNARY_MATH_OPS } from '../../types';

/// 控件面板 — 按 tab 分组分类, 不同类别颜色不同
///
/// 4 个分类 Tab:
///   - input:   数据类 (Knob/Button/Radio/Checkbox/Slider/Command) — 蓝色
///   - display: 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Spectrum/Model3D) — 绿色
///   - math:    算术控件 (Math/Filter) — 橙色
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

  /// 滤波器预设子项 — 每种 preset 一个快捷入口
  const filterItems: {
    preset: FilterPresetKind;
    icon: React.ReactNode;
    label: string;
  }[] = [
    { preset: 'Lowpass', icon: <ArrowDownToLine />, label: t(lang, 'filterLowpass') },
    { preset: 'Highpass', icon: <ArrowUpToLine />, label: t(lang, 'filterHighpass') },
    { preset: 'Bandpass', icon: <ArrowRightLeft />, label: t(lang, 'filterBandpass') },
    { preset: 'Bandstop', icon: <Ban />, label: t(lang, 'filterBandstop') },
  ];

  const inputItems: { kind: WidgetConfig['kind']; icon: React.ReactNode; label: string }[] = [
    { kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob') },
    { kind: 'Button', icon: <Square />, label: t(lang, 'button') },
    { kind: 'Radio', icon: <RadioIcon />, label: t(lang, 'radio') },
    { kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox') },
    { kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider') },
    { kind: 'Command', icon: <Send size={14} />, label: t(lang, 'command') },
    { kind: 'FrameDecoder', icon: <ScanText size={14} />, label: t(lang, 'frameDecoder') },
  ];

  const displayItems: { kind: WidgetConfig['kind']; icon: React.ReactNode; label: string }[] = [
    { kind: 'Waveform', icon: <LineChart />, label: t(lang, 'waveform') },
    { kind: 'PieChart', icon: <PieIcon />, label: t(lang, 'pieChart') },
    { kind: 'Image', icon: <ImageIcon />, label: t(lang, 'image') },
    { kind: 'Gauge', icon: <GaugeIcon />, label: t(lang, 'gauge') },
    { kind: 'LED', icon: <Lightbulb />, label: t(lang, 'led') },
    { kind: 'NumberDisplay', icon: <Hash />, label: t(lang, 'numberDisplay') },
    { kind: 'Label', icon: <Tag />, label: t(lang, 'label') },
    { kind: 'Spectrum', icon: <Activity />, label: t(lang, 'spectrum') },
    { kind: 'Model3D', icon: <Box />, label: t(lang, 'model3d') },
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

  const handleDragStart = (
    e: React.DragEvent,
    kind: WidgetConfig['kind'],
    op?: MathOp,
    preset?: FilterPresetKind
  ) => {
    e.dataTransfer.setData('application/widget-kind', kind);
    if (op) e.dataTransfer.setData('application/widget-op', op);
    if (preset) e.dataTransfer.setData('application/widget-preset', preset);
    e.dataTransfer.effectAllowed = 'copy';
    e.stopPropagation();
  };

  const handleClickAdd = (
    kind: WidgetConfig['kind'],
    op?: MathOp,
    onAdd?: () => void,
    preset?: FilterPresetKind
  ) => {
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
    // 滤波器控件: 应用所选 preset
    if (kind === 'Filter' && preset) {
      const filterWidget = widget as Extract<WidgetConfig, { kind: 'Filter' }>;
      filterWidget.params.preset = preset;
      filterWidget.params.label = `Filter ${preset}`;
    }
    addWidget(widget, activeControlTabId, { x: 280, y: 80 + Math.random() * 100 });
  };

  // 当前类别对应的项列表
  const activeItems =
    activeCategory === 'input' ? inputItems :
    activeCategory === 'display' ? displayItems :
    activeCategory === 'custom' ? customItems :
    []; // math 类别特殊处理

  const categoryBorderClass: Record<WidgetCategory, string> = {
    input: 'border-l-[3px] border-l-blue',
    display: 'border-l-[3px] border-l-green',
    math: 'border-l-[3px] border-l-orange bg-gradient-to-r from-orange/10 to-bg-input via-bg-input',
    custom: 'border-l-[3px] border-l-purple',
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* 分类 Tab */}
      <div className="flex border-b border-border flex-shrink-0 bg-bg-panel-header">
        {categories.map((cat) => (
          <button
            key={cat.id}
            className={clsx(
              'flex-1 flex items-center justify-center gap-1 py-2 px-1 text-xs font-medium text-text-secondary bg-transparent border-none border-b-2 border-transparent cursor-pointer transition-all select-none hover:bg-bg-hover hover:text-text-primary',
              activeCategory === cat.id && 'font-semibold',
            )}
            data-category={cat.id}
            onClick={() => setActiveCategory(cat.id)}
            style={{
              borderBottomColor: activeCategory === cat.id ? cat.color : 'transparent',
              color: activeCategory === cat.id ? cat.color : undefined,
            }}
          >
            <span className="w-1.5 h-1.5 rounded-full flex-shrink-0" style={{ background: cat.color }} />
            {cat.label}
          </button>
        ))}
      </div>

      {/* 控件网格 */}
      <div className="grid grid-cols-2 gap-1.5 flex-1 overflow-y-auto p-2">
        {activeCategory === 'math' ? (
          <>
            {/* 算术控件: 每个 op 一个项 */}
            {mathItems.map((item) => (
              <div
                key={item.op}
                className="border-l-[3px] border-l-orange bg-gradient-to-r from-orange/10 to-bg-input via-bg-input border border-border rounded p-2.5 flex flex-col items-center gap-1 cursor-grab transition-all text-xs text-text-secondary select-none hover:border-orange hover:from-orange/20 hover:text-text-primary active:cursor-grabbing"
                draggable
                onDragStart={(e) => handleDragStart(e, 'Math', item.op)}
                onClick={() => handleClickAdd('Math', item.op)}
                title={`${item.label} (${item.isUnary ? t(lang, 'mathUnary') : t(lang, 'mathBinary')})`}
              >
                <div className="w-5 h-5 flex items-center justify-center">
                  {item.icon}
                </div>
                <span>{item.label}</span>
              </div>
            ))}
            {/* 滤波器: 每个 preset 一个项 */}
            {filterItems.map((item) => (
              <div
                key={item.preset}
                className="bg-bg-input border border-orange/30 rounded p-2.5 flex flex-col items-center gap-1 cursor-grab transition-all text-xs text-text-secondary select-none hover:border-orange hover:text-orange active:cursor-grabbing"
                draggable
                onDragStart={(e) => handleDragStart(e, 'Filter', undefined, item.preset)}
                onClick={() => handleClickAdd('Filter', undefined, undefined, item.preset)}
                title={`${t(lang, 'filter')}: ${item.label}`}
              >
                <div className="w-5 h-5 flex items-center justify-center">
                  <FilterIcon size={14} />
                </div>
                <span>{item.label}</span>
              </div>
            ))}
          </>
        ) : (
          activeItems.map((item) => (
            <div
              key={item.kind}
              className={clsx(
                'bg-bg-input border border-border rounded p-2.5 flex flex-col items-center gap-1 cursor-grab transition-all text-xs text-text-secondary select-none hover:bg-bg-hover hover:border-accent hover:text-text-primary active:cursor-grabbing',
                categoryBorderClass[activeCategory],
              )}
              draggable
              onDragStart={(e) => handleDragStart(e, item.kind)}
              onClick={() => {
                const onAdd = (item as { onAdd?: () => void }).onAdd;
                if (onAdd) handleClickAdd(item.kind, undefined, onAdd);
                else handleClickAdd(item.kind);
              }}
              title={item.label}
            >
              <div className="w-5 h-5 flex items-center justify-center">
                {item.icon}
              </div>
              <span>{item.label}</span>
            </div>
          ))
        )}
      </div>

      {/* 当前类别说明 */}
      <div className="px-2 py-1.5 text-[10px] text-text-secondary border-t border-border bg-bg-panel-header leading-relaxed flex-shrink-0">
        {activeCategory === 'input' && t(lang, 'catInputHelp')}
        {activeCategory === 'display' && t(lang, 'catDisplayHelp')}
        {activeCategory === 'math' && t(lang, 'catMathHelp')}
        {activeCategory === 'custom' && t(lang, 'catCustomHelp')}
      </div>
    </div>
  );
}
