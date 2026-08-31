import { memo, useState } from 'react';
import type { ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { Check, Copy } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';

/// 从 hast 节点提取纯文本 — hljs 高亮会把代码拆成多个 span, 复制需还原原文
function hastText(node: unknown): string {
  if (!node || typeof node !== 'object') return '';
  const n = node as { type?: string; value?: string; children?: unknown[] };
  if (n.type === 'text') return n.value ?? '';
  if (Array.isArray(n.children)) return n.children.map(hastText).join('');
  return '';
}

/// 代码块 — 语言标签 + 复制按钮 + 横向滚动
function CodeBlock({ lang, raw, children }: { lang: string; raw: string; children: ReactNode }) {
  const lang2 = useAppStore((s) => s.lang);
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(raw);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* 剪贴板不可用时静默 */
    }
  };
  return (
    <div className="rounded border border-border-subtle overflow-hidden my-1.5">
      <div className="flex items-center px-2 py-0.5 bg-bg-window/60 border-b border-border-subtle">
        <span className="text-[10px] text-text-secondary font-mono">{lang || 'code'}</span>
        <button
          className="ml-auto p-0.5 rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
          onClick={() => void onCopy()}
          title={t(lang2, 'aiCopy')}
        >
          {copied ? <Check size={11} className="text-success" /> : <Copy size={11} />}
        </button>
      </div>
      <pre className="p-2 overflow-x-auto text-[11px] leading-relaxed">{children}</pre>
    </div>
  );
}

/// 组件化渲染规则 — 保持无 innerHTML, 样式对齐聊天面板字号
const components: Components = {
  pre: ({ children, node }) => {
    // hast 结构: <pre><code class="language-x hljs">…</code></pre>
    const codeEl = (node as { children?: { properties?: { className?: unknown } }[] } | undefined)
      ?.children?.[0];
    const classes = Array.isArray(codeEl?.properties?.className)
      ? (codeEl.properties.className as string[]).join(' ')
      : '';
    const lang = /language-([\w-]+)/.exec(classes)?.[1] ?? '';
    return (
      <CodeBlock lang={lang} raw={hastText(node)}>
        {children}
      </CodeBlock>
    );
  },
  // 外链经系统浏览器打开 (Tauri webview 内直接导航会被拦截)
  a: ({ children, href }) => (
    <a
      className="text-accent underline underline-offset-2 break-all cursor-pointer"
      onClick={(e) => {
        e.preventDefault();
        if (href) void openUrl(href);
      }}
    >
      {children}
    </a>
  ),
  p: ({ children }) => <p className="whitespace-pre-wrap">{children}</p>,
  h1: ({ children }) => <h1 className="text-sm font-semibold mt-2">{children}</h1>,
  h2: ({ children }) => <h2 className="text-sm font-semibold mt-2">{children}</h2>,
  h3: ({ children }) => <h3 className="text-xs font-semibold mt-1.5">{children}</h3>,
  h4: ({ children }) => <h4 className="text-xs font-semibold mt-1.5">{children}</h4>,
  ul: ({ children }) => <ul className="list-disc pl-4 space-y-0.5 my-1">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal pl-4 space-y-0.5 my-1">{children}</ol>,
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-border-subtle pl-2 text-text-secondary my-1">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto my-1.5">
      <table className="border-collapse text-[11px]">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border-subtle px-1.5 py-0.5 text-left font-medium">{children}</th>
  ),
  td: ({ children }) => <td className="border border-border-subtle px-1.5 py-0.5">{children}</td>,
  hr: () => <hr className="border-border-subtle my-1.5" />,
};

/// AI 回复 Markdown 渲染 — GFM (表格/任务列表/删除线) + 代码高亮。
/// 组件化输出, 不注入原始 HTML (XSS 安全); assistant 消息与流式文本共用。
export const AiMarkdown = memo(function AiMarkdown({ text }: { text: string }) {
  return (
    <div className="ai-md text-xs leading-relaxed break-words">
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
