import { useState, useMemo, useRef } from 'react';
import {
  Send,
  Plus,
  Trash2,
  AlertTriangle,
  Hexagon,
  Type as TypeIcon,
  Braces,
  List,
  ChevronDown,
  ChevronUp,
} from 'lucide-react';
import type {
  WidgetConfig,
  CommandFormat,
  ChecksumType,
  ChecksumPosition,
  FieldType,
  CommandField,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { computeChecksum, type ChecksumKind } from '../../lib/checksum';
import {
  parseHex,
  parseAscii,
  parseTemplate,
  parseStructured,
  bytesToHex,
  bytesToAscii,
} from '../../lib/commandParser';
import { t } from '../../i18n';
import { nanoid } from 'nanoid';

interface CommandSenderProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
  onRemove: () => void;
}

const FORMAT_OPTIONS: {
  value: CommandFormat;
  labelKey: string;
  icon: React.ReactNode;
}[] = [
  { value: 'hex', labelKey: 'cmdFormatHex', icon: <Hexagon size={12} /> },
  { value: 'ascii', labelKey: 'cmdFormatAscii', icon: <TypeIcon size={12} /> },
  { value: 'template', labelKey: 'cmdFormatTemplate', icon: <Braces size={12} /> },
  { value: 'structured', labelKey: 'cmdFormatStructured', icon: <List size={12} /> },
];

const CHECKSUM_OPTIONS: { value: ChecksumType; labelKey: string }[] = [
  { value: 'none', labelKey: 'cmdChecksumNone' },
  { value: 'sum8', labelKey: 'cmdChecksumSum8' },
  { value: 'xor8', labelKey: 'cmdChecksumXor8' },
  { value: 'crc8', labelKey: 'cmdChecksumCRC8' },
  { value: 'crc16Modbus', labelKey: 'cmdChecksumCRC16Modbus' },
  { value: 'crc16CCITT', labelKey: 'cmdChecksumCRC16CCITT' },
  { value: 'crc32', labelKey: 'cmdChecksumCRC32' },
  { value: 'lrc', labelKey: 'cmdChecksumLRC' },
  { value: 'custom', labelKey: 'cmdChecksumCustom' },
];

const FIELD_TYPE_OPTIONS: { value: FieldType; size: number }[] = [
  { value: 'uint8', size: 1 },
  { value: 'int8', size: 1 },
  { value: 'uint16LE', size: 2 },
  { value: 'uint16BE', size: 2 },
  { value: 'int16LE', size: 2 },
  { value: 'int16BE', size: 2 },
  { value: 'uint32LE', size: 4 },
  { value: 'uint32BE', size: 4 },
  { value: 'int32LE', size: 4 },
  { value: 'int32BE', size: 4 },
  { value: 'float32LE', size: 4 },
  { value: 'float32BE', size: 4 },
  { value: 'bytes', size: -1 },
];

