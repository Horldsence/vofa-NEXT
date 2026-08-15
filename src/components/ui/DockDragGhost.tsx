import { useEffect, useState } from 'react';
import { subscribeGhost, type GhostState } from '../../lib/dockDrag';

/// 拖拽幽灵 — 跟随指针的半透明标签, 替代 HTML5 DnD 的拖拽快照
/// pointer-events: none — 不参与命中测试, 不遮挡下方投放区
export function DockDragGhost() {
  const [ghost, setGhost] = useState<GhostState | null>(null);

  useEffect(() => subscribeGhost(setGhost), []);

  if (!ghost) return null;
  return (
    <div
      aria-hidden
      className="fixed z-[150] pointer-events-none flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-bg-tooltip border border-border text-text-primary text-xs shadow-lg whitespace-nowrap opacity-90"
      style={{ left: ghost.x + 12, top: ghost.y + 14 }}
    >
      {ghost.label}
    </div>
  );
}
