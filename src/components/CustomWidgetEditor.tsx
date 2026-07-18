import { useState, useMemo, useEffect } from 'react';
import { X, Play, Save, FileCode, AlertCircle, AlertTriangle, RotateCcw, HelpCircle } from 'lucide-react';
import { CodeEditor } from './CodeEditor';
import { CustomWidget, evalCustomWidgetDef } from './displays/CustomWidget';
import { CustomWidgetHelpModal } from './CustomWidgetHelpModal';
import type { WidgetConfig } from '../types';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';

interface CustomWidgetEditorProps {
  widget: Extract<WidgetConfig, { kind: 'Custom' }>;
  isOpen: boolean;
  onClose: () => void;
  onSave: (next: Extract<WidgetConfig, { kind: 'Custom' }>) => void;
}

/// 默认模板 — 新建 Custom widget 时使用
export const DEFAULT_CUSTOM_CODE = `({
  name: 'MyWidget',
  description: '自定义控件示例',
  inputs: [
    { id: 'value', label: 'Value' }
  ],
  outputs: [],
  settings: [
    { id: 'unit', label: 'Unit', type: 'text', default: 'V' },
    { id: 'color', label: 'Color', type: 'color', default: '#75beff' }
  ],
  onMount: function(ctx) {
    ctx.state.count = 0;
  },
  render: function(ctx) {
    const v = ctx.inputs.value ?? 0;
    const u = ctx.settings.unit || '';
    const c = ctx.settings.color || '#75beff';
    ctx.el.innerHTML =
      '<div style="padding:8px;text-align:center;font-family:sans-serif">' +
        '<div style="font-size:24px;color:' + c + ';font-weight:bold">' +
          Number(v).toFixed(2) +
        '</div>' +
        '<div style="font-size:11px;color:#888">' + u + '</div>' +
      '</div>';
  }
})
`;

const PRESET_TEMPLATES: { name: string; description: string; code: string }[] = [
  {
    name: '基础数值',
    description: '显示一个数值与单位',
    code: DEFAULT_CUSTOM_CODE,
  },
  {
    name: '进度条',
    description: '水平进度条',
    code: `({
  name: 'ProgressBar',
  inputs: [{ id: 'value', label: 'Value' }],
  outputs: [],
  settings: [
    { id: 'min', label: 'Min', type: 'number', default: 0 },
    { id: 'max', label: 'Max', type: 'number', default: 100 },
    { id: 'color', label: 'Color', type: 'color', default: '#89d185' }
  ],
  render: function(ctx) {
    var v = ctx.inputs.value ?? 0;
    var min = Number(ctx.settings.min) || 0;
    var max = Number(ctx.settings.max) || 100;
    var color = ctx.settings.color || '#89d185';
    var ratio = Math.max(0, Math.min(1, (v - min) / (max - min || 1)));
    ctx.el.innerHTML =
      '<div style="padding:6px;font-family:sans-serif">' +
        '<div style="font-size:11px;color:#888;margin-bottom:4px">' + Number(v).toFixed(2) + '</div>' +
        '<div style="width:100%;height:8px;background:#3c3c3c;border-radius:4px;overflow:hidden">' +
          '<div style="width:' + (ratio * 100) + '%;height:100%;background:' + color + '"></div>' +
        '</div>' +
      '</div>';
  }
})
`,
  },
  {
    name: '按钮',
    description: '点击发送值',
    code: `({
  name: 'Button',
  inputs: [],
  outputs: [{ id: 'click', label: 'Click' }],
  settings: [
    { id: 'label', label: 'Label', type: 'text', default: 'Send' },
    { id: 'value', label: 'Value', type: 'number', default: 1 }
  ],
  onMount: function(ctx) {
    var label = ctx.settings.label || 'Send';
    var value = Number(ctx.settings.value) || 0;
    ctx.el.innerHTML =
      '<button style="width:100%;padding:8px;background:#0e639c;color:white;border:none;border-radius:4px;cursor:pointer;font-size:12px">' +
        label +
      '</button>';
    var btn = ctx.el.querySelector('button');
    if (btn) {
      btn.addEventListener('click', function() {
        ctx.send('click', value);
        ctx.log('Sent:', value);
      });
    }
  },
  render: function(ctx) {
    // 设置变化时重建按钮
    var btn = ctx.el.querySelector('button');
    if (btn) {
      btn.textContent = ctx.settings.label || 'Send';
    }
  }
})
`,
  },
];

