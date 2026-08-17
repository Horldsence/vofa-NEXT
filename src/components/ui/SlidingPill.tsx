import { useLayoutEffect, useRef, useState } from 'react';

export interface PillRect {
  left: number;
  top: number;
  width: number;
  height: number;
  visible: boolean;
}

const HIDDEN: PillRect = { left: 0, top: 0, width: 0, height: 0, visible: false };

/// 滑动指示器 hook — 跟踪容器内 [data-tab-key="activeKey"] 元素的位置/尺寸
/// 容器需 position: relative; 各 Tab 项带 data-tab-key 属性
/// 每次渲染后重新测量 (Tab 重命名/增删/窗口缩放均能跟踪), 仅在矩形变化时 setState
export function useSlidingPill(activeKey: string | null) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [pill, setPill] = useState<PillRect>(HIDDEN);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const update = () => {
      const el =
        activeKey != null
          ? container.querySelector<HTMLElement>(`[data-tab-key="${CSS.escape(activeKey)}"]`)
          : null;
      if (!el) {
        setPill((p) => (p.visible ? HIDDEN : p));
        return;
      }
      const next: PillRect = {
        left: el.offsetLeft,
        top: el.offsetTop,
        width: el.offsetWidth,
        height: el.offsetHeight,
        visible: true,
      };
      setPill((p) =>
        p.visible &&
        p.left === next.left &&
        p.top === next.top &&
        p.width === next.width &&
        p.height === next.height
          ? p
          : next
      );
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(container);
    return () => ro.disconnect();
  });

  return { containerRef, pill };
}

/// 滑块本体 — 绝对定位在容器内, 位置/尺寸变化由 CSS transition 平滑滑行
/// variant: 'editor' (默认, bg-editor) | 'panel' (bg-active, 用于竖排通道 Tab)
export function SlidingPill({ pill, variant = 'editor' }: { pill: PillRect; variant?: 'editor' | 'panel' }) {
  if (!pill.visible) return null;
  // 使用 left/top 定位，transform: scale() 处理尺寸动画（避免 layout）
  // 由于 transition 只能作用于相同属性，需要分两步：
  // 1. 位置变化：left/top 变化 + translateX/Y 动画
  // 2. 尺寸变化：scaleX/scaleY 动画
  // 为简化，使用 scale 配合 width/height 的视觉尺寸
  return (
    <div
      aria-hidden
      className={`tab-sliding-pill ${variant === 'panel' ? 'tab-sliding-pill--panel' : ''}`}
      style={{
        left: pill.left,
        top: pill.top,
        width: pill.width,
        height: pill.height,
      }}
    />
  );
}