/// 命令发送控件 — 多格式输入 + 校验计算 + 发送到嵌入式设备
///
/// 数据流 (前端纯发送, 后端 transport 透传):
///   1. 用户选择输入格式 (HEX/ASCII/模板/结构化)
///   2. 输入内容 → parseXxx → Uint8Array (payload)
///   3. computeChecksum(payload, checksum, customScript) → 校验字节
///   4. 按 checksumPosition 拼接最终字节流
///   5. 调用 store.sendData(byteArray) → 后端 send_raw → transport
export function CommandSender({ widget, onRemove }: CommandSenderProps) {
  void onRemove;
  const params = widget.params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const sendData = useAppStore((s) => s.sendData);
  const lang = useAppStore((s) => s.lang);
  const widgets = useAppStore((s) => s.widgets);

  // 模板插值的变量来源: 当前 tab 的输入控件 (Knob/Slider/Button/Radio/Checkbox) 当前值
  // 简化: 从 widgets 中查找所有 input widget, 取其 id 和最新值
  const templateVars = useMemo(() => {
    const vars: Record<string, string | number> = {};
    for (const w of widgets) {
      if (w.kind === 'Knob' || w.kind === 'Slider' || w.kind === 'Button' || w.kind === 'Radio' || w.kind === 'Checkbox') {
        // 默认值字段
        let val: number | undefined;
        if (w.kind === 'Knob' || w.kind === 'Slider') val = w.params.default;
        else if (w.kind === 'Button') val = w.params.press_value;
        else if (w.kind === 'Radio') val = w.params.default;
        else if (w.kind === 'Checkbox') val = w.params.default ? w.params.checked_value : w.params.unchecked_value;
        if (val !== undefined) {
          vars[w.params.label || w.params.id] = val;
          vars[w.params.id] = val;
        }
      }
    }
    return vars;
  }, [widgets]);

  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastSent, setLastSent] = useState<string | null>(null);
  const sendCountRef = useRef(0);

  /// 计算最终字节流 (payload + 校验)
  const computedBytes = useMemo<{ bytes: Uint8Array | null; error: string | null }>(() => {
    try {
      let payload: Uint8Array;
      switch (params.format) {
        case 'hex':
          payload = parseHex(params.hexContent);
          break;
        case 'ascii':
          payload = parseAscii(params.asciiContent);
          break;
        case 'template':
          payload = parseTemplate(params.templateContent, templateVars);
          break;
        case 'structured':
          payload = parseStructured(params.fields);
          break;
      }
      // 计算校验
      if (params.checksum === 'none' || params.checksumPosition === 'none') {
        return { bytes: payload, error: null };
      }
      const checksumBytes = computeChecksum(
        payload,
        params.checksum as ChecksumKind,
        params.checksum === 'custom' ? params.customScript : undefined
      );
      // 拼接
      let result: Uint8Array;
      if (params.checksumPosition === 'append') {
        result = new Uint8Array(payload.length + checksumBytes.length);
        result.set(payload, 0);
        result.set(checksumBytes, payload.length);
      } else {
        // prepend
        result = new Uint8Array(payload.length + checksumBytes.length);
        result.set(checksumBytes, 0);
        result.set(payload, checksumBytes.length);
      }
      return { bytes: result, error: null };
    } catch (e) {
      return { bytes: null, error: (e as Error).message };
    }
  }, [params, templateVars]);

  /// 追加 \n
  const finalBytes = useMemo<Uint8Array | null>(() => {
    if (!computedBytes.bytes) return null;
    if (!params.appendNewline) return computedBytes.bytes;
    const result = new Uint8Array(computedBytes.bytes.length + 1);
    result.set(computedBytes.bytes, 0);
    result[computedBytes.bytes.length] = 0x0a;
    return result;
  }, [computedBytes, params.appendNewline]);

  const handleSend = async () => {
    setError(null);
    if (!finalBytes || finalBytes.length === 0) {
      setError(t(lang, 'cmdErrorEmpty'));
      return;
    }
    try {
      await sendData(Array.from(finalBytes));
      sendCountRef.current += 1;
      setLastSent(`${new Date().toLocaleTimeString()} #${sendCountRef.current} [${finalBytes.length}B] ${bytesToHex(finalBytes)}`);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const updateParams = (changes: Partial<typeof params>) => {
    updateWidget(params.id, {
      kind: 'Command',
      params: { ...params, ...changes },
    });
  };

  /// 添加字段
  const addField = () => {
    const newField: CommandField = {
      id: nanoid(6),
      name: `field${params.fields.length + 1}`,
      type: 'uint8',
      value: '0',
    };
    updateParams({ fields: [...params.fields, newField] });
  };

  /// 更新字段
  const updateField = (id: string, changes: Partial<CommandField>) => {
    updateParams({
      fields: params.fields.map((f) => (f.id === id ? { ...f, ...changes } : f)),
    });
  };

  /// 删除字段
  const removeField = (id: string) => {
    updateParams({ fields: params.fields.filter((f) => f.id !== id) });
  };

  return (
    <div className="widget-card command-sender">
      <div className="command-sender-header">
        <span className="command-sender-title">{params.label}</span>
        <button
          className="command-sender-toggle"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          <span>{t(lang, 'cmdSettings')}</span>
        </button>
      </div>

      {/* 格式选择 Tab */}
      <div className="command-sender-format-tabs">
        {FORMAT_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            className={`command-sender-format-tab ${params.format === opt.value ? 'active' : ''}`}
            onClick={() => updateParams({ format: opt.value })}
            title={t(lang, opt.labelKey)}
          >
            {opt.icon}
            <span>{t(lang, opt.labelKey)}</span>
          </button>
        ))}
      </div>

      {/* 输入区 (按格式不同) */}
      <div className="command-sender-input-area">
        {params.format === 'hex' && (
          <input
            type="text"
            className="command-sender-input"
            value={params.hexContent}
            onChange={(e) => updateParams({ hexContent: e.target.value })}
            placeholder="AA 01 02 BB"
            spellCheck={false}
          />
        )}
        {params.format === 'ascii' && (
          <textarea
            className="command-sender-textarea"
            value={params.asciiContent}
            onChange={(e) => updateParams({ asciiContent: e.target.value })}
            placeholder={'HELLO\\n\\t\\xAA'}
            spellCheck={false}
            rows={3}
          />
        )}
        {params.format === 'template' && (
          <>
            <textarea
              className="command-sender-textarea"
              value={params.templateContent}
              onChange={(e) => updateParams({ templateContent: e.target.value })}
              placeholder={'SET ${CH0} ${VALUE}\\n'}
              spellCheck={false}
              rows={3}
            />
            <div className="command-sender-vars">
              <span className="command-sender-vars-label">{t(lang, 'cmdAvailableVars')}:</span>
              {Object.keys(templateVars).length === 0 ? (
                <span className="command-sender-vars-empty">{t(lang, 'cmdNoVars')}</span>
              ) : (
                Object.entries(templateVars).map(([k, v]) => (
                  <span key={k} className="command-sender-var-chip" title={String(v)}>
                    ${`{${k}}`}={String(v)}
                  </span>
                ))
              )}
            </div>
          </>
        )}
        {params.format === 'structured' && (
          <div className="command-sender-fields">
            {params.fields.map((f) => (
              <div key={f.id} className="command-sender-field-row">
                <input
                  type="text"
                  className="command-sender-field-name"
                  value={f.name}
                  onChange={(e) => updateField(f.id, { name: e.target.value })}
                  placeholder="name"
                />
                <select
                  className="command-sender-field-type"
                  value={f.type}
                  onChange={(e) => updateField(f.id, { type: e.target.value as FieldType })}
                >
                  {FIELD_TYPE_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.value}
                    </option>
                  ))}
                </select>
                <input
                  type="text"
                  className="command-sender-field-value"
                  value={f.value}
                  onChange={(e) => updateField(f.id, { value: e.target.value })}
                  placeholder="value"
                />
                <button
                  className="btn-icon command-sender-field-remove"
                  onClick={() => removeField(f.id)}
                  title={t(lang, 'removeWidget')}
                >
                  <Trash2 size={11} />
                </button>
              </div>
            ))}
            <button
              className="command-sender-add-field"
              onClick={addField}
            >
              <Plus size={11} />
              <span>{t(lang, 'cmdAddField')}</span>
            </button>
          </div>
        )}
      </div>

      {/* 预览区 */}
      <div className="command-sender-preview">
        <div className="command-sender-preview-header">
          <span>{t(lang, 'cmdPreview')}</span>
          {finalBytes && (
            <span className="command-sender-preview-length">{finalBytes.length}B</span>
          )}
        </div>
        {computedBytes.error ? (
          <div className="command-sender-error">
            <AlertTriangle size={11} />
            <span>{computedBytes.error}</span>
          </div>
        ) : finalBytes && finalBytes.length > 0 ? (
          <>
            <div className="command-sender-preview-hex">
              {bytesToHex(finalBytes)}
            </div>
            <div className="command-sender-preview-ascii">
              {bytesToAscii(finalBytes)}
            </div>
          </>
        ) : (
          <div className="command-sender-preview-empty">{t(lang, 'cmdPreviewEmpty')}</div>
        )}
      </div>

      {/* 发送按钮 */}
      <div className="command-sender-actions">
        <button
          className="btn command-sender-send"
          onClick={handleSend}
          disabled={!finalBytes || finalBytes.length === 0}
        >
          <Send size={12} />
          <span>{t(lang, 'cmdSend')}</span>
        </button>
        {params.appendNewline && (
          <span className="command-sender-hint">+\\n</span>
        )}
      </div>

      {/* 错误提示 */}
      {error && (
        <div className="command-sender-error">
          <AlertTriangle size={11} />
          <span>{error}</span>
        </div>
      )}

      {/* 最近发送 */}
      {lastSent && (
        <div className="command-sender-last-sent" title={lastSent}>
          <span className="command-sender-last-sent-label">{t(lang, 'cmdLastSent')}:</span>
          <span className="command-sender-last-sent-value">{lastSent}</span>
        </div>
      )}

      {/* 设置面板 (校验配置等) */}
      {showSettings && (
        <div className="command-sender-settings">
          {/* 校验算法 */}
          <div className="command-sender-setting-row">
            <label>{t(lang, 'cmdChecksum')}</label>
            <select
              value={params.checksum}
              onChange={(e) => updateParams({ checksum: e.target.value as ChecksumType })}
            >
              {CHECKSUM_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>

          {/* 校验位置 */}
          {params.checksum !== 'none' && (
            <div className="command-sender-setting-row">
              <label>{t(lang, 'cmdChecksumPosition')}</label>
              <div className="command-sender-btn-group">
                {(['append', 'prepend', 'none'] as ChecksumPosition[]).map((pos) => (
                  <button
                    key={pos}
                    className={`command-sender-btn ${params.checksumPosition === pos ? 'active' : ''}`}
                    onClick={() => updateParams({ checksumPosition: pos })}
                  >
                    {t(lang, `cmdChecksumPos_${pos}`)}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* 自定义校验脚本 */}
          {params.checksum === 'custom' && (
            <div className="command-sender-setting-row command-sender-custom-row">
              <label>{t(lang, 'cmdCustomScript')}</label>
              <textarea
                className="command-sender-script"
                value={params.customScript}
                onChange={(e) => updateParams({ customScript: e.target.value })}
                spellCheck={false}
                rows={6}
                placeholder={'// bytes: 输入字节数组\n// 返回: 校验字节数组\nlet s = 0;\nfor (const b of bytes) s = (s + b) & 0xff;\nreturn [s];'}
              />
              <div className="command-sender-warn">
                <AlertTriangle size={10} />
                <span>{t(lang, 'cmdCustomWarn')}</span>
              </div>
            </div>
          )}

          {/* 追加换行 */}
          <div className="command-sender-setting-row">
            <label>{t(lang, 'cmdAppendNewline')}</label>
            <button
              className={`command-sender-btn ${params.appendNewline ? 'active' : ''}`}
              onClick={() => updateParams({ appendNewline: !params.appendNewline })}
            >
              {params.appendNewline ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
            </button>
          </div>

          {/* Label 编辑 */}
          <div className="command-sender-setting-row">
            <label>{t(lang, 'cmdLabel')}</label>
            <input
              type="text"
              value={params.label}
              onChange={(e) => updateParams({ label: e.target.value })}
            />
          </div>
        </div>
      )}
    </div>
  );
}
