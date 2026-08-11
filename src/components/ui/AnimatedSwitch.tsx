import { useEffect, useRef, type ReactNode } from 'react';

interface AnimatedSwitchProps {
  /// 切换标识 — 值变化时播放一次转场动画 (首次挂载不播放)
  switchKey: string;
  /// 有序 key 列表 — 与 axis 配合, 按新旧位置关系决定滑入方向
  /// (横向 Tab 条传 x, 竖向 Tab 列传 y; 向右/下移动时内容从右/下方滑入)
  order?: readonly string[];
  /// 滑动轴向; 缺省时仅淡入淡出 (用于无空间顺序的模式切换)
  axis?: 'x' | 'y';
  children: ReactNode;
  className?: string;
}

const EASE = 'cubic-bezier(0.2, 0, 0, 1)';
const DURATION_MS = 180;
const SLIDE_X = 16;
const SLIDE_Y = 10;

/// 通用切换转场容器
/// 纯 CSS transition (transform + opacity) — 动画由合成器线程接管, 不触发任何
/// React 状态更新 / rAF 逐帧重渲染, 也不依赖 Web Animations API。
/// 在不 remount 子树的前提下播放过渡动画, 保留子组件内部状态 (如 ReactFlow 视口)
export function AnimatedSwitch({ switchKey, order, axis, children, className }: AnimatedSwitchProps) {
  const ref = useRef<HTMLDivElement>(null);
  const prevKey = useRef(switchKey);
  // 最新 order/axis 的 ref, 避免 effect 依赖每次渲染新建的数组
  const navRef = useRef({ order, axis });
  navRef.current = { order, axis };

  useEffect(() => {
    const from = prevKey.current;
    prevKey.current = switchKey;
    if (from === switchKey) return;
    const el = ref.current;
    if (!el) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

    // 默认纯淡入淡出; 有序切换按方向滑入 (与旧 WAAPI keyframes 完全一致的视觉)
    let translate = '';
    let dist = 0;
    const { order: ord, axis: ax } = navRef.current;
    if (ord && ax) {
      const fromIdx = ord.indexOf(from);
      const toIdx = ord.indexOf(switchKey);
      if (fromIdx >= 0 && toIdx >= 0 && fromIdx !== toIdx) {
        const dir = toIdx > fromIdx ? 1 : -1;
        translate = ax === 'x' ? 'translateX' : 'translateY';
        dist = dir * (ax === 'x' ? SLIDE_X : SLIDE_Y);
      }
    }

    // 两阶段 CSS transition: 先无过渡地落到起始态, 强制同步 reflow 后过渡到终态
    const fromTransform = translate ? `${translate}(${dist}px)` : 'none';
    el.style.transition = 'none';
    el.style.opacity = '0';
    el.style.transform = fromTransform;
    void el.offsetWidth; // 让起始态立即生效
    el.style.transition = `opacity ${DURATION_MS}ms ${EASE}, transform ${DURATION_MS}ms ${EASE}`;
    el.style.opacity = '1';
    el.style.transform = 'none';
  }, [switchKey]);

  return (
    <div ref={ref} className={className}>
      {children}
    </div>
  );
}
