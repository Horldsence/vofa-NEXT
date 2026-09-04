// ============ 属性面板共享表单字段 ============
//
// draft state + blur/Enter 提交 + aria-invalid 红框的统一输入控件。
// 从 WidgetProperties 提取, 供节点属性面板各编辑器复用。

import { useEffect, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';

export function TextField({ value, label, onCommit }: { value: string; label: string; onCommit: (value: string) => void }) {
  const lang = useAppStore((s) => s.lang);
  const [draft, setDraft] = useState(value);
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(value); setInvalid(false); }, [value]);
  const commit = () => {
    const next = draft.trim();
    if (next === '') {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    setDraft(next);
    if (next !== value) onCommit(next);
  };
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input className={`form-input ${invalid ? 'border-red' : ''}`} value={draft}
        onChange={(event) => { setDraft(event.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); commit(); } }}
        aria-invalid={invalid} />
      {invalid && <span className="block mt-1 text-[10px] text-red">{t(lang, 'requiredValue')}</span>}
    </label>
  );
}

export function NumberField({ value, label, onCommit, error }: {
  value: number; label: string; onCommit: (value: number) => boolean; error?: string;
}) {
  const [draft, setDraft] = useState(String(value));
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(String(value)); setInvalid(false); }, [value]);
  const commit = () => {
    if (draft.trim() === '') {
      setInvalid(true);
      return;
    }
    const parsed = Number(draft);
    const ok = Number.isFinite(parsed) && (parsed === value || onCommit(parsed));
    setInvalid(!ok);
    if (ok) setDraft(String(parsed));
  };
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input type="number" className={`form-input font-mono ${invalid ? 'border-red' : ''}`} value={draft}
        onChange={(event) => { setDraft(event.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); commit(); } }}
        aria-invalid={invalid} />
      {invalid && <span className="block mt-1 text-[10px] text-red">{error ?? 'Invalid value'}</span>}
    </label>
  );
}

/// 可空数字输入 — 空 = null (「自适应」语义, 如节点宽高未显式设置)
export function OptionalNumberField({ value, label, placeholder, onCommit }: {
  value: number | null; label: string; placeholder?: string; onCommit: (value: number | null) => void;
}) {
  const [draft, setDraft] = useState(value == null ? '' : String(value));
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(value == null ? '' : String(value)); setInvalid(false); }, [value]);
  const commit = () => {
    const text = draft.trim();
    if (text === '') {
      setInvalid(false);
      if (value != null) onCommit(null);
      return;
    }
    const parsed = Number(text);
    const ok = Number.isFinite(parsed) && parsed >= 0;
    setInvalid(!ok);
    if (!ok) return;
    const next = Math.round(parsed);
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return (
    <label className="block mb-2 flex-1 min-w-0">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input type="number" className={`form-input font-mono ${invalid ? 'border-red' : ''}`} value={draft}
        placeholder={placeholder}
        onChange={(event) => { setDraft(event.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); commit(); } }}
        aria-invalid={invalid} />
    </label>
  );
}

/// 下拉选择 — 受控, 变更即提交 (选项枚举类参数)
export function SelectField({ value, label, options, onChange }: {
  value: string;
  label: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <select className="form-select" value={value}
        onChange={(event) => onChange(event.target.value)}>
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>{opt.label}</option>
        ))}
      </select>
    </label>
  );
}
