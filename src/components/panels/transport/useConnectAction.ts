//! 共享连接 action — 全局 Transport 节点属性面板的 connect 提交流程
import { useActionState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';

export interface ConnectActionState {
  ok: boolean;
  error?: string;
}

const INITIAL_STATE: ConnectActionState = { ok: true };

/// 连接 action — 连接指定的 Transport 节点
///
/// 注意: store 的 connectNode() 内部捕获异常 (不抛错), 失败时置该节点状态为 'Error',
/// 因此这里通过连接状态判断成功与否, 并复用 i18n 的 notifConnectFailed 错误文案。
export function useConnectAction(nodeId: string) {
  const connectNode = useAppStore((s) => s.connectNode);

  const [state, formAction, isPending] = useActionState<ConnectActionState>(
    async () => {
      await connectNode(nodeId);
      const { connectionStates, lang } = useAppStore.getState();
      if (connectionStates[nodeId] === 'Error') {
        return { ok: false, error: t(lang, 'notifConnectFailed') };
      }
      return { ok: true };
    },
    INITIAL_STATE
  );

  return { state, formAction, isPending };
}
