import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { useAppStore } from '../../../store/appStore';
import { MenuBar } from '../MenuBar';

// @tauri-apps/api/window 系列在 jsdom 无真实后端 — 桩掉窗口 API 与退出
const windowApiMock = vi.hoisted(() => ({
  setZoom: vi.fn(async (_factor: number) => undefined),
  minimize: vi.fn(async () => undefined),
  toggleMaximize: vi.fn(async () => undefined),
  close: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => windowApiMock,
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  exit: vi.fn(async () => undefined),
}));

/// 模拟 Windows 平台 UA (isWindowsPlatform 在渲染时读取 navigator.userAgent)
function stubWindowsUA() {
  Object.defineProperty(navigator, 'userAgent', {
    value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36',
    configurable: true,
  });
}

/// 恢复默认 jsdom UA
function restoreUA() {
  Object.defineProperty(navigator, 'userAgent', {
    value: 'Mozilla/5.0 (compatible; jsdom)',
    configurable: true,
  });
}

describe('MenuBar (Windows 自定义菜单栏)', () => {
  beforeEach(() => {
    stubWindowsUA();
    // 固定英文标签并复位侧边栏状态, 避免跨测试状态泄漏
    useAppStore.setState({ lang: 'en', sidebarVisible: true, sidebarView: 'widgets' });
    windowApiMock.setZoom.mockClear();
    windowApiMock.minimize.mockClear();
    windowApiMock.toggleMaximize.mockClear();
    windowApiMock.close.mockClear();
  });

  afterEach(() => {
    restoreUA();
  });

  it('渲染五组菜单标题 (App/File/Edit/View/Window/Help)', () => {
    render(<MenuBar />);
    expect(screen.getByRole('button', { name: 'VOFA-Next' })).toBeInTheDocument();
    for (const label of ['File', 'Edit', 'View', 'Window', 'Help']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
  });

  it('非 Windows 平台不渲染', () => {
    restoreUA();
    const { container } = render(<MenuBar />);
    expect(container.firstChild).toBeNull();
  });

  it('File > New Tab 创建新标签页并关闭下拉', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByRole('button', { name: 'File' }));
    const newTabItem = screen.getByRole('menuitem', { name: /New Tab/ });
    expect(newTabItem).toBeInTheDocument();

    const before = useAppStore.getState().controlTabs.length;
    fireEvent.click(newTabItem);
    expect(useAppStore.getState().controlTabs.length).toBe(before + 1);
    // 点击后下拉关闭
    expect(screen.queryByRole('menuitem', { name: /New Tab/ })).not.toBeInTheDocument();
  });

  it('View > Toggle Sidebar 切换侧边栏可见性', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Toggle Sidebar/ }));
    expect(useAppStore.getState().sidebarVisible).toBe(false);
  });

  it('View > Zoom In 调用 setZoom 且步进缩放', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Zoom In/ }));
    expect(windowApiMock.setZoom).toHaveBeenCalledTimes(1);
    const first = windowApiMock.setZoom.mock.calls[0][0];
    // 再次放大 — 基于上一次结果步进
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Zoom In/ }));
    const second = windowApiMock.setZoom.mock.calls[1][0];
    expect(second).toBeCloseTo(first + 0.2, 5);
  });

  it('Window > Minimize / Close 调用窗口 API', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByRole('button', { name: 'Window' }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Minimize/ }));
    expect(windowApiMock.minimize).toHaveBeenCalledTimes(1);
    expect(windowApiMock.close).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Window' }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Close Window/ }));
    expect(windowApiMock.close).toHaveBeenCalledTimes(1);
  });

  it('Ctrl+T 快捷键新建标签页, Ctrl+B 切换侧边栏', () => {
    render(<MenuBar />);
    const before = useAppStore.getState().controlTabs.length;
    act(() => {
      fireEvent.keyDown(window, { key: 't', ctrlKey: true });
    });
    expect(useAppStore.getState().controlTabs.length).toBe(before + 1);

    act(() => {
      fireEvent.keyDown(window, { key: 'b', ctrlKey: true });
    });
    expect(useAppStore.getState().sidebarVisible).toBe(false);
  });

  it('ESC 关闭打开的下拉面板', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByRole('button', { name: 'Help' }));
    expect(screen.getByRole('menuitem', { name: /Documentation/ })).toBeInTheDocument();
    act(() => {
      fireEvent.keyDown(window, { key: 'Escape' });
    });
    expect(screen.queryByRole('menuitem', { name: /Documentation/ })).not.toBeInTheDocument();
  });
});
