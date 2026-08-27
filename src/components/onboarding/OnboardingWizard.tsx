//! 首次使用引导向导 —— 自研 React 引导层
//!
//! - 聚光灯 (TourSpotlight) 与弹窗 (TourPopover) 均为 React 组件: 弹窗直接使用主题
//!   token, 整数 left/top 定位, 文字渲染与主应用完全一致
//! - 遮罩层 pointer-events: none, 高亮区完全可交互 —— 实操步骤由用户真实完成:
//!   点跳转条 / 拖控件建卡 / 连线 / 删卡 / 编译结果表删边 / 切 CAN 子页
//! - 门控检测声明式内联在每个步骤的 gate.spec 里 (store 订阅检测节点/边数量跨越,
//!   点击型用 document 捕获 + closest), 不存在与步骤表分离的索引映射
//! - store 型基线在步骤激活一帧后快照并武装检测, 避开 prepare 触发的
//!   startTransition 渲染窗口; 通过后打勾并延迟自动前进
//! - rAF 循环跟踪锚点矩形: 锚点延迟出现 (transition 渲染)、卡片动画、窗口缩放
//!   均自动收敛; 无锚点步骤 (欢迎页) 全屏遮罩 + 弹窗居中

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { openDataPanelAndReveal } from '../../lib/utils/revealDataTab';
import { t } from '../../i18n';
import { TourSpotlight, type TourRect } from './TourSpotlight';
import { TourPopover, type TourAlign, type TourSide } from './TourPopover';

/// 门控检测规格 — 与所属步骤定义同处声明
type GateSpec =
  /// rfNodes.length 增加 (建卡: 拖拽/单击/快速添加皆可捕获)
  | { kind: 'nodes-increase' }
  /// rfEdges.length 增加 (连线)
  | { kind: 'edges-increase' }
  /// rfNodes.length 减少 (删卡: 头部 ×、Backspace、右键菜单皆可捕获)
  | { kind: 'nodes-decrease' }
  /// rfEdges.length 减少 (编译结果表删边)
  | { kind: 'edges-decrease' }
  /// 点击命中选择器 (跳转条 / CAN 子页签)
  | { kind: 'click-in'; selector: string };

interface WizardStepDef {
  /// data-tour 锚点名; 缺省时弹窗居中 (欢迎页)
  anchor?: string;
  titleKey: string;
  contentKey: string;
  /// 固定弹窗方位 — 全部步骤显式指定, 弹窗位置不随空间试探翻转
  side?: TourSide;
  align?: TourAlign;
  /// 进入步骤的自动演示 (切侧栏视图 / 打开数据面板并聚焦)
  prepare?: () => void;
  /// 实操门控 — 提供即显示「勾选框 + 跳过」行, 完成后自动前进
  gate?: { spec: GateSpec; actionKey: string };
}

/// 高亮环相对锚点的外扩
const RING_PAD = 6;
/// 打勾后到自动前进的停顿 — 让 ✓ 动画被看见
const ADVANCE_DELAY_MS = 600;

