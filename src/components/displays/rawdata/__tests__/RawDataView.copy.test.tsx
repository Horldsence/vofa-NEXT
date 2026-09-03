import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render } from '@testing-library/react';
import { rawDataBuffer } from '../../../../lib/buffers/dataBuffer';
import { RawDataView } from '../RawDataView';

const clipboardMock = vi.hoisted(() => ({
  writeTextToClipboard: vi.fn(async () => true),
}));

vi.mock('../../../../lib/utils/clipboard', () => clipboardMock);

describe('RawDataView copy', () => {
  beforeEach(async () => {
    rawDataBuffer.clear();
    rawDataBuffer.pushBatch({
      seq: 1,
      chunks: [{ bytes_b64: btoa('0123456789abcdef'), timestamp_us: 0, direction: 'rx' }],
      total_bytes: 16,
      dropped_bytes: 0,
    });
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    clipboardMock.writeTextToClipboard.mockClear();
  });

  it('lets the browser copy a native text selection even when a row is selected', () => {
    const { container } = render(<RawDataView />);
    const content = container.querySelector<HTMLElement>('[tabindex="0"]');
    expect(content).not.toBeNull();

    fireEvent.keyDown(content!, { key: 'a', ctrlKey: true });
    vi.spyOn(window, 'getSelection').mockReturnValue({ isCollapsed: false } as Selection);

    const copyEvent = new KeyboardEvent('keydown', {
      key: 'c',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    fireEvent(content!, copyEvent);

    expect(copyEvent.defaultPrevented).toBe(false);
    expect(clipboardMock.writeTextToClipboard).not.toHaveBeenCalled();
  });
});
