const fs = require('fs');
let c = fs.readFileSync('src/components/nodes/WidgetNode.tsx','utf8');

// 1. Command: add loopbackOut output when loopbackEnabled
c = c.replace(
  "return { inputs, outputs: [] };\n    }\n    case 'FrameDecoder': {",
  "const outputs = widget.params.loopbackEnabled ? [{ id: 'loopbackOut', label: 'loopbackOut' }] : []; return { inputs, outputs }; } case 'FrameDecoder': {"
);

// 2. Add TableView case before Custom
c = c.replace(
  "return { inputs: [], outputs };\n    }\n    case 'Custom': {",
  "return { inputs: [], outputs };\n    }\n    case 'TableView': {\n      const cols = widget.params.columns ?? [];\n      return { inputs: cols.map(c => ({ id: c.portName, label: c.label })), outputs: [] };\n    }\n    case 'Custom': {"
);

// 3. Add TableView to renderContent placeholder list
c = c.replace(
  "case 'FrameDecoder':\n        // 这些控件在节点内仅显示占位, 实际渲染在 DataPanel",
  "case 'FrameDecoder':\n    case 'TableView':\n        // 这些控件在节点内仅显示占位, 实际渲染在 DataPanel"
);

fs.writeFileSync('src/components/nodes/WidgetNode.tsx', c);
console.log('done');
