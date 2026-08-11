import { memo, useEffect, useMemo, type CSSProperties } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { useAppStore } from '../../store/appStore';
import { useDockStore, edgeDir, removeCardNode, type DockNode, type DropTarget } from '../../store/dockStore';
import { DockCardFrame } from './DockCardFrame';
import type { SnapEdge } from '../ui/SnapDropOverlay';

/// 百分比矩形 (相对于中央区容器)
interface PctRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const ROOT_EDGE_SHARE = 25;

/// 计算投放预览区域 — 与 dockStore 的插入逻辑保持一致:
/// - 页面级热区 (cardId=null): 整行/整列条带 (25%)
/// - 父 split 轴向匹配: 跨越父区域的条带, 覆盖 target 靠边缘的一半
/// - 否则: 目标卡片自身的一半
function computePreviewRect(root: DockNode, target: DropTarget): PctRect | null {
  if (target.cardId === null) {
    switch (target.edge) {
      case 'top':
        return { x: 0, y: 0, w: 100, h: ROOT_EDGE_SHARE };
      case 'bottom':
        return { x: 0, y: 100 - ROOT_EDGE_SHARE, w: 100, h: ROOT_EDGE_SHARE };
      case 'left':
        return { x: 0, y: 0, w: ROOT_EDGE_SHARE, h: 100 };
      case 'right':
        return { x: 100 - ROOT_EDGE_SHARE, y: 0, w: ROOT_EDGE_SHARE, h: 100 };
    }
  }
  return findCardPreview(root, { x: 0, y: 0, w: 100, h: 100 }, target, null);
}

function findCardPreview(
  node: DockNode,
  rect: PctRect,
  target: DropTarget,
  parent: { dir: 'row' | 'col'; rect: PctRect } | null
): PctRect | null {
  const edge = target.edge as SnapEdge;
  if (node.type === 'card') {
    if (node.cardId !== target.cardId) return null;
    if (parent && parent.dir === edgeDir(edge)) {
      // 同级条带: 跨父 split 全宽/高
      switch (edge) {
        case 'top':
          return { x: parent.rect.x, y: rect.y, w: parent.rect.w, h: rect.h / 2 };
        case 'bottom':
          return { x: parent.rect.x, y: rect.y + rect.h / 2, w: parent.rect.w, h: rect.h / 2 };
        case 'left':
          return { x: rect.x, y: parent.rect.y, w: rect.w / 2, h: parent.rect.h };
        case 'right':
          return { x: rect.x + rect.w / 2, y: parent.rect.y, w: rect.w / 2, h: parent.rect.h };
      }
    }
    // 拆分目标卡片自身
    switch (edge) {
      case 'top':
        return { ...rect, h: rect.h / 2 };
      case 'bottom':
        return { x: rect.x, y: rect.y + rect.h / 2, w: rect.w, h: rect.h / 2 };
      case 'left':
        return { ...rect, w: rect.w / 2 };
      case 'right':
        return { x: rect.x + rect.w / 2, y: rect.y, w: rect.w / 2, h: rect.h };
    }
  }
  // split — 按 sizes 占比分配子矩形
  const total = node.sizes.reduce((a, b) => a + b, 0) || 100;
  let offset = 0;
  for (let i = 0; i < node.children.length; i++) {
    const share = (node.sizes[i] ?? 100 / node.children.length) / total;
    const childRect =
      node.dir === 'row'
        ? { x: rect.x + offset * rect.w, y: rect.y, w: share * rect.w, h: rect.h }
        : { x: rect.x, y: rect.y + offset * rect.h, w: rect.w, h: share * rect.h };
    offset += share;
    const found = findCardPreview(node.children[i], childRect, target, { dir: node.dir, rect });
    if (found) return found;
  }
  return null;
}

function previewStyle(r: PctRect): CSSProperties {
  return {
    left: `calc(${r.x}% + 4px)`,
    top: `calc(${r.y}% + 4px)`,
    width: `calc(${r.w}% - 8px)`,
    height: `calc(${r.h}% - 8px)`,
  };
}

/// 页面边缘热区样式 (中央区四边)
const HOT_ZONE_CLASS: Record<SnapEdge, string> = {
  top: 'absolute top-0 left-0 right-0 h-7 z-40',
  bottom: 'absolute bottom-0 left-0 right-0 h-7 z-40',
  left: 'absolute left-0 top-0 bottom-0 w-7 z-40',
  right: 'absolute right-0 top-0 bottom-0 w-7 z-40',
};

