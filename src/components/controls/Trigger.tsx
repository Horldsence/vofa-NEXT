// ============ 触发器 (Trigger) 控件 ============
//
// 维护「命令 → 输出值」对照表, 命中时后端把数字写入 value/matched 输出端口 (f32 平面),
// 字符串规则命中写入 text 输出端口 (字符串平面)。
//
// 求值全部在后端 (图每帧评估, 见 node_engine evaluate.rs 的 NodeKind::Trigger 分支):
// - manual: 每帧以当前 command 匹配 (command 改动经 update_tab_graph 同步即生效)
// - auto:   上游 trigger 端口 (number) 按 edge (level/rising) 由后端边沿检测驱动
//
// 前端只做配置编辑 (规则/mode/edge/default) 与结果展示
// (value/matched 读 graphOutputs[自己的id], text 读 customTextOutputs[自己的id])。

import { useCallback, useState } from 'react';
import { Plus, Trash2, ChevronDown, ChevronRight, Play, Radio } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useNumericInput, useNumericOutput } from '../../lib/hooks/useNumericPort';
import { t } from '../../i18n';
import { nanoid } from 'nanoid';
import type { TriggerConfig, TriggerMatchType, TriggerRule } from '../../types';
import type { WidgetConfig } from '../../types';
import type { Lang } from '../../i18n';

// ============ 常量: 匹配类型配置 ============

const MATCH_TYPE_CONFIG: Record<
  TriggerMatchType,
  { labelKey: string; icon: string; badgeClass: string; placeholder: string; hintKey: string }
> = {
  exact:    { labelKey: 'triggerMatchExact',    icon: '=',  badgeClass: 'bg-blue/20 text-blue border-blue/40',           placeholder: 'GET_TEMP', hintKey: 'triggerHintExact'    },
  prefix:   { labelKey: 'triggerMatchPrefix',   icon: '⇆',  badgeClass: 'bg-accent/20 text-accent border-accent/40',     placeholder: 'GET',      hintKey: 'triggerHintPrefix'   },
  contains: { labelKey: 'triggerMatchContains', icon: '⊃',  badgeClass: 'bg-purple/20 text-purple border-purple/40',     placeholder: 'TEMP',     hintKey: 'triggerHintContains' },
  regex:    { labelKey: 'triggerMatchRegex',    icon: '/',  badgeClass: 'bg-orange/20 text-orange border-orange/40',     placeholder: '^H.*O$',   hintKey: 'triggerHintRegex'    },
  range:    { labelKey: 'triggerMatchRange',    icon: '↔',  badgeClass: 'bg-green/20 text-green border-green/40',         placeholder: '1..10',    hintKey: 'triggerHintRange'    },
  glob:     { labelKey: 'triggerMatchGlob',     icon: '*',  badgeClass: 'bg-yellow/20 text-yellow border-yellow/40',     placeholder: 'GET_*',    hintKey: 'triggerHintGlob'     },
};

const MATCH_TYPES: TriggerMatchType[] = ['exact', 'prefix', 'contains', 'regex', 'range', 'glob'];

function ruleSummary(rule: TriggerRule): string {
  if (!rule.enabled) return '⌀ disabled';
  const cfg = MATCH_TYPE_CONFIG[rule.matchType];
  return `${cfg.icon} ${rule.pattern || '(empty)'} → ${rule.outputValue}`;
}

// ============ 内联子组件 1: 规则行 (TriggerRuleRow) ============

interface TriggerRuleRowProps {
  rule: TriggerRule;
  expanded: boolean;
  onToggleExpand: () => void;
  onUpdate: (changes: Partial<TriggerRule>) => void;
  onRemove: () => void;
  lang: Lang;
}

