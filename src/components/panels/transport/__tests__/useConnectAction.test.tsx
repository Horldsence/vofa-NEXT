import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { useAppStore } from '../../../../store/appStore';
import { useConnectAction } from '../useConnectAction';

/// 最小 harness — 复刻属性面板连接按钮的 form action 用法
function Harness({ nodeId }: { nodeId: string }) {
  const { state, formAction, isPending } = useConnectAction(nodeId);
  return (
    <form action={formAction}>
      <button type="submit" disabled={isPending}>
        {isPending ? 'Connecting' : 'Connect'}
      </button>
      {state.error && <div>{state.error}</div>}
    </form>
  );
}

describe('useConnectAction (transport node connect submit)', () => {
  const mockConnectNode = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({
      lang: 'en',
      connectionStates: { 'transport-1': 'Disconnected' },
      connectNode: mockConnectNode,
    });
  });

  it('disables the submit button and shows a pending label while connect is in flight', async () => {
    let releaseConnect!: () => void;
    mockConnectNode.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          releaseConnect = () => {
            useAppStore.setState((s) => ({
              connectionStates: { ...s.connectionStates, 'transport-1': 'Connected' },
            }));
            resolve();
          };
        })
    );

    render(<Harness nodeId="transport-1" />);
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /connecting/i })).toBeDisabled();
    });

    releaseConnect();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /^connect$/i })).toBeEnabled();
    });
    expect(mockConnectNode).toHaveBeenCalledWith('transport-1');
  });

  it('surfaces the connect error message when the store connect fails', async () => {
    mockConnectNode.mockImplementation(() => {
      useAppStore.setState((s) => ({
        connectionStates: { ...s.connectionStates, 'transport-1': 'Error' },
      }));
    });

    render(<Harness nodeId="transport-1" />);
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(screen.getByText('Connection failed')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /^connect$/i })).toBeEnabled();
  });
});