export function OnboardingWizard() {
  const isOpen = useOnboardingStore((s) => s.isWizardOpen);
  const close = useOnboardingStore((s) => s.closeWizard);
  const complete = useOnboardingStore((s) => s.completeWizard);
  const setSidebarView = useAppStore((s) => s.setSidebarView);
  const lang = useAppStore((s) => s.lang);

  const [stepIndex, setStepIndex] = useState(0);
  const [gatePassed, setGatePassed] = useState(false);
  const [frame, setFrame] = useState<{ rect: TourRect | null; vw: number; vh: number }>(() => ({
    rect: null,
    vw: window.innerWidth,
    vh: window.innerHeight,
  }));
  const advTimerRef = useRef<number | undefined>(undefined);

  // ---- 步骤定义（完整工作流叙事线）----
  // useMemo 稳定身份: 门控 effect 以 def 为依赖, 重建会令观测/基线每帧重置
  const defs: WizardStepDef[] = useMemo(() => [
    // 1. 欢迎（居中卡片）
    {
      titleKey: 'tourWelcomeTitle',
      contentKey: 'tourWelcomeContent',
    },
    // 2. 快速开始 · 模板起步
    {
      anchor: 'quickstart-panel',
      titleKey: 'tourQuickStartTitle',
      contentKey: 'tourQuickStartContent',
      side: 'right',
      align: 'start',
      prepare: () => setSidebarView('quickstart'),
    },
    // 3. 控件面板 · 九大分组总览
    {
      anchor: 'palette-root',
      titleKey: 'tourPaletteTitle',
      contentKey: 'tourPaletteContent',
      side: 'right',
      align: 'start',
      prepare: () => setSidebarView('widgets'),
    },
    // 4. 控件面板 · 跳转条（动手）
    {
      anchor: 'palette-jumpbar',
      titleKey: 'tourJumpbarTitle',
      contentKey: 'tourJumpbarContent',
      side: 'bottom',
      align: 'center',
      prepare: () => setSidebarView('widgets'),
      gate: {
        spec: { kind: 'click-in', selector: '[data-tour="palette-jumpbar"]' },
        actionKey: 'tourActJumpbar',
      },
    },
    // 5. 新建卡片（动手 · 双区聚光：侧栏+画布同处 tour-workbench 聚光内且均可交互）
    {
      anchor: 'tour-workbench',
      titleKey: 'tourCreateTitle',
      contentKey: 'tourCreateContent',
      side: 'bottom',
      align: 'center',
      prepare: () => setSidebarView('widgets'),
      gate: { spec: { kind: 'nodes-increase' }, actionKey: 'tourActCreate' },
    },
    // 6. 连接边（动手）
    {
      anchor: 'canvas',
      titleKey: 'tourConnectTitle',
      contentKey: 'tourConnectContent',
      side: 'bottom',
      align: 'center',
      gate: { spec: { kind: 'edges-increase' }, actionKey: 'tourActConnect' },
    },
    // 7. 删除卡片（动手）— 气泡挂底部, 画布内节点完全可点
    {
      anchor: 'canvas',
      titleKey: 'tourDeleteCardTitle',
      contentKey: 'tourDeleteCardContent',
      side: 'bottom',
      align: 'center',
      gate: { spec: { kind: 'nodes-decrease' }, actionKey: 'tourActDeleteCard' },
    },
    // 8. 编译报错 · 自动打开错误面板并演示定位能力
    {
      anchor: 'errors-view',
      titleKey: 'tourCompileErrorsTitle',
      contentKey: 'tourCompileErrorsContent',
      side: 'top',
      align: 'start',
      prepare: () => openDataPanelAndReveal(() => useAppStore.getState().addCompileErrorsTab()),
    },
    // 9. 编译结果 · 动手删除一条连接
    {
      anchor: 'results-table',
      titleKey: 'tourResultsTitle',
      contentKey: 'tourResultsContent',
      side: 'top',
      align: 'start',
      prepare: () => openDataPanelAndReveal(() => useAppStore.getState().addCompileResultsTab()),
      gate: { spec: { kind: 'edges-decrease' }, actionKey: 'tourActDeleteEdge' },
    },
    // 10. 数据窗口与布局（旧三步合并）
    {
      anchor: 'data-tabs',
      titleKey: 'tourDataTabsTitle',
      contentKey: 'tourDataTabsContent',
      side: 'top',
      align: 'center',
    },
    // 11. CAN 帧（动手 · 子页签切换, 命中 can-tabs 比整面板更精确）
    {
      anchor: 'can-view',
      titleKey: 'tourCanTitle',
      contentKey: 'tourCanContent',
      side: 'top',
      align: 'start',
      prepare: () => openDataPanelAndReveal(() => useAppStore.getState().addCanTab()),
      gate: {
        spec: { kind: 'click-in', selector: '[data-tour="can-tabs"]' },
        actionKey: 'tourActCanTab',
      },
    },
    // 12. 帮助中心收尾
    {
      anchor: 'help',
      titleKey: 'tourHelpTitle',
      contentKey: 'tourHelpContent',
      side: 'right',
      align: 'end',
    },
  ], [setSidebarView]);

  const isLast = stepIndex === defs.length - 1;
  const def = defs[stepIndex];

  const clearAdvanceTimer = () => {
    if (advTimerRef.current !== undefined) {
      window.clearTimeout(advTimerRef.current);
      advTimerRef.current = undefined;
    }
  };

  const gotoNext = useCallback(() => {
    clearAdvanceTimer();
    setGatePassed(false);
    setStepIndex((i) => Math.min(i + 1, defs.length - 1));
  }, []);

  const gotoPrev = useCallback(() => {
    clearAdvanceTimer();
    setGatePassed(false);
    setStepIndex((i) => Math.max(0, i - 1));
  }, []);

  // ---- 步骤进入: prepare + 门控观测 ----
  useEffect(() => {
    def.prepare?.();
    setGatePassed(false);
    if (!def.gate) return;

    const spec = def.gate.spec;
    let passed = false;
    let disposed = false;
    const disposers: (() => void)[] = [];

    const pass = () => {
      if (passed || disposed) return;
      passed = true;
      setGatePassed(true);
      advTimerRef.current = window.setTimeout(() => {
        advTimerRef.current = undefined;
        gotoNext();
      }, ADVANCE_DELAY_MS);
    };

    if (spec.kind === 'click-in') {
      // document 级捕获 + closest 匹配 — 不依赖锚点元素的解析时机,
      // React 重渲染替换节点也不影响触发
      const onClick = (ev: Event) => {
        if ((ev.target as Element | null)?.closest?.(spec.selector)) pass();
      };
      document.addEventListener('click', onClick, true);
      disposers.push(() => document.removeEventListener('click', onClick, true));
    } else {
      const useNodes = spec.kind.startsWith('nodes');
      const wantIncrease = spec.kind.endsWith('increase');
      const pick = (s: ReturnType<typeof useAppStore.getState>) =>
        useNodes ? s.rfNodes.length : s.rfEdges.length;

      // 基线先取同步初值, 一帧后 (startTransition 渲染窗口结束) 校准并武装检测:
      // 期间到来的程序性变更被吸收进基线, 不会误判为用户操作
      let baseline = pick(useAppStore.getState());
      let armed = false;
      const raf = requestAnimationFrame(() => {
        baseline = pick(useAppStore.getState());
        armed = true;
      });
      disposers.push(() => cancelAnimationFrame(raf));

      const unsub = useAppStore.subscribe((s, prev) => {
        if (!armed) return;
        const v = pick(s);
        if (v === pick(prev)) return;
        if (wantIncrease ? v > baseline : v < baseline) pass();
      });
      disposers.push(unsub);
    }

    return () => {
      disposed = true;
      disposers.forEach((d) => d());
      clearAdvanceTimer();
    };
  }, [stepIndex, def, gotoNext]);

  // ---- 锚点矩形跟踪 (rAF 循环): 延迟出现/动画/缩放自动收敛 ----
  useEffect(() => {
    let raf = 0;
    let lastKey = '';
    let scrolled = false;
    const tick = () => {
      const el = def.anchor ? document.querySelector(`[data-tour="${def.anchor}"]`) : null;
      let rect: TourRect | null = null;
      if (el) {
        const b = el.getBoundingClientRect();
        rect = {
          x: Math.max(0, Math.round(b.left - RING_PAD)),
          y: Math.max(0, Math.round(b.top - RING_PAD)),
          w: Math.round(b.width + RING_PAD * 2),
          h: Math.round(b.height + RING_PAD * 2),
        };
        // 锚点解析到后再滚动就位, 避免对尚未渲染的元素调用 scrollIntoView
        if (!scrolled) {
          scrolled = true;
          el.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
        }
      }
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const key = rect ? `${rect.x},${rect.y},${rect.w},${rect.h}` : 'null';
      if (key !== lastKey) {
        lastKey = key;
        setFrame({ rect, vw, vh });
      } else {
        setFrame((f) => (f.vw === vw && f.vh === vh ? f : { rect: f.rect, vw, vh }));
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [def.anchor]);

  if (!isOpen) return null;

  const tr = (key: string) => t(lang, key);

  return (
    <div className="pointer-events-none fixed inset-0 z-tour">
      <TourSpotlight rect={frame.rect} />
      <TourPopover
        key={stepIndex}
        rect={frame.rect}
        side={def.side ?? 'bottom'}
        align={def.align ?? 'center'}
        viewport={{ w: frame.vw, h: frame.vh }}
        stepIndex={stepIndex}
        totalSteps={defs.length}
        stepLabel={tr('tourStep')}
        prevLabel={tr('tourPrev')}
        nextLabel={tr('tourNext')}
        finishLabel={tr('tourFinish')}
        closeLabel={tr('tourSkip')}
        titleHtml={tr(def.titleKey)}
        contentHtml={tr(def.contentKey)}
        gate={
          def.gate
            ? {
                actionHtml: tr(def.gate.actionKey),
                passed: gatePassed,
                skipLabel: tr('tourNoEdgeSkip'),
              }
            : null
        }
        onSkipGate={gotoNext}
        onPrev={gotoPrev}
        onNext={() => {
          if (isLast) complete();
          else gotoNext();
        }}
        onClose={close}
      />
    </div>
  );
}
