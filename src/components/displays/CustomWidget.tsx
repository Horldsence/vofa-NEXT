import { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import { Settings2, AlertCircle } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useAppStore } from '../../store/appStore';

/// 自定义 JS 控件渲染器
///
/// 用户代码在 iframe 沙箱中执行 (sandbox="allow-scripts", 无 allow-same-origin),
/// 通过 postMessage 与主应用通信。
///
/// 用户代码格式 (返回一个对象):
/// ({
///   name: 'MyWidget',           // 显示名 (可选)
///   inputs: [                    // 输入端口 (用于节点编辑器连线)
///     { id: 'value', label: 'Value' }
///   ],
///   outputs: [],                 // 输出端口 (用户可通过 ctx.send 发送)
///   settings: [                  // 配置项 (用户在编辑器中填写)
///     { id: 'threshold', label: 'Threshold', type: 'number', default: 50 }
///   ],
///   onMount: function(ctx) {     // 初始化 (可选)
///     ctx.state = {};
///   },
///   render: function(ctx) {      // 渲染, 每次 inputs/settings 变化时调用
///     const v = ctx.inputs.value ?? 0;
///     ctx.el.innerHTML = '<div style="padding:8px">' + v + '</div>';
///   },
///   onUnmount: function(ctx) {}  // 清理 (可选)
/// })
///
/// ctx 包含:
///   ctx.el         — 渲染根 HTMLElement
///   ctx.inputs     — { [portId]: number }  当前输入值
///   ctx.settings   — { [id]: string|number|boolean }  用户配置值
///   ctx.state      — 任意对象, 用于在 render 间保存状态
///   ctx.send(portId, value) — 向输出端口发送值 (主应用收到后通过 binding 发送)
///   ctx.log(...args) — 日志输出 (编辑器中可见)

export interface CustomWidgetDef {
  name?: string;
  description?: string;
  inputs?: { id: string; label: string }[];
  outputs?: { id: string; label: string }[];
  settings?: {
    id: string;
    label: string;
    type: 'number' | 'text' | 'color' | 'boolean';
    default: string | number | boolean;
  }[];
  onMount?: (ctx: CustomWidgetRuntime) => void;
  onUnmount?: (ctx: CustomWidgetRuntime) => void;
  render?: (ctx: CustomWidgetRuntime) => void;
}

export interface CustomWidgetRuntime {
  el: HTMLElement;
  inputs: Record<string, number>;
  settings: Record<string, string | number | boolean>;
  state: Record<string, unknown>;
  send: (portId: string, value: number) => void;
  log: (...args: unknown[]) => void;
}

interface CustomWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Custom' }>;
  onRemove: () => void;
  onEdit?: () => void;
  height?: number;
}

/// 求值用户代码并返回 widget 定义对象 (供外部使用, 如读取 ports/settings schema)
export function evalCustomWidgetDef(code: string): {
  def: CustomWidgetDef | null;
  error: string | null;
} {
  try {
    // eslint-disable-next-line no-new-func
    const fn = new Function(`'use strict'; return (${code});`);
    const def = fn();
    if (!def || typeof def !== 'object') {
      return { def: null, error: '代码必须返回一个对象' };
    }
    if (typeof def.render !== 'function') {
      return { def: null, error: '代码必须定义 render(ctx) 函数' };
    }
    return { def, error: null };
  } catch (e) {
    return { def: null, error: e instanceof Error ? e.message : String(e) };
  }
}

