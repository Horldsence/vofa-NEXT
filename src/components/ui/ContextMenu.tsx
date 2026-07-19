import { useEffect, useLayoutEffect, useRef, useCallback, cloneElement } from 'react';
import { useContextMenuStore } from '../../store/contextMenuStore';
import type { ContextMenuItem, ContextMenuEntry } from '../../types';

const MENU_ITEM_HEIGHT = 26;
const MENU_MIN_WIDTH = 160;

function isSeparator(entry: ContextMenuEntry): entry is { kind: 'separator' } {
  return 'kind' in entry && entry.kind === 'separator';
}

export function ContextMenu() {
  const { visible, x, y, items, close } = useContextMenuStore();
  const menuRef = useRef<HTMLDivElement>(null);
  const positionRef = useRef({ x, y });

  // 记录最近一次请求的坐标，避免尺寸测量期间 state 已变
  positionRef.current = { x, y };

  // 边界检测：确保菜单不超出视口
  useLayoutEffect(() => {
    if (!visible || !menuRef.current) return;
    const menu = menuRef.current;
    const rect = menu.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    let nextX = positionRef.current.x;
    let nextY = positionRef.current.y;

    if (nextX + rect.width > vw - 8) {
      nextX = Math.max(8, nextX - rect.width);
    }
    if (nextY + rect.height > vh - 8) {
      nextY = Math.max(8, nextY - rect.height);
    }

    menu.style.left = `${nextX}px`;
    menu.style.top = `${nextY}px`;
  }, [visible, x, y, items]);

  // 外部点击 / ESC / resize / scroll 关闭
  useEffect(() => {
    if (!visible) return;

    const handlePointer = (e: PointerEvent | MouseEvent) => {
      const target = e.target as Node;
      if (menuRef.current && !menuRef.current.contains(target)) {
        close();
      }
    };

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };

    const handleClose = () => close();

    // 使用 capture 阶段，避免被下层 stopPropagation 漏掉
    window.addEventListener('pointerdown', handlePointer, true);
    window.addEventListener('mousedown', handlePointer, true);
    window.addEventListener('keydown', handleKey, true);
    window.addEventListener('resize', handleClose, true);
    window.addEventListener('scroll', handleClose, true);

    return () => {
      window.removeEventListener('pointerdown', handlePointer, true);
      window.removeEventListener('mousedown', handlePointer, true);
      window.removeEventListener('keydown', handleKey, true);
      window.removeEventListener('resize', handleClose, true);
      window.removeEventListener('scroll', handleClose, true);
    };
  }, [visible, close]);

  const handleItemClick = useCallback(
    (entry: ContextMenuItem) => (e: React.MouseEvent) => {
      e.stopPropagation();
      if (entry.disabled) return;
      entry.onClick();
      close();
    },
    [close]
  );

  if (!visible || items.length === 0) return null;

  return (
    <div
      ref={menuRef}
      className="context-menu"
      style={{
        position: 'fixed',
        left: x,
        top: y,
        minWidth: MENU_MIN_WIDTH,
        zIndex: 'var(--z-context-menu)',
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((entry, idx) => {
        if (isSeparator(entry)) {
          return <div key={`sep-${idx}`} className="context-menu-separator" />;
        }
        const { id, label, icon, disabled, shortcut } = entry;
        return (
          <button
            key={id}
            type="button"
            disabled={disabled}
            className={`context-menu-item ${disabled ? 'disabled' : ''}`}
            style={{ height: MENU_ITEM_HEIGHT }}
            onClick={handleItemClick(entry)}
          >
            <span className="context-menu-icon">
              {icon
                ? cloneElement(icon, {
                    size: 14,
                    className: disabled ? 'text-text-disabled' : 'text-text-secondary',
                  })
                : null}
            </span>
            <span className={`context-menu-label ${disabled ? 'text-text-disabled' : ''}`}>
              {label}
            </span>
            {shortcut && (
              <span className="context-menu-shortcut">{shortcut}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
