//! 快速开始面板 — 内置模板一键应用 + 导入模板文件
//!
//! 模板包含节点图 + 窗口组织 + 传输/协议配置 (不含用户设置)。
//! 应用方式通过系统 (Tauri) 对话框确认: 替换当前工作区 / 合并为新标签页。
//!
//! 注: 「打开数据面板」入口已迁出到独立 Sidebar view (`panels`), 见
//! DataPanelsPanel.tsx。

import {
  Sigma,
  Waves,
  Cpu,
  Usb,
  Sparkles,
  Rocket,
  Upload,
  type LucideIcon,
} from 'lucide-react';
import { ask, confirm } from '@tauri-apps/plugin-dialog';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { QUICK_START_TEMPLATES, type QuickStartTemplate } from '../../lib/quickstart/templates';
import { applyTemplate, type TemplateApplyMode } from '../../lib/quickstart/applyTemplate';
import { readSnapshotFromFile, type AppSnapshot } from '../../lib/tauri/appExport';

/// 模板图标映射
const TEMPLATE_ICONS: Record<string, LucideIcon> = {
  math: Sigma,
  filter: Waves,
  can: Cpu,
  serial: Usb,
  demo: Sparkles,
};

export function QuickStartPanel() {
  const lang = useAppStore((s) => s.lang);
  const setSidebarView = useAppStore((s) => s.setSidebarView);

  /// 应用快照并切换到控件画布视图
  const apply = async (snap: AppSnapshot, mode: TemplateApplyMode) => {
    await applyTemplate(snap, mode);
    setSidebarView('widgets');
  };

  /// 通过系统对话框确认应用方式: 先询问「替换」(推荐), 取消则再询问「合并」
  const chooseAndApply = async (snap: AppSnapshot, name: string) => {
    const title = t(lang, 'templateApplyTitle');
    const replace = await confirm(
      `${name}\n\n${t(lang, 'templateApplyReplaceConfirm')}`,
      { title, kind: 'warning' }
    );
    if (replace) {
      await apply(snap, 'replace');
      return;
    }
    const merge = await ask(
      `${name}\n\n${t(lang, 'templateApplyMergePrompt')}`,
      { title, kind: 'info' }
    );
    if (merge) await apply(snap, 'merge');
  };

  const requestApply = (tpl: QuickStartTemplate) => {
    void chooseAndApply(tpl.build(), t(lang, tpl.nameKey));
  };

  const requestImport = async () => {
    const snap = await readSnapshotFromFile();
    if (!snap) return;
    const name = snap.controlTabs?.[0]?.name ?? t(lang, 'templateImportName');
    await chooseAndApply(snap, name);
  };

  return (
    <div className="flex flex-col h-full overflow-hidden gap-3" data-tour="quickstart-panel">
      <div className="text-xs text-text-secondary leading-relaxed">
        {t(lang, 'quickStartDesc')}
      </div>

      {/* 模板列表 */}
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 -m-1 p-1">
        {QUICK_START_TEMPLATES.map((tpl) => {
          const Icon = TEMPLATE_ICONS[tpl.id] ?? Rocket;
          return (
            <div
              key={tpl.id}
              className="group bg-bg-input border border-border-subtle rounded-md p-2.5 flex items-start gap-2.5 transition-all duration-150 hover:bg-bg-hover hover:border-accent/50"
            >
              <div className="w-9 h-9 rounded-sm flex items-center justify-center bg-accent/15 text-accent flex-shrink-0 [&_svg]:w-4 [&_svg]:h-4">
                <Icon size={16} />
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-text-bright">{t(lang, tpl.nameKey)}</div>
                <div className="text-[11px] text-text-secondary leading-snug mt-0.5">
                  {t(lang, tpl.descKey)}
                </div>
              </div>
              <button
                className="shrink-0 px-2.5 h-7 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs transition-colors hover:bg-bg-button-hover"
                onClick={() => requestApply(tpl)}
              >
                {t(lang, 'templateApply')}
              </button>
            </div>
          );
        })}
      </div>

      {/* 导入模板文件 */}
      <button
        className="w-full flex items-center justify-center gap-2 px-3 h-9 bg-bg-input border border-dashed border-border rounded-md text-text-secondary text-sm cursor-pointer transition-colors hover:text-text-bright hover:border-accent/50"
        onClick={() => void requestImport()}
      >
        <Upload size={14} />
        <span>{t(lang, 'templateImportFile')}</span>
      </button>
    </div>
  );
}
