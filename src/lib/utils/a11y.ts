import type { KeyboardEvent } from 'react';

/** Maps the standard keyboard activation keys to an element's click handler. */
export function activateOnKeyboard(event: KeyboardEvent<HTMLElement>): void {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  event.preventDefault();
  event.currentTarget.click();
}
