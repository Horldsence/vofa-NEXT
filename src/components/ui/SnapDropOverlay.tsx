import { useState, type DragEvent } from 'react';

export type SnapEdge = 'top' | 'bottom' | 'left' | 'right';

interface SnapDropHandlers {
  onDragOver: (e: DragEvent<HTMLElement>) => void;
  onDragLeave: (e: DragEvent<HTMLElement>) => void;
  onDrop: (e: DragEvent<HTMLElement>) => void;
}

/// 吸附投放 hook — 挂在目标模块根元素上
/// active=false 时完全透传 (不影响模块内既有的拖放, 如控件拖入画布)
/// 边缘 30% 判定为左右吸附区, 其余按上下半区判定
export function useSnapDrop(active: boolean, onDropEdge: (edge: SnapEdge) => void) {
  const [edge, setEdge] = useState<SnapEdge | null>(null);

  const resolveEdge = (e: DragEvent<HTMLElement>): SnapEdge => {
    const r = e.currentTarget.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width;
    const y = (e.clientY - r.top) / r.height;
    if (x < 0.3) return 'left';
    if (x > 0.7) return 'right';
    return y < 0.5 ? 'top' : 'bottom';
  };

  const handlers: SnapDropHandlers = {
    onDragOver: (e) => {
      if (!active) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      setEdge(resolveEdge(e));
    },
    onDragLeave: () => {
      if (!active) return;
      setEdge(null);
    },
    onDrop: (e) => {
      if (!active) return;
      e.preventDefault();
      const dropped = resolveEdge(e);
      setEdge(null);
      onDropEdge(dropped);
    },
  };

  return { edge, handlers };
}
