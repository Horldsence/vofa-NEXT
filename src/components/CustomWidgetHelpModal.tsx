import { X, BookOpen, Code, Lightbulb, Send, Settings, Workflow } from 'lucide-react';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';

interface CustomWidgetHelpModalProps {
  isOpen: boolean;
  onClose: () => void;
}

/// 自定义控件帮助说明弹窗 — 展示使用说明与 API 设计
/// 入口: CustomWidgetEditor 顶部的 "?" 按钮
export function CustomWidgetHelpModal({ isOpen, onClose }: CustomWidgetHelpModalProps) {
  const lang = useAppStore((s) => s.lang);

  if (!isOpen) return null;

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div
        className="custom-help-modal"
        onClick={(e) => e.stopPropagation()}
        style={{ width: '80vw', height: '85vh', maxWidth: 1000, maxHeight: 800 }}
      >
        <div className="settings-header" style={{ padding: '8px 12px' }}>
          <BookOpen size={16} />
          <span>{t(lang, 'helpTitle')}</span>
          <button className="btn-icon" onClick={onClose} style={{ marginLeft: 'auto' }}>
            <X size={14} />
          </button>
        </div>

        <div className="custom-help-body">
          {/* 快速入门 */}
          <section className="help-section">
            <h2 className="help-section-title">
              <Lightbulb size={14} />
              {t(lang, 'helpQuickStart')}
            </h2>
            <ol className="help-steps">
              <li>{t(lang, 'helpStep1')}</li>
              <li>{t(lang, 'helpStep2')}</li>
              <li>{t(lang, 'helpStep3')}</li>
              <li>{t(lang, 'helpStep4')}</li>
              <li>{t(lang, 'helpStep5')}</li>
            </ol>
          </section>

          {/* 代码结构 */}
          <section className="help-section">
            <h2 className="help-section-title">
              <Code size={14} />
              {t(lang, 'helpCodeStructure')}
            </h2>
            <p className="help-desc">{t(lang, 'helpCodeStructureDesc')}</p>
            <pre className="help-code-block">{`({
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
          <section className="help-section">
            <h2 className="help-section-title">
              <Workflow size={14} />
              {t(lang, 'helpCtxApi')}
            </h2>
            <p className="help-desc">{t(lang, 'helpCtxApiDesc')}</p>
            <table className="help-api-table">
              <thead>
                <tr>
                  <th>{t(lang, 'helpApiField')}</th>
                  <th>{t(lang, 'helpApiType')}</th>
                  <th>{t(lang, 'helpApiDesc')}</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td><code>ctx.el</code></td>
                  <td>HTMLElement</td>
                  <td>{t(lang, 'helpApiEl')}</td>
                </tr>
                <tr>
                  <td><code>ctx.inputs</code></td>
                  <td>{'Record<string, number>'}</td>
                  <td>{t(lang, 'helpApiInputs')}</td>
                </tr>
                <tr>
                  <td><code>ctx.settings</code></td>
                  <td>{'Record<string, any>'}</td>
                  <td>{t(lang, 'helpApiSettings')}</td>
                </tr>
                <tr>
                  <td><code>ctx.state</code></td>
                  <td>{'Record<string, any>'}</td>
                  <td>{t(lang, 'helpApiState')}</td>
                </tr>
                <tr>
                  <td><code>ctx.send(port, value)</code></td>
                  <td>function</td>
                  <td>{t(lang, 'helpApiSend')}</td>
                </tr>
                <tr>
                  <td><code>ctx.log(...args)</code></td>
                  <td>function</td>
                  <td>{t(lang, 'helpApiLog')}</td>
                </tr>
              </tbody>
            </table>
          </section>

          {/* 输入输出 */}
          <section className="help-section">
            <h2 className="help-section-title">
              <Send size={14} />
              {t(lang, 'helpInputsOutputs')}
            </h2>
            <p className="help-desc">{t(lang, 'helpIoDesc')}</p>
            <ul className="help-list">
              <li>{t(lang, 'helpIoInputItem')}</li>
              <li>{t(lang, 'helpIoOutputItem')}</li>
              <li>{t(lang, 'helpIoChannelSource')}</li>
              <li>{t(lang, 'helpIoMathChain')}</li>
            </ul>
          </section>

          {/* 设置项类型 */}
          <section className="help-section">
            <h2 className="help-section-title">
              <Settings size={14} />
              {t(lang, 'helpSettingsTypes')}
            </h2>
            <p className="help-desc">{t(lang, 'helpSettingsDesc')}</p>
            <ul className="help-list">
              <li><code>number</code> — {t(lang, 'helpSettingsNumber')}</li>
              <li><code>text</code> — {t(lang, 'helpSettingsText')}</li>
              <li><code>color</code> — {t(lang, 'helpSettingsColor')}</li>
              <li><code>boolean</code> — {t(lang, 'helpSettingsBoolean')}</li>
            </ul>
          </section>

          {/* 示例 */}
          <section className="help-section">
            <h2 className="help-section-title">
              <Code size={14} />
              {t(lang, 'helpExamples')}
            </h2>
            <p className="help-desc">{t(lang, 'helpExamplesDesc')}</p>
            <pre className="help-code-block">{`// ${t(lang, 'helpExample1Title')}
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
          <section className="help-section">
            <h2 className="help-section-title">
              <Lightbulb size={14} />
              {t(lang, 'helpSecurity')}
            </h2>
            <p className="help-desc">{t(lang, 'helpSecurityDesc')}</p>
            <ul className="help-list">
              <li>{t(lang, 'helpSecuritySandbox')}</li>
              <li>{t(lang, 'helpSecurityNoDom')}</li>
              <li>{t(lang, 'helpSecurityPostMessage')}</li>
              <li>{t(lang, 'helpSecurityError')}</li>
            </ul>
          </section>
        </div>

        <div className="settings-footer" style={{ padding: '8px 12px', display: 'flex', justifyContent: 'flex-end' }}>
          <button className="btn-primary" onClick={onClose}>
            {t(lang, 'helpClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
