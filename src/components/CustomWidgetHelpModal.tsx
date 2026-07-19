import { X, BookOpen, Code, Lightbulb, Send, Settings, Workflow } from 'lucide-react';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';

interface CustomWidgetHelpModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function CustomWidgetHelpModal({ isOpen, onClose }: CustomWidgetHelpModalProps) {
  const lang = useAppStore((s) => s.lang);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]" onClick={onClose}>
      <div
        className="flex flex-col bg-bg-sidebar border border-border rounded-lg overflow-hidden shadow-[0_20px_60px_rgba(0,0,0,0.6)] animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        style={{ width: '80vw', height: '85vh', maxWidth: 1000, maxHeight: 800 }}
      >
        <div className="flex items-center gap-2 px-3 py-2 bg-bg-panel-header border-b border-border text-text-primary font-semibold">
          <BookOpen size={16} />
          <span>{t(lang, 'helpTitle')}</span>
          <button
            className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-auto"
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 flex flex-col gap-5">
          {/* 快速入门 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Lightbulb size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpQuickStart')}
            </h2>
            <ol className="m-0 pl-5 text-sm text-text-primary leading-[1.7] list-decimal">
              <li>{t(lang, 'helpStep1')}</li>
              <li>{t(lang, 'helpStep2')}</li>
              <li>{t(lang, 'helpStep3')}</li>
              <li>{t(lang, 'helpStep4')}</li>
              <li>{t(lang, 'helpStep5')}</li>
            </ol>
          </section>

          {/* 代码结构 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Code size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpCodeStructure')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpCodeStructureDesc')}</p>
            <pre className="m-0 px-3 py-2.5 bg-bg-input border border-border rounded font-mono text-xs text-text-primary leading-[1.5] overflow-x-auto whitespace-pre">{`({
  name: 'MyWidget',           // ${t(lang, 'helpFieldName')}
  description: '描述信息',     // ${t(lang, 'helpFieldDescription')}
  inputs: [                    // ${t(lang, 'helpFieldInputs')}
    { id: 'value', label: 'Value' },
    { id: 'setpoint', label: 'Setpoint' }
  ],
  outputs: [                   // ${t(lang, 'helpFieldOutputs')}
    { id: 'alarm', label: 'Alarm' }
  ],
  settings: [                  // ${t(lang, 'helpFieldSettings')}
    { id: 'threshold', label: '阈值', type: 'number', default: 50 },
    { id: 'color', label: '颜色', type: 'color', default: '#ff0000' },
    { id: 'text', label: '文本', type: 'text', default: 'ALARM' },
    { id: 'enabled', label: '启用', type: 'boolean', default: true }
  ],
  onMount: function(ctx) {     // ${t(lang, 'helpFieldOnMount')}
    ctx.state.count = 0;
  },
  render: function(ctx) {      // ${t(lang, 'helpFieldRender')}
    const v = ctx.inputs.value ?? 0;
    ctx.el.innerHTML = '<div>' + v + '</div>';
  },
  onUnmount: function(ctx) {}  // ${t(lang, 'helpFieldOnUnmount')}
})`}</pre>
          </section>

          {/* ctx API */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Workflow size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpCtxApi')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpCtxApiDesc')}</p>
            <table className="w-full border-collapse text-xs">
              <thead>
                <tr className="bg-bg-panel-header text-text-secondary font-semibold text-left">
                  <th className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiField')}</th>
                  <th className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiType')}</th>
                  <th className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiDesc')}</th>
                </tr>
              </thead>
              <tbody className="text-text-primary">
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.el</code></td>
                  <td className="px-2 py-1.5 border-b border-border">HTMLElement</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiEl')}</td>
                </tr>
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.inputs</code></td>
                  <td className="px-2 py-1.5 border-b border-border">{'Record<string, number>'}</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiInputs')}</td>
                </tr>
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.settings</code></td>
                  <td className="px-2 py-1.5 border-b border-border">{'Record<string, any>'}</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiSettings')}</td>
                </tr>
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.state</code></td>
                  <td className="px-2 py-1.5 border-b border-border">{'Record<string, any>'}</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiState')}</td>
                </tr>
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.send(port, value)</code></td>
                  <td className="px-2 py-1.5 border-b border-border">function</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiSend')}</td>
                </tr>
                <tr>
                  <td className="px-2 py-1.5 border-b border-border"><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">ctx.log(...args)</code></td>
                  <td className="px-2 py-1.5 border-b border-border">function</td>
                  <td className="px-2 py-1.5 border-b border-border">{t(lang, 'helpApiLog')}</td>
                </tr>
              </tbody>
            </table>
          </section>

          {/* 输入输出 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Send size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpInputsOutputs')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpIoDesc')}</p>
            <ul className="m-0 pl-5 text-sm text-text-primary leading-[1.7] list-disc">
              <li>{t(lang, 'helpIoInputItem')}</li>
              <li>{t(lang, 'helpIoOutputItem')}</li>
              <li>{t(lang, 'helpIoChannelSource')}</li>
              <li>{t(lang, 'helpIoMathChain')}</li>
            </ul>
          </section>

          {/* 设置项类型 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Settings size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpSettingsTypes')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpSettingsDesc')}</p>
            <ul className="m-0 pl-5 text-sm text-text-primary leading-[1.7] list-disc">
              <li><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">number</code> — {t(lang, 'helpSettingsNumber')}</li>
              <li><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">text</code> — {t(lang, 'helpSettingsText')}</li>
              <li><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">color</code> — {t(lang, 'helpSettingsColor')}</li>
              <li><code className="bg-bg-input px-1 py-0.5 rounded-sm text-accent font-mono">boolean</code> — {t(lang, 'helpSettingsBoolean')}</li>
            </ul>
          </section>

          {/* 示例 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Code size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpExamples')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpExamplesDesc')}</p>
            <pre className="m-0 px-3 py-2.5 bg-bg-input border border-border rounded font-mono text-xs text-text-primary leading-[1.5] overflow-x-auto whitespace-pre">{`// ${t(lang, 'helpExample1Title')}
({
  name: 'ThresholdAlarm',
  inputs: [
    { id: 'value', label: 'Value' },
    { id: 'setpoint', label: 'Setpoint' }
  ],
  outputs: [{ id: 'alarm', label: 'Alarm' }],
  settings: [
    { id: 'threshold', label: 'Threshold', type: 'number', default: 50 },
    { id: 'color', label: 'Color', type: 'color', default: '#ff5555' }
  ],
  onMount: function(ctx) {
    ctx.el.innerHTML = '<div style="padding:8px;text-align:center;font-family:sans-serif"></div>';
    var alarmDiv = ctx.el.querySelector('div');
    alarmDiv.addEventListener('click', function() {
      ctx.send('alarm', 1);
      ctx.log('Alarm sent');
    });
  },
  render: function(ctx) {
    var v = ctx.inputs.value ?? 0;
    var sp = ctx.inputs.setpoint ?? 0;
    var threshold = Number(ctx.settings.threshold) || 50;
    var color = ctx.settings.color || '#ff5555';
    var alarm = Math.abs(v - sp) > threshold;
    var div = ctx.el.querySelector('div');
    if (div) {
      div.style.background = alarm ? color : '#333';
      div.style.color = 'white';
      div.style.padding = '8px';
      div.style.borderRadius = '4px';
      div.textContent = alarm ? '⚠ ALARM' : 'OK';
    }
  }
})

// ${t(lang, 'helpExample2Title')}
({
  name: 'Oscilloscope',
  inputs: [{ id: 'value', label: 'Value' }],
  outputs: [],
  settings: [
    { id: 'maxPoints', label: 'Max Points', type: 'number', default: 100 },
    { id: 'color', label: 'Color', type: 'color', default: '#75beff' }
  ],
  onMount: function(ctx) {
    ctx.state.data = [];
    var canvas = document.createElement('canvas');
    canvas.style.width = '100%';
    canvas.style.height = '80px';
    ctx.el.appendChild(canvas);
    ctx.state.canvas = canvas;
  },
  render: function(ctx) {
    var v = ctx.inputs.value ?? 0;
    var max = Number(ctx.settings.maxPoints) || 100;
    ctx.state.data.push(v);
    if (ctx.state.data.length > max) ctx.state.data.shift();
    var canvas = ctx.state.canvas;
    if (!canvas) return;
    var ctx2d = canvas.getContext('2d');
    var w = canvas.width = canvas.offsetWidth;
    var h = canvas.height = canvas.offsetHeight;
    ctx2d.clearRect(0, 0, w, h);
    ctx2d.strokeStyle = ctx.settings.color || '#75beff';
    ctx2d.lineWidth = 1.5;
    ctx2d.beginPath();
    var data = ctx.state.data;
    var min = Math.min.apply(null, data);
    var max2 = Math.max.apply(null, data);
    var range = (max2 - min) || 1;
    for (var i = 0; i < data.length; i++) {
      var x = (i / (max - 1)) * w;
      var y = h - ((data[i] - min) / range) * h;
      if (i === 0) ctx2d.moveTo(x, y);
      else ctx2d.lineTo(x, y);
    }
    ctx2d.stroke();
  }
})`}</pre>
          </section>

          {/* 安全说明 */}
          <section className="flex flex-col gap-2">
            <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
              <Lightbulb size={14} className="text-accent flex-shrink-0" />
              {t(lang, 'helpSecurity')}
            </h2>
            <p className="m-0 text-sm text-text-secondary leading-[1.5]">{t(lang, 'helpSecurityDesc')}</p>
            <ul className="m-0 pl-5 text-sm text-text-primary leading-[1.7] list-disc">
              <li>{t(lang, 'helpSecuritySandbox')}</li>
              <li>{t(lang, 'helpSecurityNoDom')}</li>
              <li>{t(lang, 'helpSecurityPostMessage')}</li>
              <li>{t(lang, 'helpSecurityError')}</li>
            </ul>
          </section>
        </div>

        <div className="px-3 py-2 flex justify-end bg-bg-panel-header border-t border-border">
          <button className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover" onClick={onClose}>
            {t(lang, 'helpClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
