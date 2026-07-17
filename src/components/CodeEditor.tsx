import { useEffect, useRef } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { javascript } from '@codemirror/lang-javascript';
import { oneDark } from '@codemirror/theme-one-dark';
import { indentOnInput, bracketMatching, foldGutter, codeFolding } from '@codemirror/language';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  height?: string;
}

/// CodeMirror 6 包装组件 — JavaScript 模式 + OneDark 主题
export function CodeEditor({ value, onChange, readOnly = false, height = '100%' }: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // 初始化 CM 实例 (仅一次)
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const state = EditorState.create({
      doc: value,
      extensions: [
        lineNumbers(),
        foldGutter(),
        history(),
        bracketMatching(),
        indentOnInput(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        highlightSelectionMatches(),
        codeFolding(),
        javascript(),
        oneDark,
        EditorView.lineWrapping,
        EditorState.readOnly.of(readOnly),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && onChangeRef.current) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
        EditorView.theme({
          '&': { height, fontSize: '12px' },
          '.cm-content': { fontFamily: 'var(--font-mono)' },
          '.cm-gutters': { backgroundColor: '#1e1e1e' },
        }),
      ],
    });

    const view = new EditorView({ state, parent: container });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 外部 value 变化 → 同步到 CM (仅当与当前内容不同时)
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value },
      });
    }
  }, [value]);

  return <div ref={containerRef} style={{ height, width: '100%', overflow: 'hidden' }} />;
}
