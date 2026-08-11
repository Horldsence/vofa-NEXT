//! 应用更新 — 基于 GitHub Release (tauri-plugin-updater)
//!
//! 状态机: idle → checking → up-to-date / available → downloading → ready (待重启)
//! 由设置弹窗的"检查更新"按钮驱动。

import { useCallback, useRef, useState } from 'react';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdateState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'up-to-date' }
  | { status: 'available'; version: string; notes: string }
  | { status: 'downloading'; percent: number }
  | { status: 'ready' }
  | { status: 'error'; message: string };

export function useUpdater() {
  const [state, setState] = useState<UpdateState>({ status: 'idle' });
  const updateRef = useRef<Update | null>(null);

  const checkUpdate = useCallback(async () => {
    setState({ status: 'checking' });
    try {
      const update = await check();
      if (update) {
        updateRef.current = update;
        setState({ status: 'available', version: update.version, notes: update.body ?? '' });
      } else {
        updateRef.current = null;
        setState({ status: 'up-to-date' });
      }
    } catch (e) {
      setState({ status: 'error', message: String(e) });
    }
  }, []);

  const install = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setState({ status: 'downloading', percent: 0 });
    try {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
        } else if (event.event === 'Progress') {
          done += event.data.chunkLength;
          const percent = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
          setState({ status: 'downloading', percent });
        }
      });
      setState({ status: 'ready' });
    } catch (e) {
      setState({ status: 'error', message: String(e) });
    }
  }, []);

  const restart = useCallback(async () => {
    await relaunch();
  }, []);

  return { state, checkUpdate, install, restart };
}
