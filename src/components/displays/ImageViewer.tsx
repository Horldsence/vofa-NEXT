import { useState } from 'react';
import { Image as ImageIcon } from 'lucide-react';
import { WidgetCard } from '../ui/WidgetCard';
import type { WidgetConfig } from '../../types';

interface ImageViewerProps {
  widget: Extract<WidgetConfig, { kind: 'Image' }>;
  onRemove: () => void;
  /// true = DataPanel 全尺寸渲染 (左右双栏); false = WidgetNode 紧凑渲染
  full?: boolean;
}

/// 图像控件 — 占位实现, 后续可扩展为图像数据流显示
/// full 模式: 图像区铺满主区 + 信息侧栏 (固定 200px)
/// 紧凑模式: 单个 aspect-ratio 容器 (节点编辑器内)
export function ImageViewer({ widget, full = false }: ImageViewerProps) {
  const { width, height, format } = widget.params;
  const [hasImage] = useState(false);

  const infoBlock = (
    <>
      <span>
        {width}×{height}
      </span>
      <span className="text-text-secondary">{format}</span>
    </>
  );

  if (full) {
    // DataPanel 全尺寸: 左右双栏
    return (
      <div className="group bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
        {/* 主区: 图像区铺满, 保持 aspect-ratio 居中 */}
        <div className="flex-1 min-w-0 min-h-0 relative bg-black flex items-center justify-center p-4">
          <div
            className="bg-black border border-border rounded flex items-center justify-center text-text-secondary text-xs"
            style={{
              aspectRatio: `${width} / ${height}`,
              maxWidth: '100%',
              maxHeight: '100%',
              width: width > height ? '100%' : 'auto',
              height: height > width ? '100%' : 'auto',
            }}
          >
            {hasImage ? (
              <canvas width={width} height={height} />
            ) : (
              <div className="flex flex-col gap-1 items-center">
                <ImageIcon size={32} className="opacity-40" />
                {infoBlock}
              </div>
            )}
          </div>
        </div>
        {/* 侧栏: 图像信息 */}
        <div className="w-[200px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-2.5">
          <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold px-1">Image</div>
          <div className="flex flex-col gap-1 bg-bg-input border border-border rounded-sm px-2 py-1.5">
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Width</span>
              <span className="text-text-bright font-mono">{width}px</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Height</span>
              <span className="text-text-bright font-mono">{height}px</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Format</span>
              <span className="text-text-bright font-mono">{format}</span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // 紧凑模式: 节点编辑器内
  return (
    <WidgetCard>
      <div
        className="w-full bg-black border border-border rounded flex items-center justify-center text-text-secondary text-xs"
        style={{ aspectRatio: `${width} / ${height}` }}
      >
        {hasImage ? (
          <canvas width={width} height={height} />
        ) : (
          <div className="flex flex-col gap-1 items-center">
            <ImageIcon size={24} className="opacity-40" />
            <span>
              {width}×{height} {format}
            </span>
          </div>
        )}
      </div>
    </WidgetCard>
  );
}