/// Dock 布局 — 递归渲染布局树 (split → PanelGroup, card → DockCardFrame)
/// - 与 appStore 的 Tab 列表对账 (新增安置 / 删除剔除 / 空卡片裁剪)
/// - 全局投放预览 (几何与 dockStore 插入逻辑一致)
/// - 页面四边热区 (整行/整列条带停靠)
export const DockLayout = memo(function DockLayout() {
  const root = useDockStore((s) => s.root);
  const dropTarget = useDockStore((s) => s.dropTarget);
  const draggingTab = useDockStore((s) => s.draggingTab);
  const draggingCardId = useDockStore((s) => s.draggingCardId);
  const setDropTarget = useDockStore((s) => s.setDropTarget);
  const dropOnRootEdge = useDockStore((s) => s.dropOnRootEdge);
  const controlTabs = useAppStore((s) => s.controlTabs);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const reconcile = useDockStore((s) => s.reconcile);

  useEffect(() => {
    reconcile('control', controlTabs.map((tab) => tab.id));
  }, [controlTabs, reconcile]);

  useEffect(() => {
    reconcile('data', dataTabs.map((tab) => tab.id));
  }, [dataTabs, reconcile]);

  const dragging = draggingTab !== null || draggingCardId !== null;

  // 预览几何基于"摘除被拖卡片后的树"计算, 与落点后的实际布局一致
  // (整卡拖拽 → 摘除该卡; Tab 拖拽且源卡片将变空 → 摘除源卡片)
  const cards = useDockStore((s) => s.cards);
  const previewRoot = useMemo(() => {
    if (draggingCardId) return removeCardNode(root, draggingCardId) ?? root;
    if (draggingTab) {
      const origin = cards[draggingTab.fromCardId];
      if (origin && origin.tabIds.length <= 1) return removeCardNode(root, origin.id) ?? root;
    }
    return root;
  }, [root, cards, draggingTab, draggingCardId]);

  const preview = dragging && dropTarget ? computePreviewRect(previewRoot, dropTarget) : null;

  return (
    <div className="relative h-full w-full">
      <DockNodeView node={root} />

      {/* 页面四边热区 */}
      {dragging &&
        (['top', 'bottom', 'left', 'right'] as const).map((edge) => (
          <div
            key={edge}
            className={HOT_ZONE_CLASS[edge]}
            onDragOver={(e) => {
              e.preventDefault();
              e.stopPropagation();
              e.dataTransfer.dropEffect = 'move';
              setDropTarget({ cardId: null, edge });
            }}
            onDragLeave={() => setDropTarget(null)}
            onDrop={(e) => {
              e.preventDefault();
              e.stopPropagation();
              dropOnRootEdge(edge);
            }}
          />
        ))}

      {/* 投放预览 */}
      {preview && <div className="snap-drop-zone" style={previewStyle(preview)} />}
    </div>
  );
});

function DockNodeView({ node }: { node: DockNode }) {
  const setSizes = useDockStore((s) => s.setSizes);

  if (node.type === 'card') {
    return <DockCardFrame cardId={node.cardId} />;
  }

  const horizontal = node.dir === 'row';
  return (
    <PanelGroup
      direction={horizontal ? 'horizontal' : 'vertical'}
      className="gap-1"
      onLayout={(sizes) => {
        if (
          sizes.length === node.children.length &&
          sizes.some((s, i) => Math.abs(s - (node.sizes[i] ?? 0)) > 0.01)
        ) {
          setSizes(node.id, sizes);
        }
      }}
    >
      {node.children.flatMap((child, i) => {
        const panel = (
          <Panel
            key={child.id}
            defaultSize={node.sizes[i] ?? 100 / node.children.length}
            minSize={10}
            className="min-w-0 min-h-0"
          >
            <DockNodeView node={child} />
          </Panel>
        );
        if (i === 0) return [panel];
        return [
          <PanelResizeHandle
            key={`handle-${child.id}`}
            className={`${horizontal ? 'w-1' : 'h-1'} rounded-full bg-transparent hover:bg-accent/50 transition-colors`}
          />,
          panel,
        ];
      })}
    </PanelGroup>
  );
}
