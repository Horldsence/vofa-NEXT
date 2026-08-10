/// 单条曲线渲染方式选择器 (lineMode + pointMode) - 供 AxisSettings 各通道行复用
import { t, type Lang } from '../../i18n';
import { LINE_MODE_OPTIONS, POINT_MODE_OPTIONS } from './waveformRender';
import { DEFAULT_RENDER, type LineMode, type PointMode, type SeriesRender } from '../../types';

interface Props {
  /// 当前渲染配置 (undefined 时回退 DEFAULT_RENDER)
  render?: SeriesRender;
  onChange: (next: SeriesRender) => void;
  lang: Lang;
  /// 紧凑模式 (All Tab 通道行), 否则单通道 Tab 用稍大控件
  compact?: boolean;
}

export function CurveRenderControls({ render, onChange, lang, compact = false }: Props) {
  const r: SeriesRender = render ?? DEFAULT_RENDER;
  const selectCls = compact
    ? 'flex-1 min-w-0 h-6 text-[10px] px-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors'
    : 'form-select text-xs flex-1 min-w-0';
  return (
    <div className="flex items-center gap-1">
      <span className="text-[10px] text-text-secondary min-w-[24px]">{t(lang, 'curve')}</span>
      <select
        className={selectCls}
        value={r.lineMode}
        onChange={(e) => onChange({ ...r, lineMode: e.target.value as LineMode })}
        title={t(lang, 'lineMode')}
      >
        {LINE_MODE_OPTIONS.map((m) => (
          <option key={m} value={m}>{t(lang, 'lineMode_' + m)}</option>
        ))}
      </select>
      <select
        className={selectCls}
        value={r.pointMode}
        onChange={(e) => onChange({ ...r, pointMode: e.target.value as PointMode })}
        title={t(lang, 'pointMode')}
      >
        {POINT_MODE_OPTIONS.map((m) => (
          <option key={m} value={m}>{t(lang, 'pointMode_' + m)}</option>
        ))}
      </select>
    </div>
  );
}
