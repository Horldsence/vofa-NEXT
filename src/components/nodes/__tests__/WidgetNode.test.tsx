import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { NodeProps } from '@xyflow/react';

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    Handle: ({ id }: { id?: string }) => <span data-handle-id={id} />,
    useUpdateNodeInternals: () => vi.fn(),
  };
});

import { WidgetNode } from '../WidgetNode';
import { createWidget } from '../../../lib/utils/widgetDefaults';
import { useAppStore } from '../../../store/appStore';

describe('WidgetNode title rename', () => {
  beforeEach(() => {
    useAppStore.setState({
      widgets: [],
      rfNodes: [],
      rfEdges: [],
      dataTabs: [],
      tabStates: {},
      tabErrorNodes: {},
      tabErrors: {},
    });
  });

  it('renames on title-bar double click and keeps window IDs stable', () => {
    const widget = createWidget('Waveform');
    const node = {
      id: widget.params.id,
      type: 'widget',
      position: { x: 0, y: 0 },
      data: { widget, tabId: 'default' },
    };
    useAppStore.setState({
      widgets: [widget],
      rfNodes: [node],
      dataTabs: [{ id: widget.params.id, widgetId: widget.params.id, type: 'waveform-extra', name: widget.params.label, closable: true }],
    });

    render(<WidgetNode {...({ id: node.id, data: node.data } as unknown as NodeProps)} />);
    fireEvent.doubleClick(screen.getByTitle('双击标题栏修改名称'));
    const input = screen.getByRole('textbox', { name: /控件名称|Widget Name/ });
    fireEvent.change(input, { target: { value: 'Motor Scope' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(useAppStore.getState().widgets[0].params.label).toBe('Motor Scope');
    expect(useAppStore.getState().dataTabs[0]).toMatchObject({
      id: widget.params.id,
      widgetId: widget.params.id,
      name: 'Motor Scope',
    });
  });

  it('keeps editing an empty name until cancelled with Escape', () => {
    const widget = createWidget('Knob');
    const data = { widget, tabId: 'default' };
    useAppStore.setState({ widgets: [widget] });
    render(<WidgetNode {...({ id: widget.params.id, data } as unknown as NodeProps)} />);

    fireEvent.doubleClick(screen.getByTitle('双击标题栏修改名称'));
    const input = screen.getByRole('textbox', { name: /控件名称|Widget Name/ });
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(useAppStore.getState().widgets[0].params.label).toBe('Knob');

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByRole('textbox', { name: /控件名称|Widget Name/ })).not.toBeInTheDocument();
  });
});
