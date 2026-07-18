import { cloneElement, type ReactElement, type ButtonHTMLAttributes } from 'react';

interface ToolbarIconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: ReactElement<{ size?: number; className?: string }>;
  active?: boolean;
  title?: string;
  size?: 'sm' | 'md';
  variant?: 'default' | 'danger';
}

/// VSCode 风格工具栏图标按钮 — 方形、悬停背景、可选激活态
export function ToolbarIconButton({
  icon,
  active = false,
  title,
  size = 'md',
  variant = 'default',
  className = '',
  ...rest
}: ToolbarIconButtonProps) {
  const dimension = size === 'sm' ? 'w-5 h-5' : 'w-6 h-6';
  const iconSize = size === 'sm' ? 12 : 14;

  const base = 'inline-flex items-center justify-center rounded transition-colors cursor-pointer flex-shrink-0';
  const state = active
    ? variant === 'danger'
      ? 'bg-bg-danger text-red'
      : 'bg-accent/15 text-text-bright'
    : variant === 'danger'
      ? 'text-text-secondary hover:bg-bg-danger hover:text-red'
      : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary';

  return (
    <button
      type="button"
      title={title}
      className={`${base} ${dimension} ${state} ${className}`}
      {...rest}
    >
      {cloneElement(icon, { size: iconSize })}
    </button>
  );
}
