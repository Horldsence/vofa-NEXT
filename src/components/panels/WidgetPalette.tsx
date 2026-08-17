import { Fragment, useState } from 'react';
import clsx from 'clsx';
import { useAppStore } from '../../store/appStore';
import { createWidget } from '../../lib/utils/createWidget';
import { t } from '../../i18n';
import { useSlidingPill, SlidingPill } from '../ui/SlidingPill';
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
  Info,
} from 'lucide-react';
import type { WidgetConfig, WidgetCategory, MathOp, FilterPresetKind } from '../../types';
import { UNARY_MATH_OPS, WIDGET_CATEGORY_COLORS } from '../../types';
import { dockDrag } from '../../lib/dockDrag';

/// 控件面板 — 按 tab 分组分类, 不同类别颜色不同
///
/// 4 个分类 Tab (图标 + 文字分段控件, 滑动指示器与 Dock 卡片 Tab 一致):
///   - input:   数据类 (Knob/Button/Radio/Checkbox/Slider/Command) — 蓝色
///   - display: 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Spectrum/Model3D) — 绿色
///   - math:    算术控件 (Math/Filter) — 橙色, 组内再分「算术 / 滤波器」两节
///   - custom:  自定义控件 (Custom JS) — 紫色

/// 面板项统一模型 — 各分类项归一成同构条目, 渲染走同一套卡片
interface PaletteEntry {
  key: string;
  kind: WidgetConfig['kind'];
  icon: React.ReactNode;
  label: string;
  op?: MathOp;
  preset?: FilterPresetKind;
  onAdd?: () => void;
  title: string;
}

