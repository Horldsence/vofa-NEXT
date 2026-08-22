import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useAppStore } from '../../../store/appStore';
import { createWidget } from '../../../lib/utils/createWidget';
import { t } from '../../../i18n';
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
  Zap,
  Cable,
  Binary,
  Search,
  X,
  FileText,
  Type,
  Ruler,
  TextSearch,
  ArrowLeftToLine,
  ArrowRightToLine,
  Scissors,
  Link2,
  TextCursorInput,
  Delete,
  Replace,
  CaseUpper,
  CaseLower,
  Eraser,
  ArrowLeftRight,
} from 'lucide-react';
import type { WidgetConfig, TransportConfig, MathOp, StrOp } from '../../../types';
import { UNARY_MATH_OPS, WIDGET_CATEGORY_COLORS } from '../../../types';
import type { PaletteEntry, PaletteSection, SectionId } from './paletteModel';
import { flattenSections, filterSections, sectionAnchors, sectionAtScroll, totalSizeOf, HEADER_SIZE, ROW_SIZE } from './paletteModel';
import { JumpBar, type JumpTarget } from './JumpBar';
import { PaletteRow, SectionHeader } from './PaletteRow';

/// 控件面板 — 虚拟列表 + 可折叠分组 + 顶部分类跳转条
///
/// 分组顺序: 数据 → 数据接口 → 协议引擎 → 显示 → 算术 → 滤波器 → 频域 → 自定义
/// 列表由 useVirtualizer 驱动 (扁平 header/row 模型, 固定行高免 DOM 测量),
/// 行/分组头/跳转条均 memo 化, 滚动与高亮切换只触发最小范围重渲染。
/// 行高一律 px 硬编码并与 paletteModel 行高常量同源 — 根字号为 13px,
/// rem 类与行高估算的像素差会在滚动中累积成跳转落点偏差。
/// 跳转: 点击图标平滑滚动到分组 header (折叠时自动展开), 目标偏移截断到
/// 可滚动范围; 跳转期间高亮锁定在点击目标, 滚动到位或用户滚轮/触摸接管时
/// 结束并交还给滚动位置推导, 高亮不沿滚动路径闪烁。
/// 动画: 展开行淡入入场 (播过一次不再重放), 折叠行淡出退场后剔除。

/// 退场/入场动画时长 — 与 components.css 中 palette-row keyframes 一致
const EXIT_MS = 150;
const ENTER_MS = 200;