/// 生成 iframe srcdoc — 内含 bootstrap + 用户代码
function buildSrcDoc(code: string, def: CustomWidgetDef): string {
  // 序列化 def 的方法 (onMount/onUnmount/render) 需要保留为可执行函数
  // 因为 srcdoc 是字符串, 直接把用户代码嵌入并用 eval 调用
  const settingsSchema = JSON.stringify(def.settings ?? []);
  const inputsSchema = JSON.stringify(def.inputs ?? []);
  const outputsSchema = JSON.stringify(def.outputs ?? []);

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<style>
  html, body { margin: 0; padding: 0; height: 100%; background: transparent; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; font-size: 12px; color: #cccccc; }
  #root { width: 100%; height: 100%; overflow: auto; }
  * { box-sizing: border-box; }
</style>
</head>
<body>
<div id="root"></div>
<script>
(function() {
  'use strict';
  var WIDGET_CODE = ${JSON.stringify(code)};
  var SETTINGS_SCHEMA = ${settingsSchema};
  var INPUTS_SCHEMA = ${inputsSchema};
  var OUTPUTS_SCHEMA = ${outputsSchema};

  var el = document.getElementById('root');
  var ctx = {
    el: el,
    inputs: {},
    settings: {},
    state: {},
    send: function(portId, value) {
      parent.postMessage({ source: 'custom-widget', type: 'output', port: portId, value: value }, '*');
    },
    log: function() {
      var args = Array.prototype.slice.call(arguments).map(function(a) {
        return typeof a === 'object' ? JSON.stringify(a) : String(a);
      });
      parent.postMessage({ source: 'custom-widget', type: 'log', args: args }, '*');
    }
  };

  var def = null;
  try {
    // 使用 Function 求值用户代码 (在 iframe 内部, 已隔离)
    var fn = new Function('return (' + WIDGET_CODE + ');');
    def = fn();
    if (!def || typeof def !== 'object') {
      throw new Error('Code must return an object');
    }
    if (typeof def.render !== 'function') {
      throw new Error('Code must define a render(ctx) function');
    }
  } catch (e) {
    parent.postMessage({ source: 'custom-widget', type: 'error', message: e.message || String(e), stack: e.stack || '' }, '*');
    return;
  }

  // 初始化默认设置
  var defaults = {};
  (SETTINGS_SCHEMA || []).forEach(function(s) { defaults[s.id] = s.default; });
  ctx.settings = defaults;

  // 调用 onMount
  try {
    if (typeof def.onMount === 'function') {
      def.onMount(ctx);
    }
  } catch (e) {
    parent.postMessage({ source: 'custom-widget', type: 'error', message: 'onMount: ' + (e.message || String(e)), stack: e.stack || '' }, '*');
  }

  // 接收父窗口更新消息
  window.addEventListener('message', function(ev) {
    var msg = ev.data;
    if (!msg || msg.source !== 'custom-widget-parent' || msg.type !== 'update') return;
    ctx.inputs = msg.inputs || {};
    ctx.settings = msg.settings || ctx.settings;
    try {
      def.render(ctx);
    } catch (e) {
      parent.postMessage({ source: 'custom-widget', type: 'error', message: 'render: ' + (e.message || String(e)), stack: e.stack || '' }, '*');
    }
  });

  // 通知父窗口已就绪
  parent.postMessage({ source: 'custom-widget', type: 'ready', inputs: INPUTS_SCHEMA, outputs: OUTPUTS_SCHEMA, settings: SETTINGS_SCHEMA }, '*');
})();
</script>
</body>
</html>`;
}

export function CustomWidget({ widget, onEdit, height = 120 }: CustomWidgetProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const readyRef = useRef(false);

  // 解析 def (用于 schema 展示)
  const { def, error: defError } = useMemo(() => evalCustomWidgetDef(widget.params.code), [widget.params.code]);

  // 后端图评估桥接:
  //   - customInputs[widgetId] 由后端 30 FPS 推送 (后端已解析 rfEdges, 收集本 widget 的输入)
  //   - submitCustomOutput 用于将 iframe 的 ctx.send 回传到后端图
  // 只订阅本 widget 的输入, 避免全局 customInputs 更新时所有 CustomWidget 重渲染
  const inputs = useAppStore((s) => s.customInputs[widget.params.id] ?? {});
  const submitCustomOutput = useAppStore((s) => s.submitCustomOutput);

  // 缓存最新 inputs / settings 到 ref, 供 sendUpdate 读取, 避免 callback 依赖频繁变化
  const inputsRef = useRef(inputs);
  useEffect(() => { inputsRef.current = inputs; }, [inputs]);
  const settingsRef = useRef(widget.params.settings);
  useEffect(() => { settingsRef.current = widget.params.settings; }, [widget.params.settings]);

  // 监听 iframe 消息
  useEffect(() => {
    const handler = (ev: MessageEvent) => {
      const msg = ev.data;
      if (!msg || msg.source !== 'custom-widget') return;
      switch (msg.type) {
        case 'ready':
          readyRef.current = true;
          setError(null);
          // 发送初始数据
          sendUpdate();
          break;
        case 'output':
          // 用户代码调用了 ctx.send — 回传到后端图 (供下游 widget 读取)
          if (typeof msg.port === 'string' && typeof msg.value === 'number') {
            submitCustomOutput(widget.params.id, { [msg.port]: msg.value });
          }
          break;
        case 'error':
          setError(msg.message);
          break;
        case 'log':
          setLogs((prev) => [...prev.slice(-9), msg.args.join(' ')]);
          break;
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 发送更新到 iframe — 输入值由后端推送 (无需前端解析 edges)
  const sendUpdate = useCallback(() => {
    const iframe = iframeRef.current;
    if (!iframe || !iframe.contentWindow || !readyRef.current) return;
    iframe.contentWindow.postMessage({
      source: 'custom-widget-parent',
      type: 'update',
      inputs: inputsRef.current,
      settings: settingsRef.current,
    }, '*');
  }, []);

  // 后端推送输入变化 → 通知 iframe 重渲染 (替代旧 50ms 轮询)
  useEffect(() => {
    sendUpdate();
  }, [sendUpdate, inputs, widget.params.settings]);

  // srcdoc 内容
  const srcDoc = useMemo(() => {
    if (!def) return '';
    return buildSrcDoc(widget.params.code, def);
  }, [widget.params.code, def]);

  // 渲染错误 (代码语法错误)
  if (defError) {
    return (
      <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative min-h-[80px]">
        {onEdit && (
          <button
            className="absolute top-1 right-6 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
            onClick={onEdit}
          >
            <Settings2 size={11} />
          </button>
        )}
        <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">{widget.params.label || 'Custom'}</div>
        <div className="flex gap-1.5 items-start text-red text-xs p-1.5 bg-red/[0.08] border border-red/30 rounded-sm">
          <AlertCircle size={14} className="flex-shrink-0 mt-px" />
          <pre className="m-0 whitespace-pre-wrap text-[10px] flex-1">{defError}</pre>
        </div>
        {onEdit && (
          <button
            className="mt-1.5 w-full px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover"
            onClick={onEdit}
          >
            编辑代码
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative">
      {onEdit && (
        <button
          className="absolute top-1 right-6 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
          onClick={onEdit}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">
        {def?.name || widget.params.label || 'Custom'}
      </div>
      <iframe
        ref={iframeRef}
        srcDoc={srcDoc}
        sandbox="allow-scripts"
        style={{
          width: '100%',
          height,
          border: 'none',
          background: 'transparent',
          borderRadius: 2,
          display: 'block',
        }}
        title="custom-widget"
      />
      {error && (
        <div className="flex items-center gap-1 px-1.5 py-1 bg-red/10 text-red rounded-sm mt-1">
          <AlertCircle size={10} />
          <span className="text-[10px]">{error}</span>
        </div>
      )}
      {logs.length > 0 && (
        <div className="mt-1 px-1.5 py-1 bg-black/30 rounded-sm max-h-[60px] overflow-y-auto">
          {logs.slice(-3).map((l, i) => (
            <div key={i} className="font-mono text-[10px] text-text-secondary whitespace-pre-wrap break-all">{l}</div>
          ))}
        </div>
      )}
    </div>
  );
}
