import { create } from 'zustand';
import type { ContextMenuEntry } from '../types';

interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
  items: ContextMenuEntry[];
  open: (x: number, y: number, items: ContextMenuEntry[]) => void;
  close: () => void;
}

export const useContextMenuStore = create<ContextMenuState>((set) => ({
  visible: false,
  x: 0,
  y: 0,
  items: [],
  open: (x, y, items) => set({ visible: true, x, y, items }),
  close: () => set({ visible: false, items: [] }),
}));
