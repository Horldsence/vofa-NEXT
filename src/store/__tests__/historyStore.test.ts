import { describe, expect, it, vi, beforeEach } from 'vitest';

// 与 revealDataTab.test 同因: jsdom 环境无 localStorage, dockStore 的 persist
// 中间件在 setState 时会写入 storage — 导入 store 前提供内存桩
vi.hoisted(() => {
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    clear: () => store.clear(),
    key: (index: number) => [...store.keys()][index] ?? null,
    get length() {
      return store.size;
    },
  };
  const g = globalThis as { localStorage?: unknown };
  g.localStorage = localStorageMock;
});

import { useAppStore } from '../appStore';
import { useHistoryStore } from '../historyStore';
import { createWidget } from '../../lib/utils/createWidget';
import type { DataTab } from '../../types';

const FIXED_DATA_TABS: DataTab[] = [
  { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
  { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
];

function resetStores(): void {
  useAppStore.setState({
    rfNodes: [],
    rfEdges: [],
    widgets: [],
    controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
    activeControlTabId: 'default',
    dataTabs: FIXED_DATA_TABS.map((t) => ({ ...t })),
    activeDataTabId: FIXED_DATA_TABS[1].id,
  });
  // 清空历史 (同时复位合并窗口簿记), 并以当前干净状态为基线
  useHistoryStore.getState().clearHistory();
}

describe('historyStore 撤销/重做核心', () => {
  beforeEach(resetStores);

  it('基线建好后无可撤销操作, undo 为空操作', () => {
    const st = useHistoryStore.getState();
    expect(st.entries).toHaveLength(1);
    expect(st.index).toBe(0);
    expect(st.canUndo).toBe(false);

    st.undo();
    expect(useHistoryStore.getState().index).toBe(0);
  });

  it('添加控件入栈 → 撤销完整还原 (含自动创建的数据 Tab) → 重做再应用', () => {
    const app = () => useAppStore.getState();
    const tabsBefore = app().dataTabs.length;

    app().addWidget(createWidget('Waveform'), 'default');

    expect(app().widgets).toHaveLength(1);
    // Waveform 有数据窗口 Tab — addWidget 自动追加
    expect(app().dataTabs.length).toBe(tabsBefore + 1);

    let hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(2);
    expect(hs.index).toBe(1);
    expect(hs.canUndo).toBe(true);
    expect(hs.entries[1].opKey).toBe('opAddWidget');
    expect(hs.entries[1].detailKey).toBe('dataTabWaveform');

    hs.undo();

    const afterUndo = useHistoryStore.getState();
    expect(afterUndo.index).toBe(0);
    expect(afterUndo.canUndo).toBe(false);
    expect(afterUndo.canRedo).toBe(true);
    expect(app().widgets).toHaveLength(0);
    expect(app().rfNodes).toHaveLength(0);
    expect(app().dataTabs.length).toBe(tabsBefore);

    afterUndo.redo();

    const afterRedo = useHistoryStore.getState();
    expect(afterRedo.index).toBe(1);
    expect(afterRedo.canRedo).toBe(false);
    expect(app().widgets).toHaveLength(1);
    expect(app().dataTabs.length).toBe(tabsBefore + 1);
  });

  it('同控件连续参数更新在时间窗内合并为一条', () => {
    const app = () => useAppStore.getState();

    app().addWidget(createWidget('Knob'), 'default');
    expect(useHistoryStore.getState().entries).toHaveLength(2);

    const id = app().widgets[0].params.id;
    const w = app().widgets[0];
    // Knob 的 default 字段在控件联合类型上需局部收窄
    const defaultValueOf = (widget: typeof w) =>
      (widget.params as unknown as Record<string, unknown>).default as number;

    app().updateWidget(id, {
      ...w,
      params: { ...w.params, default: 10 },
    } as typeof w);
    let hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(3);
    expect(hs.entries[2].opKey).toBe('opUpdateWidgetParams');

    app().updateWidget(id, {
      ...(app().widgets[0]),
      params: { ...w.params, default: 20 },
    } as typeof w);
    // 合并进栈顶而非新增
    hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(3);
    expect(defaultValueOf(app().widgets[0])).toBe(20);

    // 一次 undo 即回滚整个手势到「参数更新前」
    hs.undo();
    expect(defaultValueOf(app().widgets[0])).toBe(defaultValueOf(w));

    // 对应的一次 redo 回到手势末态
    useHistoryStore.getState().redo();
    expect(defaultValueOf(app().widgets[0])).toBe(20);
  });

  it('跳转任意历史点后提交新操作丢弃其后的分支', () => {
    const app = () => useAppStore.getState();

    app().addWidget(createWidget('Knob'), 'default');
    app().renameControlTab('default', 'Renamed');
    let hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(3);
    const jumpTargetId = hs.entries[1].id;

    hs.jumpTo(jumpTargetId);
    hs = useHistoryStore.getState();
    expect(hs.index).toBe(1);
    expect(app().controlTabs[0].name).toBe('Tab 1'); // 重命名被回滚

    // 在过去位置产生新操作 → 未来分支 (重命名) 被丢弃
    app().renameControlTab('default', 'New Branch');
    hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(3);
    expect(hs.canRedo).toBe(false);
    expect(hs.entries[2].opKey).toBe('opRenameControlTab');
    expect(hs.entries[2].detailText).toBe('New Branch');
  });

  it('清空历史回到仅含基线的单条状态', () => {
    const app = () => useAppStore.getState();
    app().addWidget(createWidget('Knob'), 'default');
    app().addTransportNode('TestData');
    expect(useHistoryStore.getState().entries.length).toBeGreaterThanOrEqual(3);

    useHistoryStore.getState().clearHistory();

    const hs = useHistoryStore.getState();
    expect(hs.entries).toHaveLength(1);
    expect(hs.index).toBe(0);
    expect(hs.canUndo).toBe(false);
    expect(hs.canRedo).toBe(false);
    // 文档态不受清空影响
    expect(app().widgets).toHaveLength(1);
  });

  it('未触及文档切片的变更不产生历史条目 (快照相等跳过)', () => {
    const app = () => useAppStore.getState();
    app().addWidget(createWidget('Knob'), 'default');
    const lenAfterAdd = useHistoryStore.getState().entries.length;

    // 仅打开自定义控件编辑器 (widgetId 已存在) — 只改 customEditorState, 不入历史
    app().openCustomEditor(app().widgets[0].params.id);
    expect(useHistoryStore.getState().entries.length).toBe(lenAfterAdd);
  });

  it('undo 不关闭回滚之后才打开的独立面板 (操作历史 / CAN)', () => {
    const app = () => useAppStore.getState();

    // 1) 基线在此操作之前捕获 — 此时还不存在独立查看面板
    app().addWidget(createWidget('Knob'), 'default');

    // 2) 之后才打开 CAN 帧 + 操作历史面板
    app().addCanTab();
    app().addOperationHistoryTab();
    const historyTabId = app().dataTabs.find((t) => t.type === 'operation-history')!.id;
    expect(historyTabId).toBeTruthy();
    expect(app().dataTabs.some((t) => t.type === 'can')).toBe(true);

    // 3) undo 回滚文档 (控件消失), 但独立面板保持打开且视图不被切走
    useHistoryStore.getState().undo();

    const after = useAppStore.getState();
    expect(after.widgets).toHaveLength(0);
    expect(after.rfNodes).toHaveLength(0);
    expect(after.dataTabs.some((t) => t.id === historyTabId)).toBe(true);
    expect(after.dataTabs.some((t) => t.type === 'can')).toBe(true);
    // 恢复前激活的是独立面板 → 恢复后保持它, 用户可继续点击重做
    expect(after.activeDataTabId).toBe(historyTabId);

    // 4) redo 把控件找回来, 独立面板依旧存在
    useHistoryStore.getState().redo();
    const afterRedo = useAppStore.getState();
    expect(afterRedo.widgets).toHaveLength(1);
    expect(afterRedo.dataTabs.some((t) => t.id === historyTabId)).toBe(true);
  });
});
