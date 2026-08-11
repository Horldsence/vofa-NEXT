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
import { UNARY_MATH_OPS, WIDGET_CATEGORY_COLORS } from '../../types';

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
    { kind: 'RawData', icon: <Activity size={14} />, label: t(lang, 'rawData') },
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
    { id: 'input', label: t(lang, 'catInput'), color: WIDGET_CATEGORY_COLORS.input },
    { id: 'display', label: t(lang, 'catDisplay'), color: WIDGET_CATEGORY_COLORS.display },
    { id: 'math', label: t(lang, 'catMath'), color: WIDGET_CATEGORY_COLORS.math },
    { id: 'custom', label: t(lang, 'catCustom'), color: WIDGET_CATEGORY_COLORS.custom },
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

  /// 各类别的图标底色 / 悬停边框 (静态类名, 保证 Tailwind 可扫描)
  const categoryTileClass: Record<WidgetCategory, string> = {
    input: 'bg-blue/15 text-blue',
    display: 'bg-green/15 text-green',
    math: 'bg-orange/15 text-orange',
    custom: 'bg-purple/15 text-purple',
  };

  const categoryHoverClass: Record<WidgetCategory, string> = {
    input: 'hover:border-blue/50',
    display: 'hover:border-green/50',
    math: 'hover:border-orange/50',
    custom: 'hover:border-purple/50',
  };

  /// 统一卡片样式 — 图标块 + 标签, 圆角悬浮
  const cardClass = (cat: WidgetCategory) =>
    clsx(
      'group bg-bg-input border border-border rounded-lg p-2 flex flex-col items-center gap-1.5',
      'cursor-grab transition-all duration-150 select-none active:cursor-grabbing',
      'hover:bg-bg-hover hover:-translate-y-px hover:shadow-[0_4px_12px_rgba(0,0,0,0.35)]',
      categoryHoverClass[cat],
    );

  const tileClass = (cat: WidgetCategory) =>
    clsx(
      'w-8 h-8 rounded-md flex items-center justify-center [&_svg]:w-4 [&_svg]:h-4',
      'bg-bg-editor text-text-secondary transition-colors',
      categoryTileClass[cat],
    );

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* 分类 Tab — 分段控件风格 */}
      <div className="flex gap-1 p-1 border-b border-border flex-shrink-0 bg-bg-panel-header">
        {categories.map((cat) => {
          const active = activeCategory === cat.id;
          return (
            <button
              key={cat.id}
              className={clsx(
                'flex-1 flex items-center justify-center gap-1.5 h-7 px-1 text-xs font-medium rounded-md cursor-pointer transition-all duration-150 select-none',
                active
                  ? 'bg-bg-editor text-text-bright shadow-[0_1px_3px_rgba(0,0,0,0.35)]'
                  : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary',
              )}
              data-category={cat.id}
              onClick={() => setActiveCategory(cat.id)}
            >
              <span
                className={clsx('w-1.5 h-1.5 rounded-full flex-shrink-0 transition-opacity', !active && 'opacity-50')}
                style={{ background: cat.color }}
              />
              {cat.label}
            </button>
          );
        })}
      </div>

      {/* 控件网格 — auto-rows-min + content-start 防止项被剩余空间纵向拉伸 */}
      <div className="grid grid-cols-2 gap-2 flex-1 overflow-y-auto p-2.5 auto-rows-min content-start">
        {activeCategory === 'math' ? (
          <>
            {/* 算术控件: 每个 op 一个项 */}
            {mathItems.map((item) => (
              <div
                key={item.op}
                className={cardClass('math')}
                draggable
                onDragStart={(e) => handleDragStart(e, 'Math', item.op)}
                onClick={() => handleClickAdd('Math', item.op)}
                title={`${item.label} (${item.isUnary ? t(lang, 'mathUnary') : t(lang, 'mathBinary')})`}
              >
                <div className={tileClass('math')}>
                  {item.icon}
                </div>
                <span className="text-[11px] text-text-secondary transition-colors group-hover:text-text-primary">
                  {item.label}
                </span>
              </div>
            ))}
            {/* 滤波器: 每个 preset 一个项 */}
            {filterItems.map((item) => (
              <div
                key={item.preset}
                className={cardClass('math')}
                draggable
                onDragStart={(e) => handleDragStart(e, 'Filter', undefined, item.preset)}
                onClick={() => handleClickAdd('Filter', undefined, undefined, item.preset)}
                title={`${t(lang, 'filter')}: ${item.label}`}
              >
                <div className={tileClass('math')}>
                  {item.icon}
                </div>
                <span className="text-[11px] text-text-secondary transition-colors group-hover:text-text-primary">
                  {item.label}
                </span>
              </div>
            ))}
          </>
        ) : (
          activeItems.map((item) => (
            <div
              key={item.kind}
              className={cardClass(activeCategory)}
              draggable
              onDragStart={(e) => handleDragStart(e, item.kind)}
              onClick={() => {
                const onAdd = (item as { onAdd?: () => void }).onAdd;
                if (onAdd) handleClickAdd(item.kind, undefined, onAdd);
                else handleClickAdd(item.kind);
              }}
              title={item.label}
            >
              <div className={tileClass(activeCategory)}>
                {item.icon}
              </div>
              <span className="text-[11px] text-text-secondary transition-colors group-hover:text-text-primary">
                {item.label}
              </span>
            </div>
          ))
        )}
      </div>

      {/* 当前类别说明 */}
      <div className="px-2.5 py-2 text-[10px] text-text-secondary border-t border-border bg-bg-panel-header leading-relaxed flex-shrink-0">
        {activeCategory === 'input' && t(lang, 'catInputHelp')}
        {activeCategory === 'display' && t(lang, 'catDisplayHelp')}
        {activeCategory === 'math' && t(lang, 'catMathHelp')}
        {activeCategory === 'custom' && t(lang, 'catCustomHelp')}
      </div>
    </div>
  );
}
