//! 引导弹窗 — 纯 React 展示组件, 视觉与 AboutModal 同一骨架 (全部主题 token)
//!
//! - 定位: 按锚点矩形 + side/align 计算, Math.round 整数 left/top (无 transform
//!   栖息位), 文字栅格化与主应用一致, 不存在库弹窗的亚像素模糊
//! - 文案来自项目自身 i18n yml (含 <strong> 强调), 以受控 HTML 渲染保留加粗
//! - 自身高度经 ref 实测, 用于垂直夹取; 未测得前用估算值

import { useLayoutEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import type { TourRect } from './TourSpotlight';

export type TourSide = 'top' | 'bottom' | 'left' | 'right';
export type TourAlign = 'start' | 'center' | 'end';

const POPOVER_W = 320;
/// 弹窗与高亮环的间距
const GAP = 12;
/// 视口边距
const VIEWPORT_MARGIN = 12;
const ESTIMATED_H = 220;

interface TourPopoverProps {
  rect: TourRect | null;
  side: TourSide;
  align: TourAlign;
  viewport: { w: number; h: number };
  /** 步骤序号 (0-based) */
  stepIndex: number;
  totalSteps: number;
  stepLabel: string;
  prevLabel: string;
  nextLabel: string;
  finishLabel: string;
  closeLabel: string;
  titleHtml: string;
  contentHtml: string;
  gate: { actionHtml: string; passed: boolean; skipLabel: string } | null;
  onSkipGate: () => void;
  onPrev: () => void;
  onNext: () => void;
  onClose: () => void;
}

export function TourPopover(props: TourPopoverProps) {
  const {
    rect,
    side,
    align,
    viewport,
    stepIndex,
    totalSteps,
    stepLabel,
    prevLabel,
    nextLabel,
    finishLabel,
    closeLabel,
    titleHtml,
    contentHtml,
    gate,
    onSkipGate,
    onPrev,
    onNext,
    onClose,
  } = props;

  const bodyRef = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(ESTIMATED_H);
  useLayoutEffect(() => {
    const el = bodyRef.current;
    if (el) setHeight(Math.round(el.offsetHeight));
  }, [titleHtml, contentHtml, gate?.actionHtml, gate?.passed, totalSteps]);

  const isLast = stepIndex === totalSteps - 1;
  const nextDisabled = gate != null && !gate.passed;

  const style = computePosition(rect, side, align, viewport, height);

  return (
    <div
      ref={bodyRef}
      className="fixed w-80 rounded-lg border border-border bg-bg-sidebar shadow-modal p-4 flex flex-col gap-3 animate-[tour-pop-in_0.18s_ease-out]"
      style={{ left: style.left, top: style.top, pointerEvents: 'auto' }}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="text-xs text-text-secondary mb-0.5 select-none">
            {stepLabel} {stepIndex + 1} / {totalSteps}
          </div>
          <h3
            className="m-0 text-sm font-semibold leading-snug text-text-primary [&_strong]:font-semibold"
            dangerouslySetInnerHTML={{ __html: titleHtml }}
          />
        </div>
        <button
          type="button"
          className="flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded text-text-secondary transition-colors hover:bg-bg-hover hover:text-text-primary"
          onClick={onClose}
          title={closeLabel}
          aria-label={closeLabel}
        >
          <X size={14} />
        </button>
      </div>

      <p
        className="m-0 text-[13px] leading-relaxed text-text-primary [&_strong]:font-semibold"
        dangerouslySetInnerHTML={{ __html: contentHtml }}
      />

      {gate && (
        <div className="mt-0.5 flex items-center gap-2 border-t border-dashed border-border-subtle pt-2.5">
          <span
            className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] border-[1.5px] text-[11px] leading-none select-none transition-colors ${
              gate.passed
                ? 'animate-[tour-tick-in_0.22s_cubic-bezier(0.2,1.4,0.5,1)] border-accent bg-accent text-text-inverse'
                : 'border-accent text-transparent'
            }`}
            aria-hidden
          >
            ✓
          </span>
          <span
            className={`min-w-0 flex-1 text-xs leading-snug ${
              gate.passed
                ? 'text-text-secondary line-through decoration-border'
                : 'text-accent'
            }`}
            dangerouslySetInnerHTML={{ __html: gate.actionHtml }}
          />
          {!gate.passed && (
            <button
              type="button"
              className="shrink-0 cursor-pointer bg-transparent border-none p-0 px-1 text-[11px] text-text-secondary underline decoration-dotted underline-offset-[3px] transition-colors hover:text-text-primary"
              onClick={onSkipGate}
            >
              {gate.skipLabel}
            </button>
          )}
        </div>
      )}

      <div className="flex items-center justify-between gap-2 pt-0.5">
        <button
          type="button"
          className="cursor-pointer rounded border border-transparent bg-transparent px-2.5 py-1 text-xs text-text-secondary transition-colors hover:bg-bg-hover hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40"
          onClick={onPrev}
          disabled={stepIndex === 0}
        >
          {prevLabel}
        </button>
        <button
          type="button"
          className="cursor-pointer rounded border border-transparent bg-bg-button px-3 py-1 text-xs text-text-inverse transition-colors hover:bg-bg-button-hover disabled:cursor-not-allowed disabled:opacity-45"
          onClick={onNext}
          disabled={nextDisabled}
        >
          {isLast ? finishLabel : nextLabel}
        </button>
      </div>
    </div>
  );
}

function computePosition(
  rect: TourRect | null,
  side: TourSide,
  align: TourAlign,
  viewport: { w: number; h: number },
  height: number,
): { left: number; top: number } {
  // 无锚点 (欢迎页): 屏幕居中
  if (!rect) {
    return {
      left: Math.round((viewport.w - POPOVER_W) / 2),
      top: Math.round((viewport.h - height) / 2),
    };
  }

  let left: number;
  let top: number;
  const cx = rect.x + rect.w / 2;
  const cy = rect.y + rect.h / 2;

  if (side === 'right' || side === 'left') {
    left = side === 'right' ? rect.x + rect.w + GAP : rect.x - POPOVER_W - GAP;
    top = align === 'start' ? rect.y : align === 'end' ? rect.y + rect.h - height : cy - height / 2;
  } else {
    top = side === 'bottom' ? rect.y + rect.h + GAP : rect.y - height - GAP;
    left = align === 'start' ? rect.x : align === 'end' ? rect.x + rect.w - POPOVER_W : cx - POPOVER_W / 2;
  }

  const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));
  return {
    left: clamp(Math.round(left), VIEWPORT_MARGIN, viewport.w - POPOVER_W - VIEWPORT_MARGIN),
    top: clamp(Math.round(top), VIEWPORT_MARGIN, viewport.h - height - VIEWPORT_MARGIN),
  };
}