export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  const [activeCategory, setActiveCategory] = useState<WidgetCategory>('input');

  /// 分类 Tab 滑动指示器 (与 DockCardFrame 同一套动效)
  const { containerRef: tabBarRef, pill: tabPill } = useSlidingPill(activeCategory);

  const categories: {
    id: WidgetCategory;
    label: string;
    color: string;
    icon: React.ReactNode;
  }[] = [
    { id: 'input', label: t(lang, 'catInput'), color: WIDGET_CATEGORY_COLORS.input, icon: <Sliders size={13} /> },
    { id: 'display', label: t(lang, 'catDisplay'), color: WIDGET_CATEGORY_COLORS.display, icon: <LineChart size={13} /> },
    { id: 'math', label: t(lang, 'catMath'), color: WIDGET_CATEGORY_COLORS.math, icon: <Sigma size={13} /> },
    { id: 'custom', label: t(lang, 'catCustom'), color: WIDGET_CATEGORY_COLORS.custom, icon: <Code2 size={13} /> },
  ];

  const inputItems: PaletteEntry[] = [
    { key: 'Knob', kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob'), title: t(lang, 'knob') },
    { key: 'Button', kind: 'Button', icon: <Square />, label: t(lang, 'button'), title: t(lang, 'button') },
    { key: 'Radio', kind: 'Radio', icon: <RadioIcon />, label: t(lang, 'radio'), title: t(lang, 'radio') },
    { key: 'Checkbox', kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox'), title: t(lang, 'checkbox') },
    { key: 'Slider', kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider'), title: t(lang, 'slider') },
    { key: 'Command', kind: 'Command', icon: <Send size={14} />, label: t(lang, 'command'), title: t(lang, 'command') },
    { key: 'FrameDecoder', kind: 'FrameDecoder', icon: <ScanText size={14} />, label: t(lang, 'frameDecoder'), title: t(lang, 'frameDecoder') },
  ];

  const displayItems: PaletteEntry[] = [
    { key: 'Waveform', kind: 'Waveform', icon: <LineChart />, label: t(lang, 'waveform'), title: t(lang, 'waveform') },
    { key: 'PieChart', kind: 'PieChart', icon: <PieIcon />, label: t(lang, 'pieChart'), title: t(lang, 'pieChart') },
    { key: 'Image', kind: 'Image', icon: <ImageIcon />, label: t(lang, 'image'), title: t(lang, 'image') },
    { key: 'Gauge', kind: 'Gauge', icon: <GaugeIcon />, label: t(lang, 'gauge'), title: t(lang, 'gauge') },
    { key: 'LED', kind: 'LED', icon: <Lightbulb />, label: t(lang, 'led'), title: t(lang, 'led') },
    { key: 'NumberDisplay', kind: 'NumberDisplay', icon: <Hash />, label: t(lang, 'numberDisplay'), title: t(lang, 'numberDisplay') },
    { key: 'Label', kind: 'Label', icon: <Tag />, label: t(lang, 'label'), title: t(lang, 'label') },
    { key: 'Spectrum', kind: 'Spectrum', icon: <Activity />, label: t(lang, 'spectrum'), title: t(lang, 'spectrum') },
    { key: 'Model3D', kind: 'Model3D', icon: <Box />, label: t(lang, 'model3d'), title: t(lang, 'model3d') },
    { key: 'RawData', kind: 'RawData', icon: <Activity size={14} />, label: t(lang, 'rawData'), title: t(lang, 'rawData') },
  ];

  /// 算术控件子项 — 每种 op 一个快捷入口
  const mathItems: PaletteEntry[] = [
    { key: 'add', kind: 'Math', op: 'add', icon: <Plus />, label: t(lang, 'mathAdd'), title: `${t(lang, 'mathAdd')} (${t(lang, 'mathBinary')})` },
    { key: 'sub', kind: 'Math', op: 'sub', icon: <Minus />, label: t(lang, 'mathSub'), title: `${t(lang, 'mathSub')} (${t(lang, 'mathBinary')})` },
    { key: 'mul', kind: 'Math', op: 'mul', icon: <Square size={14} />, label: t(lang, 'mathMul'), title: `${t(lang, 'mathMul')} (${t(lang, 'mathBinary')})` },
    { key: 'div', kind: 'Math', op: 'div', icon: <Divide />, label: t(lang, 'mathDiv'), title: `${t(lang, 'mathDiv')} (${t(lang, 'mathBinary')})` },
    { key: 'avg', kind: 'Math', op: 'avg', icon: <Sigma />, label: t(lang, 'mathAvg'), title: `${t(lang, 'mathAvg')} (${t(lang, 'mathBinary')})` },
    { key: 'min', kind: 'Math', op: 'min', icon: <Sigma />, label: t(lang, 'mathMin'), title: `${t(lang, 'mathMin')} (${t(lang, 'mathBinary')})` },
    { key: 'max', kind: 'Math', op: 'max', icon: <Sigma />, label: t(lang, 'mathMax'), title: `${t(lang, 'mathMax')} (${t(lang, 'mathBinary')})` },
    { key: 'abs', kind: 'Math', op: 'abs', icon: <Sigma />, label: t(lang, 'mathAbs'), title: `${t(lang, 'mathAbs')} (${t(lang, 'mathUnary')})` },
    { key: 'neg', kind: 'Math', op: 'neg', icon: <Minus />, label: t(lang, 'mathNeg'), title: `${t(lang, 'mathNeg')} (${t(lang, 'mathUnary')})` },
    { key: 'square', kind: 'Math', op: 'square', icon: <Square size={14} />, label: t(lang, 'mathSquare'), title: `${t(lang, 'mathSquare')} (${t(lang, 'mathUnary')})` },
    { key: 'sqrt', kind: 'Math', op: 'sqrt', icon: <Sigma />, label: t(lang, 'mathSqrt'), title: `${t(lang, 'mathSqrt')} (${t(lang, 'mathUnary')})` },
    { key: 'sin', kind: 'Math', op: 'sin', icon: <Sigma />, label: t(lang, 'mathSin'), title: `${t(lang, 'mathSin')} (${t(lang, 'mathUnary')})` },
    { key: 'cos', kind: 'Math', op: 'cos', icon: <Sigma />, label: t(lang, 'mathCos'), title: `${t(lang, 'mathCos')} (${t(lang, 'mathUnary')})` },
    { key: 'tan', kind: 'Math', op: 'tan', icon: <Sigma />, label: t(lang, 'mathTan'), title: `${t(lang, 'mathTan')} (${t(lang, 'mathUnary')})` },
    { key: 'log', kind: 'Math', op: 'log', icon: <Sigma />, label: t(lang, 'mathLog'), title: `${t(lang, 'mathLog')} (${t(lang, 'mathUnary')})` },
  ];

  /// 滤波器预设子项 — 每种 preset 一个快捷入口
  const filterItems: PaletteEntry[] = [
    { key: 'Lowpass', kind: 'Filter', preset: 'Lowpass', icon: <ArrowDownToLine />, label: t(lang, 'filterLowpass'), title: `${t(lang, 'filter')}: ${t(lang, 'filterLowpass')}` },
    { key: 'Highpass', kind: 'Filter', preset: 'Highpass', icon: <ArrowUpToLine />, label: t(lang, 'filterHighpass'), title: `${t(lang, 'filter')}: ${t(lang, 'filterHighpass')}` },
    { key: 'Bandpass', kind: 'Filter', preset: 'Bandpass', icon: <ArrowRightLeft />, label: t(lang, 'filterBandpass'), title: `${t(lang, 'filter')}: ${t(lang, 'filterBandpass')}` },
    { key: 'Bandstop', kind: 'Filter', preset: 'Bandstop', icon: <Ban />, label: t(lang, 'filterBandstop'), title: `${t(lang, 'filter')}: ${t(lang, 'filterBandstop')}` },
  ];

  /// 频域求解子项 — FFT (时域→频域) / IFFT (频域→时域)
  const fftItems: PaletteEntry[] = [
    { key: 'FFT', kind: 'FFT', icon: <Activity />, label: t(lang, 'fft'), title: t(lang, 'fft') },
    { key: 'IFFT', kind: 'IFFT', icon: <Activity />, label: t(lang, 'ifft'), title: t(lang, 'ifft') },
  ];

  const customItems: PaletteEntry[] = [
    {
      key: 'Custom',
      kind: 'Custom',
      icon: <Code2 />,
      label: t(lang, 'custom'),
      title: t(lang, 'custom'),
      onAdd: () => openCustomEditor(),
    },
  ];

  /// 当前分类的分节内容 — math 类别拆成「算术 / 滤波器」两节, 其余单节
  const sections: { header?: string; entries: PaletteEntry[] }[] =
    activeCategory === 'input'
      ? [{ entries: inputItems }]
      : activeCategory === 'display'
        ? [{ entries: displayItems }]
        : activeCategory === 'custom'
          ? [{ entries: customItems }]
          : [
              { header: t(lang, 'catMath'), entries: mathItems },
              { header: t(lang, 'filter'), entries: filterItems },
              { header: t(lang, 'fft'), entries: fftItems },
            ];

  /// 当前类别说明 (单行, 截断时悬停显示全文)
  const helpText =
    activeCategory === 'input'
      ? t(lang, 'catInputHelp')
      : activeCategory === 'display'
        ? t(lang, 'catDisplayHelp')
        : activeCategory === 'math'
          ? t(lang, 'catMathHelp')
          : t(lang, 'catCustomHelp');

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
      }
      mathWidget.params.label = `Math ${op}`;
    }
    // 滤波器控件: 应用所选 preset
    if (kind === 'Filter' && preset) {
      const filterWidget = widget as Extract<WidgetConfig, { kind: 'Filter' }>;
      filterWidget.params.preset = preset;
      filterWidget.params.label = `Filter ${preset}`;
    }
    addWidget(widget, activeControlTabId, { x: 280, y: 80 + Math.random() * 100 });
  };

  /// 各类别的图标底色 / 悬停边框 (静态类名, 保证 Tailwind 可扫描)
  const categoryTileClass: Record<WidgetCategory, string> = {
    input: 'bg-blue/15 text-blue group-hover:bg-blue/25',
    display: 'bg-green/15 text-green group-hover:bg-green/25',
    math: 'bg-orange/15 text-orange group-hover:bg-orange/25',
    custom: 'bg-purple/15 text-purple group-hover:bg-purple/25',
  };

  const categoryHoverClass: Record<WidgetCategory, string> = {
    input: 'hover:border-blue/50',
    display: 'hover:border-green/50',
    math: 'hover:border-orange/50',
    custom: 'hover:border-purple/50',
  };

  /// 统一卡片样式 — 分类色图标块 + 标签, 悬停抬升 + 彩色描边
  const cardClass = (cat: WidgetCategory) =>
    clsx(
      'group bg-bg-input border border-border-subtle rounded-md p-2 flex flex-col items-center gap-2',
      'cursor-grab transition-all duration-150 select-none active:cursor-grabbing active:scale-[0.98]',
      'hover:bg-bg-hover hover:-translate-y-0.5 hover:shadow-[0_6px_16px_rgba(0,0,0,0.35)]',
      categoryHoverClass[cat],
    );

  const tileClass = (cat: WidgetCategory) =>
    clsx(
      'w-9 h-9 rounded-sm flex items-center justify-center [&_svg]:w-4 [&_svg]:h-4 transition-colors',
      categoryTileClass[cat],
    );

  return (
    <div className="flex flex-col h-full overflow-hidden gap-2">
      {/* 分类 Tab — 图标 + 文字分段控件, 滑动指示器与 Dock Tab 一致 */}
      <div
        ref={tabBarRef}
        className="relative flex items-center gap-0.5 p-1 rounded-lg bg-bg-panel-header border border-border-subtle flex-shrink-0"
      >
        <SlidingPill pill={tabPill} />
        {categories.map((cat) => {
          const active = activeCategory === cat.id;
          return (
            <button
              key={cat.id}
              data-tab-key={cat.id}
              className={clsx(
                'relative flex-1 flex items-center justify-center gap-1 h-7 px-1 text-xs font-medium rounded-sm cursor-pointer transition-colors duration-150 select-none whitespace-nowrap',
                active
                  ? 'text-text-bright'
                  : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary',
              )}
              onClick={() => setActiveCategory(cat.id)}
            >
              <span
                className="flex items-center flex-shrink-0 transition-colors"
                style={active ? { color: cat.color } : undefined}
              >
                {cat.icon}
              </span>
              {cat.label}
            </button>
          );
        })}
      </div>

      {/* 控件网格 — auto-rows-min + content-start 防止项被剩余空间纵向拉伸
          负 margin + 等值 padding 扩大裁剪盒: 悬停抬升/阴影不被 overflow 裁掉, 视觉间距不变 */}
      <div className="grid grid-cols-2 gap-2 flex-1 min-h-0 overflow-y-auto auto-rows-min content-start -m-1 p-1">
        {sections.map((section) => (
          <Fragment key={section.header ?? 'main'}>
            {section.header && (
              <div className="col-span-2 px-0.5 pt-1 text-[10px] font-medium uppercase tracking-wider text-text-disabled select-none">
                {section.header}
              </div>
            )}
            {section.entries.map((item) => (
              <div
                key={item.key}
                className={cardClass(activeCategory)}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  if ((e.target as HTMLElement).closest('button, input')) return;
                  dockDrag.begin(e, {
                    kind: 'widget',
                    widget: { kind: item.kind, op: item.op, preset: item.preset },
                    label: item.label,
                  });
                }}
                onClick={() => {
                  if (dockDrag.consumeClick()) return;
                  handleClickAdd(item.kind, item.op, item.onAdd, item.preset);
                }}
                title={item.title}
              >
                <div className={tileClass(activeCategory)}>
                  {item.icon}
                </div>
                <span className="text-[11px] leading-none text-text-secondary transition-colors group-hover:text-text-primary">
                  {item.label}
                </span>
              </div>
            ))}
          </Fragment>
        ))}
      </div>

      {/* 当前类别说明 — 单行提示, 截断时悬停显示全文 */}
      <div className="flex items-center gap-1.5 px-0.5 h-5 text-[10px] text-text-disabled flex-shrink-0 select-none">
        <Info size={11} className="flex-shrink-0" />
        <span className="truncate" title={helpText}>
          {helpText}
        </span>
      </div>
    </div>
  );
}
