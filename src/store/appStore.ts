import { create } from 'zustand';
import { createSidebarSlice } from './slices/sidebar';
import { createConnectionSlice } from './slices/connection';
import { createProtocolSlice } from './slices/protocol';
import { createWidgetSlice } from './slices/widgets';
import { createControlTabSlice } from './slices/controlTabs';
import { createGraphSlice } from './slices/graph';
import { createDataTabSlice } from './slices/dataTabs';
import { createDataSlice } from './slices/data';
import { createGraphStateSlice } from './slices/graphState';
import { createEventSlice } from './slices/events';

export type { SidebarView } from './slices/sidebar';
export { CHANNEL_SOURCE_ID } from './appStoreHelpers';

export type AppStore = ReturnType<typeof createSidebarSlice>
  & ReturnType<typeof createConnectionSlice>
  & ReturnType<typeof createProtocolSlice>
  & ReturnType<typeof createWidgetSlice>
  & ReturnType<typeof createControlTabSlice>
  & ReturnType<typeof createGraphSlice>
  & ReturnType<typeof createDataTabSlice>
  & ReturnType<typeof createDataSlice>
  & ReturnType<typeof createGraphStateSlice>
  & ReturnType<typeof createEventSlice>;

export const useAppStore = create<AppStore>()((set, get) => ({
  ...createSidebarSlice(set, get),
  ...createConnectionSlice(set, get),
  ...createProtocolSlice(set, get),
  ...createWidgetSlice(set, get),
  ...createControlTabSlice(set, get),
  ...createGraphSlice(set, get),
  ...createDataTabSlice(set, get),
  ...createDataSlice(set, get),
  ...createGraphStateSlice(),
  ...createEventSlice(set, get),
}));
