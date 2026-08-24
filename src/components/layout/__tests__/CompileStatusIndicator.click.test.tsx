import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { useAppStore } from '../../../store/appStore';
import CompileStatusIndicator from '../CompileStatusIndicator';

describe('CompileStatusIndicator Click Behavior', () => {
  beforeEach(() => {
    // Reset store state
    useAppStore.setState({
      lang: 'en',
      errorTabs: [],
      pendingTabs: [],
      tabStates: {},
    });
  });

  it('renders null when errorTabs and pendingTabs are empty', () => {
    const onClickSpy = vi.fn();
    const { container } = render(<CompileStatusIndicator onClickError={onClickSpy} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders a button and calls onClickError when clicked in error state', () => {
    const onClickSpy = vi.fn();
    
    // Seed store with error tabs
    useAppStore.setState({
      errorTabs: ['tab-1'],
      tabStates: { 'tab-1': 'error' },
    });

    render(<CompileStatusIndicator onClickError={onClickSpy} />);
    
    const button = screen.getByRole('button', { name: /1 compile error/i });
    expect(button).toBeInTheDocument();
    
    fireEvent.click(button);
    expect(onClickSpy).toHaveBeenCalledTimes(1);
  });

  it('compact prop hides the count text but maintains the button and clickable behavior', () => {
    const onClickSpy = vi.fn();
    
    useAppStore.setState({
      errorTabs: ['tab-1', 'tab-2'],
      tabStates: { 'tab-1': 'error', 'tab-2': 'error' },
    });

    render(<CompileStatusIndicator compact onClickError={onClickSpy} />);
    
    const button = screen.getByRole('button', { name: /2 compile errors/i });
    expect(button).toBeInTheDocument();
    expect(screen.queryByText('2')).not.toBeInTheDocument();

    fireEvent.click(button);
    expect(onClickSpy).toHaveBeenCalledTimes(1);
  });
});
