import clsx from 'clsx';

/// 选项芯片样式 — 选中态实心主题蓝, 未选中中性灰
/// 供频谱 / 3D 模型等控件的设置面板共用, 保证选中态视觉统一
export const chipClass = (active: boolean) =>
  clsx(
    'px-1.5 py-0.5 border rounded-sm text-[10px] cursor-pointer transition-colors',
    active
      ? 'bg-accent border-accent text-text-inverse font-semibold'
      : 'bg-bg-input border-border text-text-secondary hover:border-accent/60 hover:text-text-primary',
  );
