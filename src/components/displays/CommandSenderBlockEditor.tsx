import { AlertTriangle } from 'lucide-react';
import type { CommandBlock, FieldType, ChecksumType } from '../../types';
import { t } from '../../i18n';
import type { Lang } from '../../i18n';
import { FIELD_TYPE_OPTIONS, CHECKSUM_OPTIONS } from './commandSenderShared';

interface CommandBlockEditorProps {
  block: CommandBlock;
  updateBlock: (id: string, changes: Partial<CommandBlock>) => void;
  lang: Lang;
  graphInputs: Record<string, number>;
}

/// 块编辑区 (展开时) — 按 block.type 渲染对应编辑器
export function CommandBlockEditor({
  block,
  updateBlock,
  lang,
  graphInputs,
}: CommandBlockEditorProps) {
  return (
    <div className="px-2 pb-2 flex flex-col gap-1.5">
      {/* 通用: label */}
      <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
        <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockLabel')}</label>
        <input
          type="text"
          value={block.label ?? ''}
          onChange={(e) => updateBlock(block.id, { label: e.target.value })}
          className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
          placeholder={t(lang, 'cmdBlockLabelPlaceholder')}
        />
      </div>

      {/* const_hex */}
      {block.type === 'const_hex' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className="text-[10px] text-text-secondary">HEX</label>
          <input
            type="text"
            value={block.hex ?? ''}
            onChange={(e) => updateBlock(block.id, { hex: e.target.value })}
            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
            placeholder="AA 01 02"
            spellCheck={false}
          />
        </div>
      )}

      {/* var_ref */}
      {block.type === 'var_ref' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockPortName')}</label>
            <input
              type="text"
              value={block.portName ?? ''}
              onChange={(e) => updateBlock(block.id, { portName: e.target.value })}
              className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
              placeholder="speed"
              spellCheck={false}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockType')}</label>
            <select
              value={block.fieldType ?? 'uint16LE'}
              onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType })}
              className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
            >
              {FIELD_TYPE_OPTIONS.map((ft) => (
                <option key={ft} value={ft}>{ft}</option>
              ))}
            </select>
          </div>
          <div className="text-[10px] text-text-secondary opacity-70 px-1">
            {t(lang, 'cmdBlockVarRefHint')}: {String(graphInputs[block.portName ?? 'value'] ?? 0)}
          </div>
        </>
      )}

      {/* typed_const */}
      {block.type === 'typed_const' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockType')}</label>
            <select
              value={block.fieldType ?? 'uint8'}
              onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType })}
              className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
            >
              {FIELD_TYPE_OPTIONS.map((ft) => (
                <option key={ft} value={ft}>{ft}</option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockValue')}</label>
            <input
              type="text"
              value={block.value ?? ''}
              onChange={(e) => updateBlock(block.id, { value: e.target.value })}
              className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
              placeholder="0"
              spellCheck={false}
            />
          </div>
        </>
      )}

      {/* checksum */}
      {block.type === 'checksum' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdChecksum')}</label>
            <select
              value={block.checksum ?? 'sum8'}
              onChange={(e) => updateBlock(block.id, { checksum: e.target.value as ChecksumType })}
              className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
            >
              {CHECKSUM_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          {block.checksum === 'custom' && (
            <div className="flex flex-col gap-1">
              <label className="text-[10px] text-text-secondary">{t(lang, 'cmdCustomScript')}</label>
              <textarea
                className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-1.5 py-1 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
                value={block.customScript ?? ''}
                onChange={(e) => updateBlock(block.id, { customScript: e.target.value })}
                spellCheck={false}
                rows={4}
                placeholder={'// bytes: 输入字节数组\n// 返回: 校验字节数组\nlet s = 0;\nfor (const b of bytes) s = (s + b) & 0xff;\nreturn [s];'}
              />
              <div className="flex items-center gap-1 px-1.5 py-0.5 bg-yellow/10 border border-yellow/30 text-yellow text-[10px] rounded-sm">
                <AlertTriangle size={10} />
                <span>{t(lang, 'cmdCustomWarn')}</span>
              </div>
            </div>
          )}
          <div className="text-[10px] text-text-secondary opacity-70 px-1">
            {t(lang, 'cmdBlockChecksumHint')}
          </div>
        </>
      )}
    </div>
  );
}
