import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { useAppStore } from '../../../../store/appStore';
import { TransportConfigPanel } from '../../TransportConfigPanel';

describe('useConnectAction (transport connect submit)', () => {
  const mockConnect = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({
      lang: 'en',
      connectionState: 'Disconnected',
      transportConfig: {
        kind: 'Serial',
        params: {
          port_name: '',
          baud_rate: 115200,
          data_bits: 8,
          parity: 'none',
          stop_bits: 'one',
          flow_control: 'none',
        },
      },
      protocolConfig: { kind: 'JustFloat', channels: null },
      connect: mockConnect,
    });
  });

  it('disables the submit button and shows a pending label while connect is in flight', async () => {
    let releaseConnect!: () => void;
    mockConnect.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          releaseConnect = () => {
            useAppStore.setState({ connectionState: 'Connected' });
            resolve();
          };
        })
    );

    render(<TransportConfigPanel />);
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /connecting/i })).toBeDisabled();
    });

    releaseConnect();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /disconnect/i })).toBeInTheDocument();
    });
  });

  it('surfaces the connect error message when the store connect fails', async () => {
    mockConnect.mockImplementation(async () => {
      useAppStore.setState({ connectionState: 'Error' });
    });

    render(<TransportConfigPanel />);
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      expect(screen.getByText('Connection failed')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /^connect$/i })).toBeEnabled();
  });
});
