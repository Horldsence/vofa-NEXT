// ============ 数据 Tab 图标 ============
import {
  Activity as ActivityIcon,
  AlertTriangle as AlertTriangleIcon,
  BarChart3 as BarChart3Icon,
  Box as BoxIcon,
  CircuitBoard as CircuitBoardIcon,
  Cpu as CpuIcon,
  History as HistoryIcon,
  Image as ImageIcon,
  LineChart as LineChartIcon,
  ListTree as ListTreeIcon,
  PieChart as PieIcon,
  ScanText as ScanTextIcon,
  Send as SendIcon,
  Settings2 as SettingsIcon,
  Zap as ZapIcon,
} from 'lucide-react';

/// 数据 Tab 图标 (按类型)
export function DataTabIcon({ type, size = 12 }: { type: string; size?: number }) {
  switch (type) {
    case 'waveform':
    case 'waveform-extra':
      return <LineChartIcon size={size} />;
    case 'raw':
      return <ActivityIcon size={size} />;
    case 'pie':
      return <PieIcon size={size} />;
    case 'image':
      return <ImageIcon size={size} />;
    case 'model3d':
      return <BoxIcon size={size} />;
    case 'spectrum':
      return <BarChart3Icon size={size} />;
    case 'command':
      return <SendIcon size={size} />;
    case 'can':
      return <CpuIcon size={size} />;
    case 'logic':
      return <CircuitBoardIcon size={size} />;
    case 'frame-decoder':
      return <ScanTextIcon size={size} />;
    case 'trigger':
      return <ZapIcon size={size} />;
    case 'table-view':
      return <BarChart3Icon size={size} />;
    case 'compile-errors':
      return <AlertTriangleIcon size={size} />;
    case 'compile-results':
      return <ListTreeIcon size={size} />;
    case 'operation-history':
      return <HistoryIcon size={size} />;
    case 'node-properties':
      return <SettingsIcon size={size} />;
    default:
      return null;
  }
}
