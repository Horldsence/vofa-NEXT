import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { RawDataLineSource, RawDataLineView } from '../../../../lib/buffers/dataBuffer';
import { Row } from '../RawDataRow';

function createBuffer(bytes: number[]): RawDataLineSource {
  const line: RawDataLineView = {
    offset: 0,
    timestamp: 0,
    direction: 'rx',
    bytes: Uint8Array.from(bytes),
  };

  return {
    lineCount: 1,
    newlineLineCount: 1,
    totalBytes: bytes.length,
    droppedBytes: 0,
    getLine: () => line,
    getNewlineLine: () => line,
    subscribe: () => () => { return undefined; },
    clear: () => { return undefined; },
  };
}

describe('RawDataRow', () => {
  it.each(['ascii', 'hex'] as const)('preserves an ASCII space in %s view', (repr) => {
    const { container } = render(
      <Row
        originalIndex={0}
        filteredIndex={0}
        grouping="line"
        repr={repr}
        buffer={createBuffer([0x41, 0x20, 0x42])}
        showTimestamp={false}
        showOffset={false}
        hexColorMode="none"
        isSelected={false}
        version={1}
        onMouseDown={vi.fn()}
      />
    );

    const spaceCell = Array.from(container.querySelectorAll('span')).find(
      (element) => element.textContent === ' '
    );

    expect(spaceCell).toHaveClass('whitespace-pre');
  });
});
