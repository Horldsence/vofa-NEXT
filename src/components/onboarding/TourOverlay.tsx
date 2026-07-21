//! 分步高亮引导遮罩
//!
//! - 通过 4 个暗色遮挡层包围目标元素，形成聚光灯效果
//! - 提示框固定显示在目标右侧（出界时自动调整到左侧）
//! - 背景遮挡层拦截点击，仅目标元素和提示按钮可操作
//! - 支持 Prev / Next / Skip，以及步骤指示器

import { useState, useEffect, useMemo, useCallback } from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';

export interface TourStep {
  target?: string;
  titleKey: string;
  contentKey: string;
  prepare?: () => void;
}

interface TourOverlayProps {
  steps: TourStep[];
  isOpen: boolean;
  onComplete: () => void;
  onSkip: () => void;
}

const GAP = 8;
const TOOLBAR_WIDTH = 320;
const TOOLBAR_MARGIN = 16;

export function TourOverlay({ steps, isOpen, onComplete, onSkip }: TourOverlayProps) {
  const lang = useAppStore((s) => s.lang);
  const [index, setIndex] = useState(0);
  const [rect, setRect] = useState<DOMRect | null>(null);

  const current = steps[index];

  const updateRect = useCallback(() => {
    if (!current?.target) {
      setRect(null);
      return;
    }
    const el = document.querySelector(`[data-tour="${current.target}"]`);
    if (el) {
      const r = el.getBoundingClientRect();
      setRect(
        new DOMRect(
          Math.max(0, r.left - GAP),
          Math.max(0, r.top - GAP),
          r.width + GAP * 2,
          r.height + GAP * 2
        )
      );
      el.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
    } else {
      setRect(null);
    }
  }, [current]);

  useEffect(() => {
    if (!isOpen) return;
    current?.prepare?.();
    updateRect();
    const handleResize = () => updateRect();
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [isOpen, current, updateRect]);

  const isFirst = index === 0;
  const isLast = index === steps.length - 1;

  const handleNext = useCallback(() => {
    if (isLast) {
      onComplete();
    } else {
      setIndex((i) => Math.min(steps.length - 1, i + 1));
    }
  }, [isLast, onComplete, steps.length]);

  const handlePrev = useCallback(() => {
    setIndex((i) => Math.max(0, i - 1));
  }, []);

  const tooltipStyle = useMemo(() => {
    if (!rect) {
      return {
        left: '50%',
        top: '50%',
        transform: 'translate(-50%, -50%)',
        maxWidth: TOOLBAR_WIDTH,
      };
    }
    let left = rect.right + TOOLBAR_MARGIN;
    let top = rect.top;
    // 右侧超出视口时放到左侧
    if (left + TOOLBAR_WIDTH > window.innerWidth) {
      left = Math.max(16, rect.left - TOOLBAR_WIDTH - TOOLBAR_MARGIN);
    }
    top = Math.max(16, Math.min(top, window.innerHeight - 200));
    return {
      left,
      top,
      maxWidth: TOOLBAR_WIDTH,
      transform: 'none',
    };
  }, [rect]);

  if (!isOpen || steps.length === 0) return null;

  return (
    <div className="fixed inset-0 z-[9600]" style={{ pointerEvents: 'none' }}>
      {/* 遮挡层：上 */}
      {rect && (
        <div
          className="fixed bg-black/60 transition-all duration-300"
          style={{
            top: 0,
            left: 0,
            right: 0,
            height: rect.top,
            pointerEvents: 'auto',
          }}
        />
      )}
      {/* 遮挡层：下 */}
      {rect && (
        <div
          className="fixed bg-black/60 transition-all duration-300"
          style={{
            bottom: 0,
            left: 0,
            right: 0,
            top: rect.bottom,
            pointerEvents: 'auto',
          }}
        />
      )}
      {/* 遮挡层：左 */}
      {rect && (
        <div
          className="fixed bg-black/60 transition-all duration-300"
          style={{
            top: rect.top,
            left: 0,
            width: rect.left,
            height: rect.height,
            pointerEvents: 'auto',
          }}
        />
      )}
      {/* 遮挡层：右 */}
      {rect && (
        <div
          className="fixed bg-black/60 transition-all duration-300"
          style={{
            top: rect.top,
            right: 0,
            left: rect.right,
            height: rect.height,
            pointerEvents: 'auto',
          }}
        />
      )}

      {/* 目标高亮边框 */}
      {rect && (
        <div
          className="fixed rounded border-2 border-accent shadow-[0_0_0_4px_rgba(59,130,246,0.2)] transition-all duration-300"
          style={{
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
            pointerEvents: 'none',
          }}
        />
      )}

      {/* 提示框 */}
      <div
        className="fixed bg-bg-sidebar border border-border rounded-lg shadow-modal p-4 flex flex-col gap-3 animate-[settings-slide-in_0.2s_ease-out]"
        style={{
          ...tooltipStyle,
          pointerEvents: 'auto',
        }}
      >
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="text-[10px] uppercase tracking-wide text-text-secondary mb-0.5">
              {t(lang, 'tourStep')} {index + 1} / {steps.length}
            </div>
            <h3 className="text-sm font-semibold text-text-primary m-0">
              {t(lang, current.titleKey)}
            </h3>
          </div>
          <button
            className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer flex-shrink-0"
            onClick={onSkip}
            title={t(lang, 'tourSkip')}
          >
            <X size={14} />
          </button>
        </div>

        <p className="m-0 text-xs text-text-secondary leading-relaxed">
          {t(lang, current.contentKey)}
        </p>

        {/* 步骤点 */}
        <div className="flex items-center gap-1.5">
          {steps.map((_, i) => (
            <div
              key={i}
              className={`h-1.5 rounded-full transition-all duration-200 ${
                i === index ? 'w-4 bg-accent' : 'w-1.5 bg-border'
              }`}
            />
          ))}
        </div>

        {/* 按钮 */}
        <div className="flex items-center justify-between gap-2 pt-1">
          <button
            className="px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            onClick={handlePrev}
            disabled={isFirst}
          >
            <ChevronLeft size={12} className="inline -ml-0.5" />
            {t(lang, 'tourPrev')}
          </button>

          <button
            className="px-3 py-1 bg-bg-button text-text-inverse border-none rounded text-xs inline-flex items-center gap-1 transition-colors hover:bg-bg-button-hover cursor-pointer"
            onClick={handleNext}
          >
            {isLast ? t(lang, 'tourFinish') : t(lang, 'tourNext')}
            {!isLast && <ChevronRight size={12} />}
          </button>
        </div>
      </div>
    </div>
  );
}
