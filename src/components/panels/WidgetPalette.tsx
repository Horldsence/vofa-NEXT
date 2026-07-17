import { useAppStore, createWidget } from '../../store/appStore';
import { t } from '../../i18n';
import {
  Gauge as KnobIcon,
  Square,
  CheckSquare,
  Sliders,
  Tag,
  LineChart,
  PieChart as PieIcon,
  Image as ImageIcon,
  Radio as RadioIcon,
} from 'lucide-react';
import type { WidgetConfig } from '../../types';

/// 控件面板 — 列出所有可添加的控件类型
/// 支持拖拽到节点编辑器画布 或 点击直接添加
export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);

  const palette: {
    kind: WidgetConfig['kind'];
    icon: React.ReactNode;
    label: string;
  }[] = [
    { kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob') },
    { kind: 'Button', icon: <Square />, label: t(lang, 'button') },
    { kind: 'Radio', icon: <RadioIcon />, label: t(lang, 'radio') },
    { kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox') },
    { kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider') },
    { kind: 'Label', icon: <Tag />, label: t(lang, 'label') },
    { kind: 'Waveform', icon: <LineChart />, label: t(lang, 'waveform') },
    { kind: 'PieChart', icon: <PieIcon />, label: t(lang, 'pieChart') },
    { kind: 'Image', icon: <ImageIcon />, label: t(lang, 'image') },
  ];

  const handleDragStart = (e: React.DragEvent, kind: WidgetConfig['kind']) => {
    e.dataTransfer.setData('application/widget-kind', kind);
    e.dataTransfer.effectAllowed = 'copy';
  };

  const handleClick = (kind: WidgetConfig['kind']) => {
    const widget = createWidget(kind);
    // 点击添加时放到默认位置
    addWidget(widget, activeControlTabId, { x: 280, y: 80 + Math.random() * 100 });
  };

  return (
    <div className="palette-grid">
      {palette.map((item) => (
        <div
          key={item.kind}
          className="palette-item"
          draggable
          onDragStart={(e) => handleDragStart(e, item.kind)}
          onClick={() => handleClick(item.kind)}
          style={{ cursor: 'grab' }}
          title={item.label}
        >
          {item.icon}
          <span>{item.label}</span>
        </div>
      ))}
    </div>
  );
}
