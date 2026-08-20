import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RawDataViewSendPanel } from '../RawDataViewSendPanel';

describe('RawDataViewSendPanel', () => {
  it('shows and updates the selected append option', () => {
    const onAppendModeChange = vi.fn();
    const { rerender } = render(
      <RawDataViewSendPanel
        appendMode="nl"
        sendContent=""
        onAppendModeChange={onAppendModeChange}
        onSendContentChange={vi.fn()}
        onSend={vi.fn()}
        lang="en"
        targetTransportLabel="Serial"
      />
    );

    const none = screen.getByRole('button', { name: 'None' });
    const newline = screen.getByRole('button', { name: '\\n' });
    expect(newline).toHaveAttribute('aria-pressed', 'true');
    expect(newline).toHaveClass('bg-accent');
    expect(newline).not.toHaveClass('bg-bg-input');

    fireEvent.click(none);
    expect(onAppendModeChange).toHaveBeenCalledWith('none');

    rerender(
      <RawDataViewSendPanel
        appendMode="none"
        sendContent=""
        onAppendModeChange={onAppendModeChange}
        onSendContentChange={vi.fn()}
        onSend={vi.fn()}
        lang="en"
        targetTransportLabel="Serial"
      />
    );
    expect(none).toHaveAttribute('aria-pressed', 'true');
    expect(newline).toHaveAttribute('aria-pressed', 'false');
  });
});