function TriggerRuleRow({ rule, expanded, onToggleExpand, onUpdate, onRemove, lang }: TriggerRuleRowProps) {
  const cfg = MATCH_TYPE_CONFIG[rule.matchType];
  return (
    <div className="border border-border rounded-sm">
      <div className="flex items-center gap-1.5 px-1.5 py-1 cursor-pointer select-none" onClick={onToggleExpand}>
        <span className={`inline-flex items-center gap-0.5 px-1 py-0.5 rounded-sm text-[9px] font-semibold uppercase border ${cfg.badgeClass}`}>
          <span className="font-mono">{cfg.icon}</span>
          {t(lang, cfg.labelKey)}
        </span>
        <span className="text-[10px] text-text-secondary font-mono truncate flex-1 min-w-0">{ruleSummary(rule)}</span>
        <span className="text-text-secondary shrink-0 p-0.5 pointer-events-none">
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </span>
        <button
          className="text-text-secondary hover:text-red shrink-0 p-0.5"
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          title={t(lang, 'removeWidget')}
        >
          <Trash2 size={11} />
        </button>
      </div>
      {expanded && (
        <div className="px-2 pb-2 flex flex-col gap-1.5">
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-[10px] text-text-secondary">{t(lang, 'triggerEnabled')}</label>
            <button
              className={`bg-bg-input border border-border text-text-secondary px-2 py-0.5 text-xs rounded-sm cursor-pointer transition-all hover:text-text-primary text-left ${rule.enabled ? 'bg-bg-button text-text-inverse border-bg-button' : ''}`}
              onClick={() => onUpdate({ enabled: !rule.enabled })}
            >
              {rule.enabled ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
            </button>
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-[10px] text-text-secondary">{t(lang, 'triggerMatchType')}</label>
            <select
              className="form-select text-xs w-full"
              value={rule.matchType}
              onChange={(e) => onUpdate({ matchType: e.target.value as TriggerMatchType })}
            >
              {MATCH_TYPES.map((mt) => (
                <option key={mt} value={mt}>{t(lang, MATCH_TYPE_CONFIG[mt].labelKey)}</option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-[10px] text-text-secondary">{t(lang, 'triggerPattern')}</label>
            <input
              type="text"
              className="form-input text-xs font-mono w-full"
              value={rule.pattern}
              placeholder={cfg.placeholder}
              onChange={(e) => onUpdate({ pattern: e.target.value })}
              title={t(lang, cfg.hintKey)}
            />
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-[10px] text-text-secondary">{t(lang, 'triggerOutputType')}</label>
            <div className="flex bg-bg-input rounded border border-border overflow-hidden">
              {(['number', 'string'] as const).map((ot) => (
                <button
                  key={ot}
                  className={`flex-1 px-2 py-0.5 text-xs transition-colors ${ot !== 'number' ? 'border-l border-border' : ''} ${rule.outputType === ot ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
                  onClick={() => onUpdate({ outputType: ot })}
                >
                  {ot === 'number' ? t(lang, 'triggerOutputNumber') : t(lang, 'triggerOutputString')}
                </button>
              ))}
            </div>
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-[10px] text-text-secondary">
              {rule.outputType === 'string' ? t(lang, 'triggerOutputText') : t(lang, 'triggerOutputValue')}
            </label>
            {rule.outputType === 'string' ? (
              <input
                type="text"
                className="form-input text-xs w-full"
                value={rule.outputText}
                onChange={(e) => onUpdate({ outputText: e.target.value })}
              />
            ) : (
              <input
                type="number"
                step="any"
                className="form-input text-xs font-mono w-full"
                value={Number.isFinite(rule.outputValue) ? rule.outputValue : 0}
                onChange={(e) => {
                  const n = parseFloat(e.target.value);
                  onUpdate({ outputValue: Number.isFinite(n) ? n : 0 });
                }}
              />
            )}
          </div>
          {rule.matchType === 'regex' && (
            <div className="grid grid-cols-[80px_1fr] items-center gap-2">
              <label className="text-[10px] text-text-secondary">{t(lang, 'triggerFlags')}</label>
              <input
                type="text"
                className="form-input text-xs font-mono w-full"
                value={rule.flags ?? ''}
                placeholder="i, im, ims"
                onChange={(e) => onUpdate({ flags: e.target.value })}
                title={t(lang, 'triggerFlagsHint')}
              />
            </div>
          )}
          <div className="text-[10px] text-text-secondary opacity-70">{t(lang, cfg.hintKey)}</div>
        </div>
      )}
    </div>
  );
}

// ============ 内联子组件 2: 手动模式面板 (ManualPanel) ============

interface ManualPanelProps {
  command: string;
  onCommandChange: (s: string) => void;
  result: TriggerResultSnapshot | null;
  lang: Lang;
}

function ManualPanel({ command, onCommandChange, result, lang }: ManualPanelProps) {
  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'triggerManualInput')}</div>
      <textarea
        className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-2 py-1.5 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
        value={command}
        onChange={(e) => onCommandChange(e.target.value)}
        placeholder={t(lang, 'triggerCmdPlaceholder')}
        spellCheck={false}
        rows={3}
      />

      {/* 后端每帧以当前 command 求值, 结果为实时快照 */}
      {result && <ResultBlock result={result} titleKey="triggerMatchResult" lang={lang} />}
    </>
  );
}

/// 结果快照 — 读自后端图输出 (graphOutputs[id].value / .matched)
interface TriggerResultSnapshot {
  matched: boolean;
  value: number;
}

// 复用: 结果预览 (matched + value)
function ResultBlock({ result, titleKey, lang }: { result: TriggerResultSnapshot; titleKey: string; lang: Lang }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, titleKey)}</div>
      <div className="flex items-center gap-2 px-2 py-1 bg-bg-editor rounded-sm">
        <span className="text-[10px] text-text-secondary">{t(lang, 'triggerMatched')}</span>
        <span className={`text-xs font-mono ${result.matched ? 'text-green' : 'text-red'}`}>
          {result.matched ? '✓ YES' : '✗ NO'}
        </span>
      </div>
      <div className="flex items-center justify-between gap-2 px-2 py-1 bg-bg-editor rounded-sm">
        <span className="text-[10px] text-text-secondary">{t(lang, 'triggerValue')}</span>
        <span className="text-xs font-mono text-green">{result.value.toFixed(4)}</span>
      </div>
    </div>
  );
}

// ============ 内联子组件 3: 自动模式面板 (AutoPanel) ============

interface AutoPanelProps {
  triggerValue: number;
  edge: 'level' | 'rising';
  onEdgeChange: (e: 'level' | 'rising') => void;
  result: TriggerResultSnapshot | null;
  lang: Lang;
}

function AutoPanel({ triggerValue, edge, onEdgeChange, result, lang }: AutoPanelProps) {
  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">
        {t(lang, 'triggerAutoInput')}
      </div>

      <div className="flex items-center gap-2 px-2 py-1.5 bg-bg-editor border border-border rounded-sm">
        <span className="text-[10px] text-text-secondary">{t(lang, 'triggerPort')}</span>
        <span className={`text-xs font-mono ${triggerValue !== 0 ? 'text-green' : 'text-text-secondary'}`}>
          {triggerValue.toFixed(4)}
        </span>
        <span className={`ml-auto text-[10px] px-1.5 py-0.5 rounded ${triggerValue !== 0 ? 'bg-green/20 text-green' : 'bg-bg-input text-text-secondary'}`}>
          {triggerValue !== 0 ? 'ACTIVE' : 'IDLE'}
        </span>
      </div>

      <div className="grid grid-cols-[80px_1fr] items-center gap-2">
        <label className="text-[10px] text-text-secondary">{t(lang, 'triggerEdge')}</label>
        <div className="flex bg-bg-input rounded border border-border overflow-hidden">
          {(['level', 'rising'] as const).map((e) => (
            <button
              key={e}
              className={`flex-1 px-2 py-0.5 text-xs transition-colors ${e !== 'level' ? 'border-l border-border' : ''} ${edge === e ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => onEdgeChange(e)}
            >
              {t(lang, e === 'level' ? 'triggerEdgeLevel' : 'triggerEdgeRising')}
            </button>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-[80px_1fr] items-center gap-2">
        <label className="text-[10px] text-text-secondary">{t(lang, 'triggerCommand')}</label>
        {/* 自动模式的匹配输入来自上游 trigger 端口, 只读展示 (手动命令文本不参与) */}
        <span className="text-xs font-mono text-text-primary px-2 py-1 bg-bg-input border border-border rounded truncate">
          {String(triggerValue)}
        </span>
      </div>

      {result && <ResultBlock result={result} titleKey="triggerLastResult" lang={lang} />}
    </>
  );
}

// ============ 主组件: Trigger ============

interface TriggerProps {
  widget: Extract<WidgetConfig, { kind: 'Trigger' }>;
  onRemove: () => void;
}

export function Trigger({ widget }: TriggerProps) {
  const params: TriggerConfig = widget.params;
  const { id, mode, edge, defaultMiss, defaultMissText, command, rules } = params;

  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);

  // 块列表 UI 状态
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  // 结果展示: 读后端图输出快照 (后端每帧求值, manual/auto 都由后端驱动)
  // value/matched 走数值平面; 后端尚未产出 (节点未进图) 时不显示结果区
  const resultValue = useNumericOutput(id, 'value').latest?.value;
  const resultMatched = useNumericOutput(id, 'matched').latest?.value;
  const lastResult: TriggerResultSnapshot | null =
    resultValue === undefined && resultMatched === undefined
      ? null
      : { value: resultValue ?? 0, matched: (resultMatched ?? 0) !== 0 };

  // 自动模式: 读取上游 trigger 端口 (仅展示; 边沿检测与匹配在后端)
  const triggerValue = useNumericInput(id, 'trigger').latest?.value ?? 0;

  // 通用: 更新 widget params (注意保留其它字段)
  const updateParams = useCallback(
    (changes: Partial<TriggerConfig>) => {
      updateWidget(id, { kind: 'Trigger', params: { ...params, ...changes } });
    },
    [id, params, updateWidget],
  );

  // 块列表操作
  const toggleExpand = useCallback((ruleId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(ruleId)) next.delete(ruleId);
      else next.add(ruleId);
      return next;
    });
  }, []);

  const handleAddRule = useCallback(
    (matchType: TriggerMatchType) => {
      const newRule: TriggerRule = {
        id: nanoid(6),
        pattern: '',
        matchType,
        outputType: 'number',
        outputValue: 0,
        outputText: '',
        enabled: true,
      };
      updateParams({ rules: [...rules, newRule] });
      setExpandedIds((prev) => new Set(prev).add(newRule.id));
    },
    [rules, updateParams],
  );

  const handleUpdateRule = useCallback(
    (ruleId: string, changes: Partial<TriggerRule>) => {
      updateParams({
        rules: rules.map((r) => (r.id === ruleId ? { ...r, ...changes } : r)),
      });
    },
    [rules, updateParams],
  );

  const handleRemoveRule = useCallback(
    (ruleId: string) => {
      updateParams({ rules: rules.filter((r) => r.id !== ruleId) });
      setExpandedIds((prev) => {
        const next = new Set(prev);
        next.delete(ruleId);
        return next;
      });
    },
    [rules, updateParams],
  );

  // 手动模式: command 改变时实时同步到 widget.params (持久化用户最新输入)
  // 后端图每帧以当前 command 求值 — 无需前端触发
  // 自动模式不使用该输入 — 匹配输入来自上游 trigger 端口
  const handleCommandChange = useCallback(
    (next: string) => updateParams({ command: next }),
    [updateParams],
  );

  return (
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 左侧: 规则块列表 */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col gap-2 p-3 overflow-y-auto bg-bg-sidebar">
        {/* 顶部: 标题 + 模式切换 */}
        <div className="flex items-center justify-between pb-1.5 border-b border-border shrink-0">
          <span className="text-base font-semibold text-text-bright">{params.label || t(lang, 'triggerTitle')}</span>
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-text-secondary">{rules.length} {t(lang, 'triggerRulesCount')}</span>
            {/* 模式切换 */}
            <div className="flex bg-bg-input rounded border border-border overflow-hidden">
              <button
                className={`px-2 py-0.5 text-[10px] transition-colors ${mode === 'manual' ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
                onClick={() => updateParams({ mode: 'manual' })}
                title={t(lang, 'triggerModeManual')}
              >
                <Play size={10} className="inline mr-0.5" />
                {t(lang, 'triggerModeManual')}
              </button>
              <button
                className={`px-2 py-0.5 text-[10px] transition-colors border-l border-border ${mode === 'auto' ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
                onClick={() => updateParams({ mode: 'auto' })}
                title={t(lang, 'triggerModeAuto')}
              >
                <Radio size={10} className="inline mr-0.5" />
                {t(lang, 'triggerModeAuto')}
              </button>
            </div>
          </div>
        </div>

        {/* 规则列表 */}
        <div className="flex flex-col gap-1.5">
          {rules.length === 0 && (
            <div className="text-xs text-text-secondary opacity-60 italic py-4 text-center">{t(lang, 'triggerRulesEmpty')}</div>
          )}
          {rules.map((rule) => (
            <TriggerRuleRow
              key={rule.id}
              rule={rule}
              expanded={expandedIds.has(rule.id)}
              onToggleExpand={() => toggleExpand(rule.id)}
              onUpdate={(changes) => handleUpdateRule(rule.id, changes)}
              onRemove={() => handleRemoveRule(rule.id)}
              lang={lang}
            />
          ))}
        </div>

        <div className="flex flex-wrap gap-1 pt-1 border-t border-border shrink-0">
          {MATCH_TYPES.map((mt) => {
            const cfg = MATCH_TYPE_CONFIG[mt];
            return (
              <button
                key={mt}
                className="inline-flex items-center gap-1 bg-transparent border border-dashed border-border text-text-secondary px-2 py-1 text-[11px] rounded-sm cursor-pointer transition-all hover:text-text-primary hover:border-accent"
                onClick={() => handleAddRule(mt)}
                title={t(lang, cfg.hintKey)}
              >
                <Plus size={11} />
                <span className={`inline-flex items-center gap-0.5 px-1 py-0.5 rounded-sm text-[9px] font-semibold border ${cfg.badgeClass}`}>
                  <span className="font-mono">{cfg.icon}</span>
                  {t(lang, cfg.labelKey)}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 右侧: 模式面板 + 全局设置 */}
      <div className="w-[320px] shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-3">
        {mode === 'manual' ? (
          <ManualPanel command={command} onCommandChange={handleCommandChange} result={lastResult} lang={lang} />
        ) : (
          <AutoPanel triggerValue={triggerValue} edge={edge}
            onEdgeChange={(e) => updateParams({ edge: e })} result={lastResult} lang={lang} />
        )}

        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-1">{t(lang, 'triggerGlobalSettings')}</div>
        <div className="flex flex-col gap-2 p-2 bg-bg-editor border border-border rounded">
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdLabel')}</label>
            <input type="text" value={params.label}
              onChange={(e) => updateParams({ label: e.target.value })}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors" />
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'triggerMissValue')}</label>
            <input type="number" step="any"
              value={Number.isFinite(defaultMiss) ? defaultMiss : 0}
              onChange={(e) => { const n = parseFloat(e.target.value); updateParams({ defaultMiss: Number.isFinite(n) ? n : 0 }); }}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors font-mono" />
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'triggerMissText')}</label>
            <input type="text"
              value={defaultMissText}
              onChange={(e) => updateParams({ defaultMissText: e.target.value })}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors" />
          </div>
        </div>
      </div>
    </div>
  );
}
