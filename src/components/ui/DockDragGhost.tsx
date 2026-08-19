import { useEffect, useState } from 'react';
import { subscribeGhost, type GhostState } from '../../lib/dockDrag';

/// 拖拽幽灵 — 以指针为中心的半透明标签, 替代 HTML5 DnD 的拖拽快照
/// pointer-events: none — 不参与命中测试, 不遮挡下方投放区
/// 释放时播放放大淡出动画 (releasing); 低动画偏好下 dockDrag 直接清除, 不进入释放态
export function DockDragGhost() {
  const [ghost, setGhost] = useState<GhostState | null>(null);

  useEffect(() => subscribeGhost(setGhost), []);

  if (!ghost) return null;
  const releasing = ghost.releasing ?? false;
  return (
    <div
      aria-hidden
      className="fixed z-[150] pointer-events-none flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-bg-tooltip border border-border text-text-primary text-xs shadow-lg whitespace-nowrap"
      style={{
        left: ghost.x,
        top: ghost.y,
        transform: releasing ? 'translate(-50%, -50%) scale(1.3)' : 'translate(-50%, -50%)',
        opacity: releasing ? 0 : 0.9,
        transition: 'transform 180ms ease-out, opacity 180ms ease-out',
      }}
    >
      {ghost.label}
    </div>
  );
}
