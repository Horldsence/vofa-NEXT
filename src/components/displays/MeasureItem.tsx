/// 测量值网格项 (Vpp/Vmin/Vmax/Vavg/Vrms/Freq)
export function MeasureItem({
  label,
  value,
  unit,
  formatter,
}: {
  label: string;
  value: number;
  unit: string;
  formatter?: (v: number) => string;
}) {
  return (
    <div className="measure-item">
      <span className="measure-label">{label}</span>
      <span className="measure-value">
        {formatter ? formatter(value) : value.toFixed(3)}
        <span className="measure-unit">{unit}</span>
      </span>
    </div>
  );
}

/// 频率格式化 (Hz → k/M)
export function formatFreq(hz: number): string {
  if (hz >= 1e6) return (hz / 1e6).toFixed(2) + 'M';
  if (hz >= 1e3) return (hz / 1e3).toFixed(2) + 'k';
  return hz.toFixed(2);
}
