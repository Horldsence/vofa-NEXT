import { useMemo } from 'react';
import { VERTICAL_DIVS } from '../../lib/scopeUtils';
import { getEffectiveChannel } from '../../types';
import { CURSOR_COLOR } from './waveformConstants';
import { getThemeColor } from './wavechartFormatters';
import type { ScopeAxisConfig } from '../../types';

interface CursorOverlayProps {
  cursors: ScopeAxisConfig['cursors'];
  running: boolean;
  hPosition: number;
  timeWindowSec: number;
  connectedChannels: number[];
  sharedY: boolean;
  channels: ScopeAxisConfig['channels'];
}

/// 游标 SVG 叠加层 — 在波形图上绘制垂直或水平游标线
export function CursorOverlay({
  cursors, running, hPosition, timeWindowSec,
  connectedChannels, sharedY, channels,
}: CursorOverlayProps) {
  const cursorColor = getThemeColor('--color-waveform-cursor', CURSOR_COLOR);

  return useMemo(() => {
    if (cursors.type === 'vertical') {
      const viewEnd = running ? 0 : -hPosition;
      const viewStart = viewEnd - timeWindowSec;
      const range = timeWindowSec || 1;
      const c1R = (cursors.c1 - viewStart) / range;
      const c2R = (cursors.c2 - viewStart) / range;
      return (
        <>
          <line x1={`${c1R * 100}%`} y1="0" x2={`${c1R * 100}%`} y2="100%" stroke={cursorColor} strokeWidth={1} strokeDasharray="4 2" />
          <line x1={`${c2R * 100}%`} y1="0" x2={`${c2R * 100}%`} y2="100%" stroke={cursorColor} strokeWidth={1} strokeDasharray="4 2" />
        </>
      );
    }

    const chIdx = connectedChannels[0] ?? 0;
    const chCfg = getEffectiveChannel({ channels, sharedY, running, hPosition, timeBase: 0, cursors, yUnit: '', grid: false }, chIdx);
    const vPerDiv = chCfg.vPerDiv;
    const pos = chCfg.position;
    let c1R: number, c2R: number;
    if (sharedY) {
      const yMin = pos - (vPerDiv * VERTICAL_DIVS) / 2;
      const yMax = pos + (vPerDiv * VERTICAL_DIVS) / 2;
      const yRange = yMax - yMin || 1;
      c1R = 1 - (cursors.c1 - yMin) / yRange;
      c2R = 1 - (cursors.c2 - yMin) / yRange;
    } else {
      const c1Div = (cursors.c1 - pos) / vPerDiv;
      const c2Div = (cursors.c2 - pos) / vPerDiv;
      c1R = 1 - (c1Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
      c2R = 1 - (c2Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
    }
    return (
      <>
        <line x1="0" y1={`${c1R * 100}%`} x2="100%" y2={`${c1R * 100}%`} stroke={cursorColor} strokeWidth={1} strokeDasharray="4 2" />
        <line x1="0" y1={`${c2R * 100}%`} x2="100%" y2={`${c2R * 100}%`} stroke={cursorColor} strokeWidth={1} strokeDasharray="4 2" />
      </>
    );
  }, [cursors, running, hPosition, timeWindowSec, connectedChannels, sharedY, channels, cursorColor]);
}
