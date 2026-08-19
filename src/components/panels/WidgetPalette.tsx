import { useRef, useState } from 'react';
import clsx from 'clsx';
import { useAppStore } from '../../store/appStore';
import { createWidget } from '../../lib/utils/createWidget';
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
  Cable,
  Binary,
  ChevronRight,
} from 'lucide-react';
import type { WidgetConfig, WidgetCategory, MathOp, FilterPresetKind, TransportConfig } from '../../types';
import { UNARY_MATH_OPS, WIDGET_CATEGORY_COLORS } from '../../types';
import { dockDrag } from '../../lib/dockDrag';

/// 控件面板 — 紧凑单行列表 + 可折叠分组 + 顶部分类跳转条
///
/// 分组顺序: 数据 → 数据接口 → 协议引擎 → 显示 → 算术 → 滤波器 → 频域 → 自定义
/// 顶部跳转条点击图标平滑滚动到对应分组 (折叠时自动展开), 滚动时高亮当前分组。
/// 每个控件一行 (分类色小图标 + 名称), 左键拖拽或单击均可添加。

/// 面板项统一模型 — 各分类项归一成同构条目, 渲染走同一套行样式
interface PaletteEntry {
  key: string;
  kind?: WidgetConfig['kind'];
  icon: React.ReactNode;
  label: string;
  op?: MathOp;
  preset?: FilterPresetKind;
  /// 全局节点条目: 数据接口 / 协议引擎 (拖入或点击创建全局节点)
  globalNode?: 'transport' | 'protocol';
  transportKind?: TransportConfig['kind'];
  onAdd?: () => void;
  title: string;
}

type SectionId = 'input' | 'transport' | 'protocol' | 'display' | 'math' | 'filter' | 'fft' | 'custom';

interface PaletteSection {
  id: SectionId;
  header: string;
  /// 图标块 / 跳转条所用分类色
  category: WidgetCategory;
  entries: PaletteEntry[];
}

