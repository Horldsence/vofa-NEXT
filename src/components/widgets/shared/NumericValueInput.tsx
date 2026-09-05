import { useEffect, useRef, useState } from 'react';
import { formatControlValue, snapControlValue, type NumericControlRange } from '../../../lib/utils/numericControl';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';

interface NumericValueInputProps extends NumericControlRange {
  value: number;
  onPreview: (value: number) => void;
  onCommit: (value: number) => void;
}

export function NumericValueInput({ value, min, max, step, onPreview, onCommit }: NumericValueInputProps) {
  const lang = useAppStore((s) => s.lang);
  const [draft, setDraft] = useState(() => formatControlValue(value, min, step));
  const [invalid, setInvalid] = useState(false);
  const focusedRef = useRef(false);
  const committedValueRef = useRef(value);

  useEffect(() => {
    if (!focusedRef.current) {
      committedValueRef.current = value;
      setDraft(formatControlValue(value, min, step));
      setInvalid(false);
    }
  }, [value, min, step]);

  const commit = () => {
    if (draft.trim() === '') {
      setInvalid(true);
      return;
    }
    const parsed = Number(draft);
    if (!Number.isFinite(parsed)) {
      setInvalid(true);
      return;
    }
    const next = snapControlValue(parsed, { min, max, step });
    setInvalid(false);
    setDraft(formatControlValue(next, min, step));
    if (next !== committedValueRef.current) {
      committedValueRef.current = next;
      onCommit(next);
    }
  };

  return (
    <label className="nodrag nowheel block">
      <input
        type="number"
        className={`form-input nodrag nowheel h-7 text-center font-mono ${invalid ? 'border-red' : ''}`}
        min={min}
        max={max}
        step={step}
        value={draft}
        onFocus={() => { focusedRef.current = true; }}
        onBlur={() => { focusedRef.current = false; commit(); }}
        onChange={(event) => {
          const nextDraft = event.target.value;
          setDraft(nextDraft);
          setInvalid(false);
          if (nextDraft.trim() === '') return;
          const parsed = Number(nextDraft);
          if (Number.isFinite(parsed)) onPreview(snapControlValue(parsed, { min, max, step }));
        }}
        onPointerUp={commit}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === 'Enter') {
            event.preventDefault();
            commit();
          }
        }}
        onKeyUp={(event) => {
          event.stopPropagation();
          if (event.key === 'ArrowUp' || event.key === 'ArrowDown') commit();
        }}
        aria-label={t(lang, 'currentValue')}
        aria-invalid={invalid}
      />
      {invalid && <span className="block mt-1 text-[10px] text-red">{t(lang, 'invalidValue')}</span>}
    </label>
  );
}
