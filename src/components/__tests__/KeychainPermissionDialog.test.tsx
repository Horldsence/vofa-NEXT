import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KeychainPermissionDialog } from '../KeychainPermissionDialog';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';

const originalDismiss = useSettingsStore.getState().dismissKeychainPermissionPrompt;
const originalRetry = useSettingsStore.getState().retryKeychainPermission;

describe('KeychainPermissionDialog', () => {
  beforeEach(() => {
    useAppStore.setState({ lang: 'zh' });
    useSettingsStore.setState({
      keychainPermissionRetrying: false,
      keychainPermissionRetryError: null,
      dismissKeychainPermissionPrompt: originalDismiss,
      retryKeychainPermission: originalRetry,
    });
  });

  it('解释权限作用并把不再提醒选择交给稍后处理动作', () => {
    const dismiss = vi.fn();
    useSettingsStore.setState({ dismissKeychainPermissionPrompt: dismiss });
    render(<KeychainPermissionDialog />);

    expect(screen.getByRole('dialog')).toHaveTextContent('需要访问系统钥匙串');
    expect(screen.getByRole('dialog')).toHaveTextContent('不会写入设置文件');
    expect(screen.getByRole('dialog')).toHaveTextContent('串口、图表及其他功能不受影响');

    fireEvent.click(screen.getByLabelText('不再提醒'));
    fireEvent.click(screen.getByRole('button', { name: '稍后处理' }));
    expect(dismiss).toHaveBeenCalledWith(true);
  });

  it('支持英文重试状态和再次拒绝提示', () => {
    useAppStore.setState({ lang: 'en' });
    useSettingsStore.setState({
      keychainPermissionRetrying: true,
      keychainPermissionRetryError: 'denied',
    });
    render(<KeychainPermissionDialog />);

    expect(screen.getByRole('button', { name: 'Requesting…' })).toBeDisabled();
    expect(screen.getByRole('alert')).toHaveTextContent('still not authorized');
  });
});
