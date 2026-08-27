# 操作历史 — 快照式撤销栈与时间线面板

画布侧的撤销 / 重做 / 任意点回溯 (`src/store/historyStore.ts`)。撤销栈为**快照模型**:
每条记录保存操作完成后的整份文档快照, `index` 指向当前生效条目; 撤销、重做与
面板任意点跳转统一为「移动 index 并应用该条目快照」, 因此历史面板可以像
Photoshop 一样点击任意一条直接回到那个时刻。

```
用户操作 → withHistoryOp 包裹的 action
              │ beginHistoryOp (惰性建基线)
              ▼
        commitHistoryOp
        ├─ 连续手势合并 (coalesceKey + 700ms 窗口)
        ├─ 与栈顶活引用一致 → 视为无变更不入栈
        └─ index 之后截断 (丢弃 redo 分支)
              │ captureDoc: data 深拷贝 + refs 活引用
              ▼
   useHistoryStore { entries[], index }   ──▶   OperationHistoryView 时间线
```

## 记录范围

- 埋点方式: 各 store 切片的变更 action 通过 `withHistoryOp` 包裹
  (`graph` / `widgets` / `controlTabs` / `protocol` 四个切片),
  异步流程用 `beginHistoryOp` + `commitHistoryOp` 成对调用。
- 未埋点的写入 (后端事件推送 / 启动种子图) 天然不入栈;
- 导入备份 / 应用模板走 `rebaseHistory`, 以导入结果为新基线,
  基线条目文案随 `baselineOpKey` 区分 (初始状态 / 导入工作区 / 应用模板)。
- 栈深上限 200 条, 超出丢最旧 (基线允许被挤出, 语义仍是「最早可回退点」);
  会话内有效, 不持久化。

## 快照双轨

每份快照存两轨:

| 字段 | 内容 | 用途 |
|---|---|---|
| `data` | 全部 tracked 切片的深拷贝 | 恢复用存档, 与 store 脱钩 |
| `refs` | 捕获时刻各切片的活引用 | 等值判断 (克隆体引用永远互不相同) |

tracked 切片: `rfNodes` / `rfEdges` / `widgets` / `controlTabs` /
`activeControlTabId` / `dataTabs` / `activeDataTabId`。
「本次变更是否真的动了文档」用 refs 判定; 深拷贝保证 undo 存档不受后续原地改写污染。

## 恢复副作用

`restoreSnapshot` 全量替换切片后必须:

1. `removeDerived` 清掉消失全局节点的派生端口表;
2. 尽力关闭消失 Transport 的连接 (`api.closeTransport`);
3. `syncAllTabGraphs` 让后端按恢复后的图重建派生端口与求值引擎。

独立查看面板 (无 widgetId 的 data tab: 操作历史 / CAN / 逻辑 / 固定编译页)
属于 UI 态而非文档态 — 回滚文档时保持打开且不被切换, 否则用户正看着的历史面板
会在跳回旧快照的瞬间被一并"退没"。

## 时间线面板 (`OperationHistoryView`)

- **入口**: 数据卡片标题栏「＋」→「打开操作历史」; 打开面板即建立基线快照 (无需等首次操作)。
- **列表**: 最新在上, 行首为「步骤号 + 节点主题徽章」— 徽章视觉与画布节点同源
  (`nodeKindVisuals`: 控件按分类色, Transport 黄 · Cable, Protocol 主题色 · Binary),
  连线类条目渲染「源色点 → 目标色点」双端点配色; 时间戳常显。
- **游标语义**: 当前生效条目高亮; 其上方的灰显分区是已被撤销的未来, 中间由
  「已撤销 · 可重做」分隔条标记游标位置。点击任意一条直接跳转到该时刻
  (快照式回滚), 之后的新操作会丢弃其后方分支。
- **头部工具栏**: 步骤进度徽章、撤销 (`Ctrl+Z`) / 重做 (`Ctrl+Y`) 按钮
  (与菜单栏「编辑」项同走 `useHistoryStore`)、两段式确认清空 (3 秒未二次点击自动复位)。

## 新手引导集成

引导向导的「操作历史 · 撤销与回溯」步骤 (`OnboardingWizard.tsx`) 为实操门控步骤:

- `prepare`: `openDataPanelAndReveal(() => addOperationHistoryTab())` 自动打开面板并聚焦;
- 锚点: 面板根节点 `data-tour="operation-history"`;
- 门控: document 捕获级 click 监听命中 `[data-tour="operation-history"]`
  即通过 — 点击任意历史记录跳转或点撤销按钮均算完成, 打勾后自动前进。

步骤顺序位于「编译结果 · 动手删除一条连接」之后 — 用户刚完成一次破坏性编辑,
正是演示「记录在案, 一键回溯」的最佳时机。
