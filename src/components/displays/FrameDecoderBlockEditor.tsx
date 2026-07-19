import { AlertTriangle } from 'lucide-react';
import type {
  DecoderBlock,
  ChecksumType,
  DecoderChecksumPosition,
  DecoderChecksumCover,
  FieldType,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import {
  CHECKSUM_OPTIONS,
  CHECKSUM_POSITION_OPTIONS,
  CHECKSUM_COVER_OPTIONS,
  FIELD_TYPE_OPTIONS,
} from './frameDecoderShared';

export interface BlockEditorProps {
  block: DecoderBlock;
  updateBlock: (id: string, changes: Partial<DecoderBlock>) => void;
  lang: ReturnType<typeof useAppStore.getState>['lang'];
}

export function BlockEditor({ block, updateBlock, lang }: BlockEditorProps) {
  const inputClass = "w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent";
  const labelClass = "text-[10px] text-text-secondary";
  const selectClass = "w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent";

  return (
    <>
      {/* 通用: label */}
      <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
        <label className={labelClass}>{t(lang, 'cmdBlockLabel')}</label>
        <input
          type="text"
          value={block.label ?? ''}
          onChange={(e) => updateBlock(block.id, { label: e.target.value } as Partial<DecoderBlock>)}
          className={inputClass}
          placeholder={t(lang, 'cmdBlockLabelPlaceholder')}
        />
      </div>

      {/* header / tail: HEX */}
      {(block.type === 'header' || block.type === 'tail') && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>HEX</label>
          <input
            type="text"
            value={block.hex ?? ''}
            onChange={(e) => updateBlock(block.id, { hex: e.target.value } as Partial<DecoderBlock>)}
            className={inputClass}
            placeholder="AA BB"
            spellCheck={false}
          />
        </div>
      )}

      {/* length / id / field: fieldType + portName */}
      {(block.type === 'length' || block.type === 'id' || block.type === 'field') && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockType')}</label>
            <select
              value={block.fieldType ?? 'uint8'}
              onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {FIELD_TYPE_OPTIONS.map((ft) => (
                <option key={ft} value={ft}>{ft}</option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockPortName')}</label>
            <input
              type="text"
              value={block.portName ?? ''}
              onChange={(e) => updateBlock(block.id, { portName: e.target.value } as Partial<DecoderBlock>)}
              className={inputClass}
              placeholder={block.type === 'length' ? 'length' : block.type === 'id' ? 'id_value' : 'field_1'}
              spellCheck={false}
            />
          </div>
        </>
      )}

      {/* length: unit */}
      {block.type === 'length' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdLengthUnit')}</label>
          <select
            value={block.unit ?? 'bytes'}
            onChange={(e) => updateBlock(block.id, { unit: e.target.value as 'bytes' | 'fields' } as Partial<DecoderBlock>)}
            className={selectClass}
          >
            <option value="bytes">{t(lang, 'fdLengthUnitBytes')}</option>
            <option value="fields">{t(lang, 'fdLengthUnitFields')}</option>
          </select>
        </div>
      )}

      {/* field: lengthRef (仅 bytes 类型有意义) */}
      {block.type === 'field' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdLengthRef')}</label>
          <input
            type="text"
            value={block.lengthRef ?? ''}
            onChange={(e) => updateBlock(block.id, { lengthRef: e.target.value || null } as Partial<DecoderBlock>)}
            className={inputClass}
            placeholder={t(lang, 'fdLengthRefPlaceholder')}
            spellCheck={false}
          />
        </div>
      )}

      {/* bitfield: byteOffset / bitOffset / bitLength / isSigned / portName */}
      {block.type === 'bitfield' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdByteOffset')}</label>
            <input
              type="number"
              min={0}
              value={block.byteOffset ?? 0}
              onChange={(e) => updateBlock(block.id, { byteOffset: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdBitOffset')}</label>
            <input
              type="number"
              min={0}
              max={7}
              value={block.bitOffset ?? 0}
              onChange={(e) => updateBlock(block.id, { bitOffset: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdBitLength')}</label>
            <input
              type="number"
              min={1}
              max={32}
              value={block.bitLength ?? 4}
              onChange={(e) => updateBlock(block.id, { bitLength: parseInt(e.target.value, 10) || 1 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdSigned')}</label>
            <button
              className={`px-2 py-0.5 text-xs rounded-sm border ${block.isSigned ? 'bg-bg-button text-text-inverse border-bg-button' : 'bg-bg-input text-text-secondary border-border'}`}
              onClick={() => updateBlock(block.id, { isSigned: !block.isSigned } as Partial<DecoderBlock>)}
            >
              {block.isSigned ? t(lang, 'fdSignedYes') : t(lang, 'fdSignedNo')}
            </button>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockPortName')}</label>
            <input
              type="text"
              value={block.portName ?? ''}
              onChange={(e) => updateBlock(block.id, { portName: e.target.value } as Partial<DecoderBlock>)}
              className={inputClass}
              placeholder="bits_1"
              spellCheck={false}
            />
          </div>
        </>
      )}

      {/* checksum: algorithm / cover / position / customScript */}
      {block.type === 'checksum' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdChecksum')}</label>
            <select
              value={block.algorithm ?? 'sum8'}
              onChange={(e) => updateBlock(block.id, { algorithm: e.target.value as ChecksumType } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdChecksumCover')}</label>
            <select
              value={block.cover ?? 'all_prior'}
              onChange={(e) => updateBlock(block.id, { cover: e.target.value as DecoderChecksumCover } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_COVER_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          {block.cover === 'range' && (
            <>
              <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                <label className={labelClass}>{t(lang, 'fdCoverStart')}</label>
                <input
                  type="number"
                  min={0}
                  value={block.coverStart ?? 0}
                  onChange={(e) => updateBlock(block.id, { coverStart: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
                  className={inputClass}
                />
              </div>
              <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                <label className={labelClass}>{t(lang, 'fdCoverEnd')}</label>
                <input
                  type="number"
                  min={0}
                  value={block.coverEnd ?? 0}
                  onChange={(e) => updateBlock(block.id, { coverEnd: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
                  className={inputClass}
                />
              </div>
            </>
          )}
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdChecksumPosition')}</label>
            <select
              value={block.position ?? 'append'}
              onChange={(e) => updateBlock(block.id, { position: e.target.value as DecoderChecksumPosition } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_POSITION_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          {block.algorithm === 'custom' && (
            <div className="flex flex-col gap-1">
              <label className={labelClass}>{t(lang, 'cmdCustomScript')}</label>
              <textarea
                className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-1.5 py-1 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
                value={block.customScript ?? ''}
                onChange={(e) => updateBlock(block.id, { customScript: e.target.value } as Partial<DecoderBlock>)}
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
        </>
      )}

      {/* matchId (除 id 块外都可设置) */}
      {block.type !== 'id' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdMatchId')}</label>
          <input
            type="number"
            value={block.matchId ?? ''}
            onChange={(e) => {
              const v = e.target.value === '' ? null : parseInt(e.target.value, 10);
              updateBlock(block.id, { matchId: v } as Partial<DecoderBlock>);
            }}
            className={inputClass}
            placeholder={t(lang, 'fdMatchIdPlaceholder')}
          />
        </div>
      )}
    </>
  );
}