export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);
  const addTransportNode = useAppStore((s) => s.addTransportNode);
  const addProtocolNode = useAppStore((s) => s.addProtocolNode);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);

  /// 分组折叠状态 — 默认全部展开, 仅本次会话内有效
  const [collapsed, setCollapsed] = useState<Partial<Record<SectionId, boolean>>>({});
  /// 当前可视分组 (跳转条高亮)
  const [activeSection, setActiveSection] = useState<SectionId>('input');
  const listRef = useRef<HTMLDivElement>(null);
  const sectionRefs = useRef<Partial<Record<SectionId, HTMLElement | null>>>({});

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

  /// 数据接口子项 — 每种传输类型一个全局节点入口
  const transportItems: PaletteEntry[] = (
    [
      ['Serial', 'serial'],
      ['Udp', 'udp'],
      ['TcpClient', 'tcpClient'],
      ['TcpServer', 'tcpServer'],
      ['TestData', 'testData'],
      ['Slcan', 'slcan'],
      ['CandleLight', 'candleLight'],
    ] as [TransportConfig['kind'], Parameters<typeof t>[1]][]
  ).map(([kind, key]) => ({
    key: `transport-${kind}`,
    globalNode: 'transport' as const,
    transportKind: kind,
    icon: <Cable size={14} />,
    label: t(lang, key),
    title: `${t(lang, 'dataInterface')}: ${t(lang, key)}`,
  }));

  /// 协议引擎子项 — 全局节点入口
  const protocolItems: PaletteEntry[] = [
    {
      key: 'protocol',
      globalNode: 'protocol' as const,
      icon: <Binary size={14} />,
      label: t(lang, 'protocolEngine'),
      title: t(lang, 'protocolEngine'),
    },
  ];

  const sections: PaletteSection[] = [
    { id: 'input', header: t(lang, 'catInput'), category: 'input', entries: inputItems },
    { id: 'transport', header: t(lang, 'dataInterface'), category: 'input', entries: transportItems },
    { id: 'protocol', header: t(lang, 'protocolEngine'), category: 'input', entries: protocolItems },
    { id: 'display', header: t(lang, 'catDisplay'), category: 'display', entries: displayItems },
    { id: 'math', header: t(lang, 'catMath'), category: 'math', entries: mathItems },
    { id: 'filter', header: t(lang, 'filter'), category: 'math', entries: filterItems },
    { id: 'fft', header: t(lang, 'fft'), category: 'math', entries: fftItems },
    { id: 'custom', header: t(lang, 'catCustom'), category: 'custom', entries: customItems },
  ];

  /// 顶部跳转条 — 算术/滤波器/频域合并为一个「算术」跳转入口
  const jumpTargets: { id: SectionId; label: string; color: string; icon: React.ReactNode }[] = [
    { id: 'input', label: t(lang, 'catInput'), color: WIDGET_CATEGORY_COLORS.input, icon: <Sliders size={14} /> },
    { id: 'transport', label: t(lang, 'dataInterface'), color: WIDGET_CATEGORY_COLORS.input, icon: <Cable size={14} /> },
    { id: 'protocol', label: t(lang, 'protocolEngine'), color: WIDGET_CATEGORY_COLORS.input, icon: <Binary size={14} /> },
    { id: 'display', label: t(lang, 'catDisplay'), color: WIDGET_CATEGORY_COLORS.display, icon: <LineChart size={14} /> },
    { id: 'math', label: t(lang, 'catMath'), color: WIDGET_CATEGORY_COLORS.math, icon: <Sigma size={14} /> },
    { id: 'custom', label: t(lang, 'catCustom'), color: WIDGET_CATEGORY_COLORS.custom, icon: <Code2 size={14} /> },
  ];

  const handleClickAdd = (item: PaletteEntry) => {
    if (item.onAdd) {
      item.onAdd();
      return;
    }
    // 全局节点: 数据接口 / 协议引擎
    if (item.globalNode === 'transport') {
      addTransportNode(item.transportKind ?? 'Serial', { x: 60, y: 60 + Math.random() * 60 });
      return;
    }
    if (item.globalNode === 'protocol') {
      addProtocolNode(undefined, { x: 300, y: 60 + Math.random() * 60 });
      return;
    }
    if (!item.kind) return;
    const kind = item.kind;
    const op = item.op;
    const preset = item.preset;
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

  /// 跳转到分组 — 折叠时先展开, 再平滑滚动到位
  /// 只滚动内部列表容器 (不用 scrollIntoView, 避免连带滚动祖先容器把跳转条顶出窗口上部)
  const jumpTo = (id: SectionId) => {
    setCollapsed((c) => (c[id] ? { ...c, [id]: false } : c));
    setActiveSection(id);
    const list = listRef.current;
    const el = sectionRefs.current[id];
    if (!list || !el) return;
    list.scrollTo({
      top: list.scrollTop + el.getBoundingClientRect().top - list.getBoundingClientRect().top,
      behavior: 'smooth',
    });
  };

  /// 滚动时同步当前可视分组 — 取顶部偏移量不超过滚动位置 (留一行余量) 的最后一个分组
  const handleScroll = () => {
    const list = listRef.current;
    if (!list) return;
    let current: SectionId = 'input';
    for (const s of sections) {
      const el = sectionRefs.current[s.id];
      if (el && el.offsetTop - list.offsetTop <= list.scrollTop + 32) {
        current = s.id;
      }
    }
    if (current !== activeSection) setActiveSection(current);
  };

  /// 跳转条高亮归属 — 滤波器/频域归入「算术」入口
  const jumpActive: SectionId =
    activeSection === 'filter' || activeSection === 'fft' ? 'math' : activeSection;

  /// 各类别的图标底色 (静态类名, 保证 Tailwind 可扫描)
  const categoryTileClass: Record<WidgetCategory, string> = {
    input: 'bg-blue/15 text-blue group-hover:bg-blue/25',
    display: 'bg-green/15 text-green group-hover:bg-green/25',
    math: 'bg-orange/15 text-orange group-hover:bg-orange/25',
    custom: 'bg-purple/15 text-purple group-hover:bg-purple/25',
  };

  /// 行样式 — 单行: 分类色图标块 + 名称, 字号取 theme token (--font-size-sm)
  const rowClass =
    'group flex items-center gap-2 h-8 px-1.5 rounded-sm cursor-grab select-none transition-colors duration-150 active:cursor-grabbing hover:bg-bg-hover';

  const tileClass = (cat: WidgetCategory) =>
    clsx(
      'w-6 h-6 rounded-sm flex items-center justify-center flex-shrink-0 [&_svg]:w-4 [&_svg]:h-4 transition-colors',
      categoryTileClass[cat],
    );

  return (
    <div className="flex flex-col h-full overflow-hidden gap-1.5">
      {/* 分类跳转条 — 点击图标滚动到对应分组, 当前可视分组高亮 */}
      <div className="flex items-center gap-0.5 p-1 rounded-lg bg-bg-panel-header border border-border-subtle flex-shrink-0">
        {jumpTargets.map((target) => {
          const active = jumpActive === target.id;
          return (
            <button
              key={target.id}
              title={target.label}
              className={clsx(
                'flex-1 flex items-center justify-center h-7 rounded-sm cursor-pointer transition-colors duration-150 select-none',
                active ? 'bg-bg-hover' : 'hover:bg-bg-hover',
              )}
              onClick={() => jumpTo(target.id)}
            >
              <span
                className="flex items-center transition-colors"
                style={{ color: active ? target.color : undefined }}
              >
                <span className={active ? '' : 'text-text-secondary'}>{target.icon}</span>
              </span>
            </button>
          );
        })}
      </div>

      {/* 控件列表 — 折叠分组 + 紧凑单行条目 */}
      <div ref={listRef} onScroll={handleScroll} className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-0.5">
        {sections.map((section) => {
          const isCollapsed = collapsed[section.id] ?? false;
          return (
            <div
              key={section.id}
              ref={(el) => {
                sectionRefs.current[section.id] = el;
              }}
              className="flex flex-col gap-0.5"
            >
              <button
                className="flex items-center gap-1 w-full px-1 pt-1.5 pb-0.5 text-[length:var(--font-size-xs)] font-medium uppercase tracking-wider text-text-disabled hover:text-text-secondary cursor-pointer select-none transition-colors"
                onClick={() => setCollapsed((c) => ({ ...c, [section.id]: !isCollapsed }))}
              >
                <ChevronRight
                  size={12}
                  className={clsx('flex-shrink-0 transition-transform duration-150', !isCollapsed && 'rotate-90')}
                />
                {section.header}
              </button>
              {!isCollapsed &&
                section.entries.map((item) => (
                  <div
                    key={item.key}
                    className={rowClass}
                    onPointerDown={(e) => {
                      if (e.button !== 0) return;
                      if ((e.target as HTMLElement).closest('button, input')) return;
                      dockDrag.begin(e, {
                        kind: 'widget',
                        widget: {
                          kind: item.kind,
                          op: item.op,
                          preset: item.preset,
                          globalNode: item.globalNode,
                          transportKind: item.transportKind,
                        },
                        label: item.label,
                      });
                    }}
                    onClick={() => {
                      if (dockDrag.consumeClick()) return;
                      handleClickAdd(item);
                    }}
                    title={item.title}
                  >
                    <div className={tileClass(section.category)}>{item.icon}</div>
                    <span className="text-[length:var(--font-size-sm)] leading-none truncate text-text-secondary transition-colors group-hover:text-text-primary">
                      {item.label}
                    </span>
                  </div>
                ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
