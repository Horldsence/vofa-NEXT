import { useState } from 'react';
import { Image as ImageIcon } from 'lucide-react';
import type { WidgetConfig } from '../../types';

interface ImageViewerProps {
  widget: Extract<WidgetConfig, { kind: 'Image' }>;
  onRemove: () => void;
}

/// 图像控件 — 占位实现, 后续可扩展为图像数据流显示
export function ImageViewer({ widget }: ImageViewerProps) {
  const { width, height, format } = widget.params;
  const [hasImage] = useState(false);

  return (
    <div className="widget-card">
      <div
        style={{
          width: '100%',
          aspectRatio: `${width} / ${height}`,
          background: '#000',
          border: '1px solid var(--border)',
          borderRadius: 4,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'var(--text-secondary)',
          fontSize: 11,
        }}
      >
        {hasImage ? (
          <canvas width={width} height={height} />
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4, alignItems: 'center' }}>
            <ImageIcon size={24} style={{ opacity: 0.4 }} />
            <span>
              {width}×{height} {format}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
