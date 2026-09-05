import { api } from '../../lib/tauri/tauri';
import type { RunAction, RunState } from '../../types';
import type { AppSlice } from './types';

/// 工作区运行控制 — 启停/暂停状态与动作
///
/// 状态权威在 Rust (ExecutionControl): 本切片保存 UI 镜像,
/// `workspace:run` 事件 (含其他写入方的切换) 是收敛来源;
/// 动作调用以命令返回值为准, 事件只做幂等刷新。
export interface RunControlSlice {
  runState: RunState;
  setRunState: (state: RunState) => void;
  /// start 从停止启动 / 从暂停恢复; pause 停止处理并保持连接; stop 清空执行状态
  workspaceRun: (action: RunAction) => Promise<void>;
  /// 启动水合 — 读取后端当前运行快照 (新建/重开项目默认 stopped)
  hydrateRunState: () => Promise<void>;
}

export const createRunControlSlice: AppSlice<RunControlSlice> = (set) => ({
  runState: 'stopped',
  setRunState: (state) => set({ runState: state }),
  workspaceRun: async (action) => {
    try {
      const snapshot = await api.workspaceRun(action);
      set({ runState: snapshot.state });
    } catch (error) {
      console.warn('workspace_run 失败:', error);
    }
  },
  hydrateRunState: async () => {
    try {
      const snapshot = await api.getWorkspaceRunState();
      set({ runState: snapshot.state });
    } catch {
      // 纯浏览器 dev / 测试环境无 Tauri 后端 — 保持默认 stopped
    }
  },
});
