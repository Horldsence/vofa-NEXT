//! 应用更新 store — 自动/手动检查更新的前端状态机
//!
//! 状态机: idle → checking → up-to-date / available → downloading → ready (待重启)
//!        任意阶段失败 → error
//! 后端契约:
//! - invoke('check_update', { channel }) → { available, currentVersion, version, notes, date }
//! - invoke('download_and_install_update') → void, 期间 emit update://progress, 完成 emit update://ready
//! 取代原 useUpdater hook (React 局部状态无法支撑状态栏/弹窗/设置三处共享)

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { relaunch as processRelaunch } from '@tauri-apps/plugin-process';
import { useSettingsStore } from './settingsStore';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'error';

export type UpdateChannel = 'stable' | 'beta';

export interface UpdateInfo {
  version: string;
  notes: string;
  date: string;
}

/// 后端 check_update 响应
interface CheckUpdateResult {
  available: boolean;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  date: string | null;
}

interface UpdateStore {
  status: UpdateStatus;
  /// 最近一次检查的来源 — 状态栏只对 manual 结果显示"已是最新"/错误图标
  lastTrigger: 'auto' | 'manual';
  updateInfo: UpdateInfo | null;
  currentVersion: string;
  /// 下载进度 0-100 (total 未知时保持上次的确定值)
  progress: number;
  error: string | null;
  dialogOpen: boolean;

  check: (trigger: 'auto' | 'manual') => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  relaunch: () => Promise<void>;
  skipVersion: () => void;
  setChannel: (channel: UpdateChannel) => void;
  openDialog: () => void;
  closeDialog: () => void;
}

/// 解析更新通道: 显式设置优先; 否则按当前版本推导 (含 '-' 视为预发布 → beta)
/// getVersion 失败 (非 Tauri 环境) 时回退 stable
export async function resolveUpdateChannel(): Promise<UpdateChannel> {
  const explicit = useSettingsStore.getState().settings.general.updateChannel;
  if (explicit) return explicit;
  try {
    const version = await getVersion();
    return version.includes('-') ? 'beta' : 'stable';
  } catch {
    return 'stable';
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const useUpdateStore = create<UpdateStore>((set, get) => ({
  status: 'idle',
  lastTrigger: 'manual',
  updateInfo: null,
  currentVersion: '',
  progress: 0,
  error: null,
  dialogOpen: false,

  check: async (trigger) => {
    set({ status: 'checking', lastTrigger: trigger, error: null });
    try {
      const channel = await resolveUpdateChannel();
      const res = await invoke<CheckUpdateResult>('check_update', { channel });
      if (res.available && res.version) {
        set({
          status: 'available',
          updateInfo: {
            version: res.version,
            notes: res.notes ?? '',
            date: res.date ?? '',
          },
          currentVersion: res.currentVersion,
        });
        // 自动检查只在用户未跳过该版本时打扰用户; 手动检查不自动弹窗
        const skipped = useSettingsStore.getState().settings.general.skippedUpdateVersion;
        if (trigger === 'auto' && res.version !== skipped) {
          set({ dialogOpen: true });
        }
      } else {
        set({ status: 'up-to-date', updateInfo: null, currentVersion: res.currentVersion });
      }
    } catch (e) {
      // auto 失败不打断用户 — 仅记录状态, UI 层对 auto 失败保持静默
      set({ status: 'error', error: errorMessage(e) });
    }
  },

  downloadAndInstall: async () => {
    set({ status: 'downloading', progress: 0, error: null });
    let unlisten: (() => void) | null = null;
    try {
      unlisten = await listen<{ received: number; total: number | null }>(
        'update://progress',
        (event) => {
          const { received, total } = event.payload;
          // total 未知时保持不确定态 (进度条停在原值)
          if (total && total > 0) {
            set({ progress: Math.min(100, Math.round((received / total) * 100)) });
          }
        }
      );
      await invoke('download_and_install_update');
      // invoke resolve 与 update://ready 事件二者取先到 — 此处兜底
      if (get().status === 'downloading') {
        set({ status: 'ready', progress: 100 });
      }
    } catch (e) {
      set({ status: 'error', error: errorMessage(e) });
    } finally {
      unlisten?.();
    }
  },

  relaunch: async () => {
    await processRelaunch();
  },

  skipVersion: () => {
    const info = get().updateInfo;
    if (info) {
      useSettingsStore.getState().update('general', 'skippedUpdateVersion', info.version);
    }
    set({ dialogOpen: false, status: 'idle' });
  },

  setChannel: (channel) => {
    useSettingsStore.getState().update('general', 'updateChannel', channel);
    void get().check('manual');
  },

  openDialog: () => set({ dialogOpen: true }),
  closeDialog: () => set({ dialogOpen: false }),
}));

/// 后端安装完成事件 — 模块加载时注册一次 (store 与应用同生命周期, 无需清理)
void listen('update://ready', () => {
  if (useUpdateStore.getState().status === 'downloading') {
    useUpdateStore.setState({ status: 'ready', progress: 100 });
  }
});
