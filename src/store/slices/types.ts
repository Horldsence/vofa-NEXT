import type { StoreApi } from 'zustand';
import type { AppStore } from '../appStore';

export type AppSlice<T> = (
  set: StoreApi<AppStore>['setState'],
  get: StoreApi<AppStore>['getState'],
) => T;
