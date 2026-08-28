import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { AiMarkdown } from '../AiMarkdown';
import { tauriMock } from '../../../test/setup';

/// AI 回复 Markdown 渲染 — GFM / 代码高亮 / 无 HTML 注入 / 外链走系统浏览器

describe('AiMarkdown', () => {
  it('渲染标题 / 列表 / 表格 (GFM)', () => {
    render(<AiMarkdown text={'# 标题\n\n- 项目一\n\n| a | b |\n|---|---|\n| 1 | 2 |'} />);
    expect(screen.getByRole('heading', { level: 1, name: '标题' })).toBeInTheDocument();
    expect(screen.getByRole('list')).toBeInTheDocument();
    expect(screen.getByRole('table')).toBeInTheDocument();
  });

  it('代码块带语言标签与复制按钮', () => {
    render(<AiMarkdown text={'```rust\nfn main() {}\n```\n\n行内 `code` 文本'} />);
    expect(screen.getByText('rust')).toBeInTheDocument();
    expect(screen.getByTitle('复制')).toBeInTheDocument();
  });

  it('原始 HTML 不被注入 DOM (默认不渲染内联 HTML)', () => {
    const { container } = render(<AiMarkdown text={'<img src=x onerror="void 0" />'} />);
    expect(container.querySelector('img')).toBeNull();
  });

  it('链接点击经系统浏览器打开 (plugin-opener), 不在 webview 内导航', () => {
    render(<AiMarkdown text={'[官网](https://example.com)'} />);
    fireEvent.click(screen.getByText('官网'));
    expect(tauriMock.openUrl).toHaveBeenCalledWith('https://example.com');
  });
});