export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);
  const addTransportNode = useAppStore((s) => s.addTransportNode);
  const addProtocolNode = useAppStore((s) => s.addProtocolNode);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);

  /// 分组折叠状态 — 默认全部展开, 仅本次会话内有效
  const [collapsed, setCollapsed] = useState<Partial<Record<SectionId, boolean>>>({});
  /// 正在播放退场动画的分组 — 动画期间行保留在模型中, 结束后才真正折叠
  const [collapsing, setCollapsing] = useState<SectionId | null>(null);
  /// 刚展开的分组 — 其行播放入场动画, 动画结束清除
  const [enteringSection, setEnteringSection] = useState<SectionId | null>(null);
  /// 当前可视分组 (跳转条高亮)
  const [activeSection, setActiveSection] = useState<SectionId>('input');

  const listRef = useRef<HTMLDivElement>(null);
  /// 进行中的跳转目标 (截断后的滚动偏移) — 平滑滚动到位或用户接管时结束;
  /// 跳转期间高亮锁定在点击目标上, 不随滚动路径变化, 从根上杜绝高亮闪烁
  const jumpRef = useRef<{ offset: number } | null>(null);
  const collapseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const enterTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /// 已播放入场动画的行 key — 虚拟列表滚动会卸载/重挂载行, 标记后重挂载不再重放
  const enterPlayedRef = useRef<Set<string>>(new Set());

  /// 搜索查询 (label/title 大小写不敏感子串匹配)
  const [searchQuery, setSearchQuery] = useState('');
  const trimmedQuery = searchQuery.trim().toLowerCase();
  const isSearching = trimmedQuery.length > 0;

  const sections = useMemo<PaletteSection[]>(() => {
    const inputItems: PaletteEntry[] = [
      { key: 'Knob', kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob'), title: t(lang, 'knob') },
      { key: 'Button', kind: 'Button', icon: <Square />, label: t(lang, 'button'), title: t(lang, 'button') },
      { key: 'Radio', kind: 'Radio', icon: <RadioIcon />, label: t(lang, 'radio'), title: t(lang, 'radio') },
      { key: 'Checkbox', kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox'), title: t(lang, 'checkbox') },
      { key: 'Slider', kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider'), title: t(lang, 'slider') },
      { key: 'Command', kind: 'Command', icon: <Send size={14} />, label: t(lang, 'command'), title: t(lang, 'command') },
      { key: 'FrameDecoder', kind: 'FrameDecoder', icon: <ScanText size={14} />, label: t(lang, 'frameDecoder'), title: t(lang, 'frameDecoder') },
      { key: 'Trigger', kind: 'Trigger', icon: <Zap size={14} />, label: t(lang, 'trigger'), title: t(lang, 'trigger') },
      { key: 'TextInput', kind: 'TextInput', icon: <TextCursorInput size={14} />, label: t(lang, 'textInput'), title: t(lang, 'textInput') },
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
      /// TextDisplay — 字符串展示控件 (与 Trigger.text 端口连接)
      { key: 'TextDisplay', kind: 'TextDisplay', icon: <FileText size={14} />, label: t(lang, 'textDisplay'), title: t(lang, 'textDisplay') },
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

    /// 字符串操作子项 — 每种 op 一个快捷入口 (端口表见 STR_OP_PORTS)
    const strItems: PaletteEntry[] = [
      { key: 'str-len', kind: 'Str', op: 'len', icon: <Ruler size={14} />, label: t(lang, 'strLen'), title: `${t(lang, 'strLen')} — ${t(lang, 'strLenDesc')}` },
      { key: 'str-find', kind: 'Str', op: 'find', icon: <TextSearch size={14} />, label: t(lang, 'strFind'), title: `${t(lang, 'strFind')} — ${t(lang, 'strFindDesc')}` },
      { key: 'str-contains', kind: 'Str', op: 'contains', icon: <Search size={14} />, label: t(lang, 'strContains'), title: `${t(lang, 'strContains')} — ${t(lang, 'strContainsDesc')}` },
      { key: 'str-left', kind: 'Str', op: 'left', icon: <ArrowLeftToLine size={14} />, label: t(lang, 'strLeft'), title: `${t(lang, 'strLeft')} — ${t(lang, 'strLeftDesc')}` },
      { key: 'str-right', kind: 'Str', op: 'right', icon: <ArrowRightToLine size={14} />, label: t(lang, 'strRight'), title: `${t(lang, 'strRight')} — ${t(lang, 'strRightDesc')}` },
      { key: 'str-mid', kind: 'Str', op: 'mid', icon: <Scissors size={14} />, label: t(lang, 'strMid'), title: `${t(lang, 'strMid')} — ${t(lang, 'strMidDesc')}` },
      { key: 'str-concat', kind: 'Str', op: 'concat', icon: <Link2 size={14} />, label: t(lang, 'strConcat'), title: `${t(lang, 'strConcat')} — ${t(lang, 'strConcatDesc')}` },
      { key: 'str-insert', kind: 'Str', op: 'insert', icon: <TextCursorInput size={14} />, label: t(lang, 'strInsert'), title: `${t(lang, 'strInsert')} — ${t(lang, 'strInsertDesc')}` },
      { key: 'str-delete', kind: 'Str', op: 'delete', icon: <Delete size={14} />, label: t(lang, 'strDelete'), title: `${t(lang, 'strDelete')} — ${t(lang, 'strDeleteDesc')}` },
      { key: 'str-replace', kind: 'Str', op: 'replace', icon: <Replace size={14} />, label: t(lang, 'strReplace'), title: `${t(lang, 'strReplace')} — ${t(lang, 'strReplaceDesc')}` },
      { key: 'str-upper', kind: 'Str', op: 'upper', icon: <CaseUpper size={14} />, label: t(lang, 'strUpper'), title: `${t(lang, 'strUpper')} — ${t(lang, 'strUpperDesc')}` },
      { key: 'str-lower', kind: 'Str', op: 'lower', icon: <CaseLower size={14} />, label: t(lang, 'strLower'), title: `${t(lang, 'strLower')} — ${t(lang, 'strLowerDesc')}` },
      { key: 'str-trim', kind: 'Str', op: 'trim', icon: <Eraser size={14} />, label: t(lang, 'strTrim'), title: `${t(lang, 'strTrim')} — ${t(lang, 'strTrimDesc')}` },
      { key: 'str-reverse', kind: 'Str', op: 'reverse', icon: <ArrowLeftRight size={14} />, label: t(lang, 'strReverse'), title: `${t(lang, 'strReverse')} — ${t(lang, 'strReverseDesc')}` },
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

    return [
      { id: 'input', header: t(lang, 'catInput'), category: 'input', entries: inputItems },
      { id: 'transport', header: t(lang, 'dataInterface'), category: 'input', entries: transportItems },
      { id: 'protocol', header: t(lang, 'protocolEngine'), category: 'input', entries: protocolItems },
      { id: 'display', header: t(lang, 'catDisplay'), category: 'display', entries: displayItems },
      { id: 'math', header: t(lang, 'catMath'), category: 'math', entries: mathItems },
      { id: 'filter', header: t(lang, 'filter'), category: 'math', entries: filterItems },
      { id: 'fft', header: t(lang, 'fft'), category: 'math', entries: fftItems },
      { id: 'string', header: t(lang, 'catString'), category: 'string', entries: strItems },
      { id: 'custom', header: t(lang, 'catCustom'), category: 'custom', entries: customItems },
    ];
  }, [lang, openCustomEditor]);

  /// 顶部跳转条 — 算术/滤波器/频域合并为一个「算术」跳转入口
  const jumpTargets = useMemo<JumpTarget[]>(
    () => [
      { id: 'input', label: t(lang, 'catInput'), color: WIDGET_CATEGORY_COLORS.input, icon: <Sliders size={14} /> },
      { id: 'transport', label: t(lang, 'dataInterface'), color: WIDGET_CATEGORY_COLORS.input, icon: <Cable size={14} /> },
      { id: 'protocol', label: t(lang, 'protocolEngine'), color: WIDGET_CATEGORY_COLORS.input, icon: <Binary size={14} /> },
      { id: 'display', label: t(lang, 'catDisplay'), color: WIDGET_CATEGORY_COLORS.display, icon: <LineChart size={14} /> },
      { id: 'math', label: t(lang, 'catMath'), color: WIDGET_CATEGORY_COLORS.math, icon: <Sigma size={14} /> },
      { id: 'string', label: t(lang, 'catString'), color: WIDGET_CATEGORY_COLORS.string, icon: <Type size={14} /> },
      { id: 'custom', label: t(lang, 'catCustom'), color: WIDGET_CATEGORY_COLORS.custom, icon: <Code2 size={14} /> },
    ],
    [lang],
  );

  /// 搜索过滤后的 sections — 搜索时清空分组内的非匹配条目, 空分组整体剔除;
  /// 非搜索态直接返回原 sections, 保留折叠/展开状态
  const filteredSections = useMemo<PaletteSection[]>(
    () => filterSections(sections, isSearching ? trimmedQuery : ''),
    [sections, isSearching, trimmedQuery],
  );

  /// 扁平条目模型 — 退场动画期间该行仍保留 (折叠状态临时视为展开);
  /// 搜索态强制全部展开 (无折叠意义)
  const items = useMemo(
    () =>
      flattenSections(
        filteredSections,
        isSearching ? {} : collapsing ? { ...collapsed, [collapsing]: false } : collapsed,
      ),
    [filteredSections, isSearching, collapsed, collapsing],
  );

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => listRef.current,
    /// 固定行高: estimateSize 按条目类型返回常量, 跳过 measureElement 的 DOM 测量开销
    estimateSize: (index) => (items[index]?.type === 'header' ? HEADER_SIZE : ROW_SIZE),
    overscan: 6,
    getItemKey: (index) => items[index]?.key ?? index,
  });

  const virtualItems = virtualizer.getVirtualItems();

  /// 各分组的像素锚点 — 与虚拟列表的固定行高估算同源, 滚动推导与跳转落点都以此为准
  const anchors = useMemo(() => sectionAnchors(items), [items]);

  useEffect(
    () => () => {
      if (collapseTimer.current) clearTimeout(collapseTimer.current);
      if (enterTimer.current) clearTimeout(enterTimer.current);
    },
    [],
  );

  /// 分组展开后的入场动画窗口 — 开启新一轮时重置已播放标记
  const enterSection = (id: SectionId) => {
    enterPlayedRef.current.clear();
    setEnteringSection(id);
    if (enterTimer.current) clearTimeout(enterTimer.current);
    enterTimer.current = setTimeout(() => {
      setEnteringSection(null);
      enterTimer.current = null;
    }, ENTER_MS);
  };

  /// 入场动画已播放标记 (挂载即播, 重挂载不重放)
  const markEnterPlayed = useCallback((key: string) => {
    enterPlayedRef.current.add(key);
  }, []);

  /// 滚动驱动高亮同步 — 取 header 偏移不超过滚动位置 (留余量) 的最后一个分组。
  /// 跳转进行中高亮锁定在点击目标; 滚动到位 (含触底截断) 的当帧结束跳转并交还推导
  const handleScroll = () => {
    const el = listRef.current;
    if (!el) return;
    if (jumpRef.current) {
      if (Math.abs(el.scrollTop - jumpRef.current.offset) > 1) return;
      jumpRef.current = null;
    }
    const id = sectionAtScroll(anchors, el.scrollTop);
    setActiveSection((prev) => (prev === id ? prev : id));
  };

  /// 用户接管 — 滚轮/触摸即取消跳转锁定, 高亮交还实时滚动位置
  const cancelJump = () => {
    jumpRef.current = null;
  };

  /// 折叠/展开分组 — 折叠先播退场动画再剔除行, 展开行播入场动画
  const toggleSection = useCallback(
    (id: SectionId) => {
      jumpRef.current = null;
      if (collapsing === id) return;
      if (collapsed[id]) {
        setCollapsed((c) => ({ ...c, [id]: false }));
        enterSection(id);
        return;
      }
      setCollapsing(id);
      collapseTimer.current = setTimeout(() => {
        setCollapsed((c) => ({ ...c, [id]: true }));
        setCollapsing(null);
        collapseTimer.current = null;
      }, EXIT_MS);
    },
    [collapsed, collapsing],
  );

  /// 跳转到分组 — 折叠时先展开, 再平滑滚动到其 header。
  /// 目标偏移截断到可滚动范围: 底部分组到不了顶部时落点即最大滚动位置,
  /// 到位判定因此始终精确, 不会锁死
  const jumpTo = (id: SectionId) => {
    const el = listRef.current;
    if (!el) return;
    /// 目标分组正在播退场动画: 取消折叠, 避免跳转落点后行被剔除
    if (collapsing === id) {
      if (collapseTimer.current) clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
      setCollapsing(null);
    }
    let nextItems = items;
    if (collapsed[id]) {
      setCollapsing(null);
      setCollapsed({ ...collapsed, [id]: false });
      enterSection(id);
      nextItems = flattenSections(sections, { ...collapsed, [id]: false });
    }
    setActiveSection(id);
    const anchor = sectionAnchors(nextItems).find((a) => a.id === id);
    if (!anchor) return;
    const offset = Math.min(anchor.offset, Math.max(0, totalSizeOf(nextItems) - el.clientHeight));
    /// 已在目标位置: 无需滚动, 高亮已就位
    if (Math.abs(el.scrollTop - offset) <= 1) return;
    jumpRef.current = { offset };
    /// 下一帧执行: 若分组刚展开, 需等虚拟列表拿到新模型 (撑开滚动高度) 后再滚动
    requestAnimationFrame(() => {
      virtualizer.scrollToOffset(offset, { behavior: 'smooth' });
    });
  };

  /// 点击/落放添加控件 — 全局节点 (数据接口/协议引擎) 与画布控件分流
  const handleActivate = useCallback(
    (item: PaletteEntry) => {
      if (item.onAdd) {
        item.onAdd();
        return;
      }
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
      const widget = createWidget(kind);
      // 算术控件: 应用所选 op
      if (kind === 'Math' && item.op) {
        const mathWidget = widget as Extract<WidgetConfig, { kind: 'Math' }>;
        mathWidget.params.op = item.op as MathOp;
        if (UNARY_MATH_OPS.includes(item.op as MathOp)) {
          mathWidget.params.inputCount = 1;
        }
        mathWidget.params.label = `Math ${item.op}`;
      }
      // 字符串控件: 应用所选 op
      if (kind === 'Str' && item.op) {
        const strWidget = widget as Extract<WidgetConfig, { kind: 'Str' }>;
        strWidget.params.op = item.op as StrOp;
        strWidget.params.label = `Str ${item.op}`;
      }
      // 滤波器控件: 应用所选 preset
      if (kind === 'Filter' && item.preset) {
        const filterWidget = widget as Extract<WidgetConfig, { kind: 'Filter' }>;
        filterWidget.params.preset = item.preset;
        filterWidget.params.label = `Filter ${item.preset}`;
      }
      addWidget(widget, activeControlTabId, { x: 280, y: 80 + Math.random() * 100 });
    },
    [addWidget, addTransportNode, addProtocolNode, activeControlTabId],
  );

  /// 跳转条高亮归属 — 滤波器/频域归入「算术」入口
  const jumpActive: SectionId =
    activeSection === 'filter' || activeSection === 'fft' ? 'math' : activeSection;

  return (
    <div className="flex flex-col h-full overflow-hidden gap-1.5">
      <JumpBar targets={jumpTargets} activeId={jumpActive} onJump={jumpTo} />

      {/* 搜索框 — 按 label/title 子串过滤, 命中项所在分组自动展开 (搜索态禁用折叠) */}
      <div className="relative flex-shrink-0 px-1">
        <Search size={11} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-disabled pointer-events-none" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder={t(lang, 'paletteSearchPlaceholder')}
          className="w-full pl-6 pr-6 py-1 text-xs bg-bg-input text-text-primary border border-border rounded-sm focus:outline-none focus:border-accent placeholder:text-text-disabled"
        />
        {searchQuery && (
          <button
            className="absolute right-2 top-1/2 -translate-y-1/2 text-text-disabled hover:text-text-primary"
            onClick={() => setSearchQuery('')}
            title={t(lang, 'paletteClearSearch')}
          >
            <X size={11} />
          </button>
        )}
      </div>

      {/* 虚拟滚动列表 — 扁平 header/row 模型, 固定行高 */}
      <div
        ref={listRef}
        onScroll={handleScroll}
        onWheel={cancelJump}
        onTouchStart={cancelJump}
        className="flex-1 min-h-0 overflow-y-auto"
      >
        {items.length === 0 && isSearching ? (
          <div className="flex items-center justify-center h-full text-xs text-text-secondary italic px-2 text-center">
            {t(lang, 'paletteNoResults')}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            <div
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualItems[0]?.start ?? 0}px)`,
              }}
            >
              {virtualItems.map((vi) => {
                const item = items[vi.index];
                if (!item) return null;
                return item.type === 'header' ? (
                  <SectionHeader
                    key={vi.key}
                    header={item.header}
                    collapsed={isSearching ? false : (collapsed[item.sectionId] ?? false) || collapsing === item.sectionId}
                    onToggle={() => toggleSection(item.sectionId)}
                  />
                ) : (
                  <PaletteRow
                    key={vi.key}
                    entry={item.entry}
                    category={item.category}
                    entering={enteringSection === item.sectionId && !enterPlayedRef.current.has(item.key)}
                    exiting={collapsing === item.sectionId}
                    onActivate={handleActivate}
                    onEnterPlayed={markEnterPlayed}
                  />
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
