import { useAppStore, createWidget } from '../../store/appStore';
import { t } from '../../i18n';
import {
  Gauge as KnobIcon,
  Square,
  CircleDot,
  CheckSquare,
  Sliders,
  Tag,
  LineChart,
  PieChart as PieIcon,
  Image as ImageIcon,
} from 'lucide-react';
import type { WidgetConfig } from '../../types';

/// 控件面板 — 列出所有可添加的控件类型
export function WidgetPalette() {
  const lang = useAppStore((s) => s.lang);
  const addWidget = useAppStore((s) => s.addWidget);

  const palette: {
    kind: WidgetConfig['kind'];
    icon: React.ReactNode;
    label: string;
  }[] = [
    { kind: 'Knob', icon: <KnobIcon />, label: t(lang, 'knob') },
    { kind: 'Button', icon: <Square />, label: t(lang, 'button') },
    { kind: 'Radio', icon: <CircleDot />, label: t(lang, 'radio') },
    { kind: 'Checkbox', icon: <CheckSquare />, label: t(lang, 'checkbox') },
    { kind: 'Slider', icon: <Sliders />, label: t(lang, 'slider') },
    { kind: 'Label', icon: <Tag />, label: t(lang, 'label') },
    { kind: 'Waveform', icon: <LineChart />, label: t(lang, 'waveform') },
    { kind: 'PieChart', icon: <PieIcon />, label: t(lang, 'pieChart') },
    { kind: 'Image', icon: <ImageIcon />, label: t(lang, 'image') },
  ];

  const handleAdd = (kind: WidgetConfig['kind']) => {
    addWidget(createWidget(kind));
  };

  return (
    <div className="palette-grid">
      {palette.map((item) => (
        <div
          key={item.kind}
          className="palette-item"
          onClick={() => handleAdd(item.kind)}
        >
          {item.icon}
          <span>{item.label}</span>
        </div>
      ))}
    </div>
  );
}