/// 自定义控件编辑器 — CodeMirror 编辑 + 实时预览
export function CustomWidgetEditor({ widget, isOpen, onClose, onSave }: CustomWidgetEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const [code, setCode] = useState(widget.params.code);
  const [label, setLabel] = useState(widget.params.label);
  const [settings, setSettings] = useState(widget.params.settings);
  const [previewKey, setPreviewKey] = useState(0);
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  // 同步外部 widget 变化
  useEffect(() => {
    if (isOpen) {
      setCode(widget.params.code);
      setLabel(widget.params.label);
      setSettings(widget.params.settings);
    }
  }, [widget, isOpen]);

  // 实时校验代码
  const validation = useMemo(() => evalCustomWidgetDef(code), [code]);

  // 预览用的 widget 对象 (基于当前编辑内容)
  const previewWidget: Extract<WidgetConfig, { kind: 'Custom' }> = {
    kind: 'Custom',
    params: {
      id: '__preview__',
      label,
      code,
      settings,
    },
  };

  const handleSave = () => {
    const next: Extract<WidgetConfig, { kind: 'Custom' }> = {
      kind: 'Custom',
      params: {
        id: widget.params.id,
        label: label.trim() || 'Custom',
        code,
        settings,
      },
    };
    onSave(next);
    onClose();
  };

  const handleApplyTemplate = (tmplCode: string) => {
    setCode(tmplCode);
    setPreviewKey((k) => k + 1);
  };

  const handleRebuildPreview = () => {
    setPreviewKey((k) => k + 1);
  };

  if (!isOpen) return null;

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div
        className="custom-widget-editor"
        onClick={(e) => e.stopPropagation()}
        style={{ width: '90vw', height: '85vh', maxWidth: 1200, maxHeight: 800 }}
      >
        <div className="settings-header" style={{ padding: '8px 12px' }}>
          <FileCode size={16} />
          <span>{t(lang, 'customWidgetEditor')}</span>
          <button
            className="btn-icon"
            onClick={() => setIsHelpOpen(true)}
            style={{ marginLeft: 'auto' }}
            title={t(lang, 'helpTitle')}
          >
            <HelpCircle size={14} />
          </button>
          <button className="btn-icon" onClick={onClose} title={t(lang, 'customCancel')}>
            <X size={14} />
          </button>
        </div>

        {/* 性能警告横幅 — 提示用户 Custom JS 性能低于原生 Rust 节点 */}
        <div
          className="custom-perf-warning"
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 8,
            padding: '6px 12px',
            background: 'rgba(204, 153, 0, 0.12)',
            borderBottom: '1px solid rgba(204, 153, 0, 0.3)',
            color: '#cca230',
            fontSize: 11,
            lineHeight: 1.4,
          }}
        >
          <AlertTriangle size={14} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>
            <strong>{t(lang, 'customPerfWarningTitle')}</strong>
            <span style={{ marginLeft: 6, opacity: 0.9 }}>
              {t(lang, 'customPerfWarning')}
            </span>
          </div>
        </div>

        <div className="custom-editor-body">
          {/* 左侧: 代码编辑器 */}
          <div className="custom-editor-left">
            <div className="custom-editor-toolbar">
              <span style={{ color: 'var(--text-secondary)', fontSize: 11 }}>
                {t(lang, 'customWidgetCode')}
              </span>
              <div style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
                <select
                  className="custom-template-select"
                  value=""
                  onChange={(e) => {
                    if (e.target.value) {
                      const tmpl = PRESET_TEMPLATES.find((t) => t.name === e.target.value);
                      if (tmpl) handleApplyTemplate(tmpl.code);
                      e.target.value = '';
                    }
                  }}
                >
                  <option value="">{t(lang, 'customTemplate')}</option>
                  {PRESET_TEMPLATES.map((t) => (
                    <option key={t.name} value={t.name}>
                      {t.name} - {t.description}
                    </option>
                  ))}
                </select>
                <button
                  className="btn-icon"
                  onClick={handleRebuildPreview}
                  title={t(lang, 'customRebuildPreview')}
                >
                  <RotateCcw size={12} />
                </button>
              </div>
            </div>

            <div className="custom-editor-label-row">
              <label style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
                {t(lang, 'label')}
              </label>
              <input
                type="text"
                className="custom-label-input"
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                placeholder="Custom Widget"
              />
            </div>

            <div className="custom-editor-code">
              <CodeEditor value={code} onChange={setCode} height="100%" />
            </div>

            {/* 错误提示 */}
            {validation.error && (
              <div className="custom-editor-error">
                <AlertCircle size={12} />
                <pre style={{ margin: 0, whiteSpace: 'pre-wrap', fontSize: 11 }}>
                  {validation.error}
                </pre>
              </div>
            )}

            {/* 解析出的 schema 信息 */}
            {validation.def && (
              <div className="custom-schema-info">
                <div className="custom-schema-row">
                  <span className="custom-schema-key">{t(lang, 'customInputs')}:</span>
                  <span>{validation.def.inputs?.map((i) => i.label).join(', ') || '-'}</span>
                </div>
                <div className="custom-schema-row">
                  <span className="custom-schema-key">{t(lang, 'customOutputs')}:</span>
                  <span>{validation.def.outputs?.map((o) => o.label).join(', ') || '-'}</span>
                </div>
                <div className="custom-schema-row">
                  <span className="custom-schema-key">{t(lang, 'customSettings')}:</span>
                  <span>{validation.def.settings?.map((s) => s.id).join(', ') || '-'}</span>
                </div>
              </div>
            )}
          </div>

          {/* 右侧: 预览 */}
          <div className="custom-editor-right">
            <div className="custom-editor-toolbar">
              <Play size={12} style={{ color: 'var(--green)' }} />
              <span style={{ fontSize: 11 }}>{t(lang, 'customPreview')}</span>
            </div>
            <div className="custom-preview-area">
              {validation.def ? (
                <CustomWidget
                  key={previewKey}
                  widget={previewWidget}
                  onRemove={() => {}}
                  height={200}
                />
              ) : (
                <div className="custom-preview-empty">
                  <AlertCircle size={24} style={{ opacity: 0.4 }} />
                  <span>{t(lang, 'customPreviewUnavailable')}</span>
                </div>
              )}
            </div>

            {/* 设置项编辑 */}
            {validation.def?.settings && validation.def.settings.length > 0 && (
              <div className="custom-settings-area">
                <div className="custom-settings-title">{t(lang, 'customSettingsValues')}</div>
                {validation.def.settings.map((s) => (
                  <div key={s.id} className="custom-setting-row">
                    <label>{s.label}</label>
                    {s.type === 'boolean' ? (
                      <input
                        type="checkbox"
                        checked={settings[s.id] === true}
                        onChange={(e) =>
                          setSettings({ ...settings, [s.id]: e.target.checked })
                        }
                      />
                    ) : s.type === 'color' ? (
                      <input
                        type="color"
                        value={String(settings[s.id] ?? s.default)}
                        onChange={(e) =>
                          setSettings({ ...settings, [s.id]: e.target.value })
                        }
                      />
                    ) : (
                      <input
                        type={s.type === 'number' ? 'number' : 'text'}
                        value={String(settings[s.id] ?? s.default)}
                        onChange={(e) =>
                          setSettings({
                            ...settings,
                            [s.id]:
                              s.type === 'number'
                                ? parseFloat(e.target.value) || 0
                                : e.target.value,
                          })
                        }
                      />
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="settings-footer" style={{ padding: '8px 12px', display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button className="btn-secondary" onClick={onClose}>
            {t(lang, 'customCancel')}
          </button>
          <button
            className="btn-primary"
            onClick={handleSave}
            disabled={!validation.def}
            style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}
          >
            <Save size={12} />
            {t(lang, 'customSave')}
          </button>
        </div>
      </div>

      {/* 帮助说明弹窗 */}
      <CustomWidgetHelpModal isOpen={isHelpOpen} onClose={() => setIsHelpOpen(false)} />
    </div>
  );
}
