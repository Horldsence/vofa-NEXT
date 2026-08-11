//! 共享连接 action — 7 种传输表单复用同一份 connect 提交流程
import { useActionState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';

export interface ConnectActionState {
  ok: boolean;
  error?: string;
}

const INITIAL_STATE: ConnectActionState = { ok: true };

/// 连接 action — 从 store 读取当前 transportConfig 并调用 connect()
///
/// 注意: store 的 connect() 内部捕获异常 (不抛错), 失败时置 connectionState = 'Error',
/// 因此这里通过连接状态判断成功与否, 并复用 i18n 的 notifConnectFailed 错误文案。
export function useConnectAction() {
  const connect = useAppStore((s) => s.connect);

  const [state, formAction, isPending] = useActionState<ConnectActionState>(
    async () => {
      await connect();
      const { connectionState, lang } = useAppStore.getState();
      if (connectionState === 'Error') {
        return { ok: false, error: t(lang, 'notifConnectFailed') };
      }
      return { ok: true };
    },
    INITIAL_STATE
  );

  return { state, formAction, isPending };
}
