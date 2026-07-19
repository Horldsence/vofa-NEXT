import { type ReactNode } from 'react';
import { X, Settings2 } from 'lucide-react';
import clsx from 'clsx';

export interface WidgetCardProps {
  children: ReactNode;
  /** Header label at top (uppercase, text-secondary) */
  label?: string;
  /** Badge text in a colored pill (e.g. filter preset, math op symbol) */
  badge?: string;
  /** Tailwind color name for the badge — controls bg/text/border shades */
  badgeColor?: 'blue' | 'green' | 'orange' | 'purple' | 'red' | 'accent' | 'yellow' | 'indigo';
  /** Show remove (×) button on hover */
  onRemove?: () => void;
  /** Show edit (⚙) button on hover */
  onEdit?: () => void;
  /** Remove the min-w-[140px] constraint */
  noMinWidth?: boolean;
  /** Extra classes applied to the card element */
  className?: string;
}

const BADGE_CLASSES: Record<string, string> = {
  blue: 'bg-blue/20 text-blue border-blue/40',
  green: 'bg-green/20 text-green border-green/40',
  orange: 'bg-orange/20 text-orange border-orange/40',
  purple: 'bg-purple/20 text-purple border-purple/40',
  red: 'bg-red/20 text-red border-red/40',
  accent: 'bg-accent/20 text-accent border-accent/40',
  yellow: 'bg-yellow/20 text-yellow border-yellow/40',
  indigo: 'bg-indigo/20 text-indigo border-indigo/40',
};

/// VSCode-style widget card — the shared container for all control & display widgets
///
/// Provides:
///   1. Standard card container (dark sidebar background + border + rounded)
///   2. Hover-reveal remove (×) and edit (⚙) buttons at top-right
///   3. Optional header label (uppercase, text-secondary)
///   4. Optional colored badge pill
///   5. Children slot for widget-specific content
export function WidgetCard({
  children,
  label,
  badge,
  badgeColor = 'accent',
  onRemove,
  onEdit,
  noMinWidth = false,
  className,
}: WidgetCardProps) {
  return (
    <div
      className={clsx(
        'group bg-bg-sidebar border border-border rounded p-2.5 flex flex-col gap-1.5 relative',
        !noMinWidth && 'min-w-[140px]',
        className,
      )}
    >
      {/* Remove button (×) — top-right corner */}
      {onRemove && (
        <button
          type="button"
          className="absolute top-1 right-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary cursor-pointer z-10"
          onClick={onRemove}
        >
          <X size={12} />
        </button>
      )}

      {/* Edit button (⚙) — sits left of remove when both exist */}
      {onEdit && (
        <button
          type="button"
          className={clsx(
            'absolute top-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary cursor-pointer z-10',
            onRemove ? 'right-7' : 'right-1',
          )}
          onClick={onEdit}
          title="Edit"
        >
          <Settings2 size={11} />
        </button>
      )}

      {/* Header row: badge pill + label text */}
      {(badge || label) && (
        <div className="flex items-center gap-1.5">
          {badge && (
            <span
              className={clsx(
                'px-1.5 py-0.5 rounded-sm text-[10px] font-semibold w-fit border',
                BADGE_CLASSES[badgeColor] ?? BADGE_CLASSES.accent,
              )}
            >
              {badge}
            </span>
          )}
          {label && (
            <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">
              {label}
            </div>
          )}
        </div>
      )}

      {children}
    </div>
  );
}
