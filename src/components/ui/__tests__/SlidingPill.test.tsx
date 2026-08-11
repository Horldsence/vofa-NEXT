import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { SlidingPill, type PillRect } from '../SlidingPill';

const VISIBLE_PILL: PillRect = { left: 10, top: 20, width: 120, height: 32, visible: true };

describe('SlidingPill', () => {
  it('positions the pill element from the pill rect', () => {
    render(<SlidingPill pill={VISIBLE_PILL} />);
    const pill = document.querySelector('.tab-sliding-pill');
    expect(pill).not.toBeNull();
    expect(pill).toBeInTheDocument();
    expect(pill).toHaveStyle({ left: '10px', top: '20px', width: '120px', height: '32px' });
  });

  it('renders nothing while the pill is hidden', () => {
    render(<SlidingPill pill={{ left: 0, top: 0, width: 0, height: 0, visible: false }} />);
    expect(document.querySelector('.tab-sliding-pill')).toBeNull();
  });

  it('applies the panel variant class', () => {
    render(<SlidingPill pill={VISIBLE_PILL} variant="panel" />);
    expect(document.querySelector('.tab-sliding-pill')).toHaveClass('tab-sliding-pill--panel');
  });
});
