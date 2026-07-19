import { cloneElement, type ReactElement } from 'react';
import clsx from 'clsx';

export interface PanelTab<T extends string> {
  value: T;
  label: string;
  icon: ReactElement<{ size?: number; className?: string }>;
}

interface PanelTabsProps<T extends string> {
  tabs: PanelTab<T>[];
  active: T;
  onChange: (value: T) => void;
}

/// VSCode 风格面板标签切换器 — 紧凑、图标+文字、高对比激活态
export function PanelTabs<T extends string>({ tabs, active, onChange }: PanelTabsProps<T>) {
  return (
    <div className="flex items-center gap-0.5 p-1 border-b border-border bg-bg-panel-header flex-shrink-0">
      {tabs.map((tab) => {
        const isActive = tab.value === active;
        return (
          <button
            key={tab.value}
            type="button"
            onClick={() => onChange(tab.value)}
            className={clsx(
              'flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-all cursor-pointer',
              isActive
                ? 'bg-accent/15 text-text-bright'
                : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary',
            )}
          >
            {cloneElement(tab.icon, { size: 13, className: isActive ? 'text-accent' : 'text-text-secondary' })}
            <span>{tab.label}</span>
          </button>
        );
      })}
    </div>
  );
}
