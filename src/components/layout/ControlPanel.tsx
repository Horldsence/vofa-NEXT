import { useAppStore, createWidget } from '../../store/appStore';
import { t } from '../../i18n';
import { LayoutGrid, Plus } from 'lucide-react';
import { Knob } from '../controls/Knob';
import { ButtonWidget } from '../controls/ButtonWidget';
import { Radio } from '../controls/Radio';
import { Checkbox } from '../controls/Checkbox';
import { Slider } from '../controls/Slider';
import { Label } from '../controls/Label';
import { PieChart } from '../displays/PieChart';
import { ImageViewer } from '../displays/ImageViewer';

/// 控件区 — 显示用户添加的交互控件
export function ControlPanel() {
  const lang = useAppStore((s) => s.lang);
  const widgets = useAppStore((s) => s.widgets);
  const removeWidget = useAppStore((s) => s.removeWidget);
  const addWidget = useAppStore((s) => s.addWidget);

  const handleQuickAdd = () => {
    addWidget(createWidget('Slider'));
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <span>{t(lang, 'controlArea')}</span>
        <div className="panel-header-actions">
          <button
            className="btn-icon"
            title={t(lang, 'addWidget')}
            onClick={handleQuickAdd}
          >
            <Plus size={14} />
          </button>
          <LayoutGrid size={14} style={{ opacity: 0.5 }} />
        </div>
      </div>
      <div className="panel-content">
        {widgets.filter((w) => w.kind !== 'Waveform').length === 0 ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: 'var(--text-secondary)',
              fontSize: 12,
            }}
          >
            {t(lang, 'noWidgets')}
          </div>
        ) : (
          <div className="widget-grid">
            {widgets.map((widget) => {
              if (widget.kind === 'Waveform') return null;
              const id = widget.params.id;
              const common = {
                key: id,
                onRemove: () => removeWidget(id),
              };
              switch (widget.kind) {
                case 'Knob':
                  return <Knob {...common} widget={widget} key={id} />;
                case 'Button':
                  return <ButtonWidget {...common} widget={widget} key={id} />;
                case 'Radio':
                  return <Radio {...common} widget={widget} key={id} />;
                case 'Checkbox':
                  return <Checkbox {...common} widget={widget} key={id} />;
                case 'Slider':
                  return <Slider {...common} widget={widget} key={id} />;
                case 'Label':
                  return <Label {...common} widget={widget} key={id} />;
                case 'PieChart':
                  return <PieChart {...common} widget={widget} key={id} />;
                case 'Image':
                  return <ImageViewer {...common} widget={widget} key={id} />;
                default:
                  return null;
              }
            })}
          </div>
        )}
      </div>
    </div>
  );
}
