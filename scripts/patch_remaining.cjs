const fs = require('fs');

// --- DataPanel.tsx ---
let dp = fs.readFileSync('src/components/layout/DataPanel.tsx', 'utf8');

// 1. Add import
dp = dp.replace(
  "import { FrameDecoder } from '../displays/FrameDecoder';",
  "import { FrameDecoder } from '../displays/FrameDecoder';\nimport { TableView } from '../displays/TableView';"
);

// 2. Add table-view case
dp = dp.replace(
  "case 'frame-decoder': {\n        const widget = widgets.find(",
  "case 'table-view': {\n        const widget = widgets.find(\n          (w) => w.params.id === tab.widgetId && w.kind === 'TableView'\n        ) as Extract<WidgetConfig, { kind: 'TableView' }> | undefined;\n        if (!widget) return <div className=\"flex items-center justify-center h-full text-text-secondary text-sm\">{t(lang, 'noWidgets')}</div>;\n        const cmdWidget = widgets.find(\n          (w) => w.kind === 'Command' && w.params.loopbackEnabled\n        ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;\n        return (\n          <div className=\"flex h-full w-full\">\n            <TableView widget={widget} onRemove={() => {}} loopbackHistory={cmdWidget?.params.loopbackHistory} />\n          </div>\n        );\n      }\n      case 'frame-decoder': {\n        const widget = widgets.find("
);

// 3. Add table-view icon
dp = dp.replace(
  "case 'frame-decoder':\n        return <ScanText size={12} />;",
  "case 'frame-decoder':\n        return <ScanText size={12} />;\n      case 'table-view':\n        return <BarChart3 size={12} />;"
);

// 4. Add table-view close confirm
dp = dp.replace(
  "case 'frame-decoder':\n        return t(lang, 'closeFrameDecoderTab');",
  "case 'frame-decoder':\n        return t(lang, 'closeFrameDecoderTab');\n      case 'table-view':\n        return t(lang, 'closeTableViewTab');"
);

fs.writeFileSync('src/components/layout/DataPanel.tsx', dp);

// --- i18n zh.yml ---
let zh = fs.readFileSync('src/i18n/locales/zh.yml', 'utf8');
zh += '\n# TableView\ntableViewNoColumns: 未配置列\ntableViewShowRaw: 显示原始字节\ntableViewHideRaw: 隐藏原始字节\ntableViewTimestamp: 时间戳\ntableViewEmpty: 等待数据...\ntableViewRows: 行\ntableViewLoopbackRows: 回环\ntableViewLabel: 表格\ntableViewDesc: 以表格形式展示上游节点的输出值\ncloseTableViewTab: 关闭表格\n\n# Loopback\ncmdLoopback: 回环模式\ncmdLoopbackDesc: 发送后捕获协议引擎解析结果\ncmdLoopbackManual: 手动发送\ncmdLoopbackOnChange: 值变化发送\ncmdLoopbackTimer: 定时发送\ncmdLoopbackInterval: 定时间隔 (ms)\n';
fs.writeFileSync('src/i18n/locales/zh.yml', zh);

// --- i18n en.yml ---
let en = fs.readFileSync('src/i18n/locales/en.yml', 'utf8');
en += '\n# TableView\ntableViewNoColumns: No columns\ntableViewShowRaw: Show Raw Bytes\ntableViewHideRaw: Hide Raw Bytes\ntableViewTimestamp: Timestamp\ntableViewEmpty: Waiting for data...\ntableViewRows: rows\ntableViewLoopbackRows: loopback\ntableViewLabel: Table\ntableViewDesc: Display upstream node outputs in table form\ncloseTableViewTab: Close Table\n\n# Loopback\ncmdLoopback: Loopback\ntableViewShowRaw: Show Raw Bytes\ntableViewHideRaw: Hide Raw Bytes\ncmdLoopbackDesc: Capture protocol parsed results after sending\ncmdLoopbackManual: Manual send\ncmdLoopbackOnChange: Send on change\ncmdLoopbackTimer: Timed send\ncmdLoopbackInterval: Interval (ms)\n';
fs.writeFileSync('src/i18n/locales/en.yml', en);

console.log('all done');
