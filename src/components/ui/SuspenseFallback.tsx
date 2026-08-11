//! 共享 Suspense 加载占位 — React.lazy 懒加载模块加载期间的统一视觉
//!
//! - 默认: 填满父容器 (数据 Tab 切换等场景), 复用 module-card + animate-spin
//! - overlay: 弹窗场景, 使用全屏遮罩 (fixed inset-0 + bg-bg-overlay), 与现有弹窗遮罩一致
interface SuspenseFallbackProps {
  /** 作为弹窗加载占位时使用全屏遮罩 */
  overlay?: boolean;
}

export function SuspenseFallback({ overlay = false }: SuspenseFallbackProps) {
  const spinner = (
    <div className="flex items-center justify-center">
      <div className="h-4 w-4 rounded-full border-2 border-border border-t-accent animate-spin" />
    </div>
  );

  if (overlay) {
    return (
      <div className="fixed inset-0 bg-bg-overlay z-modal flex items-center justify-center">
        <div className="module-card bg-bg-sidebar px-6 py-5 flex items-center justify-center">
          {spinner}
        </div>
      </div>
    );
  }

  return (
    <div className="module-card h-full w-full bg-bg-sidebar flex items-center justify-center">
      {spinner}
    </div>
  );
}
