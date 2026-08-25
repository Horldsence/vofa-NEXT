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
import { createDerivedSlice } from './slices/derived';
import { createCompileStatusSlice } from './slices/compileStatus';
import { createCompileHirSlice } from './slices/compileHir';

export type { SidebarView } from './slices/sidebar';
export {
  createTransportNode,
  createProtocolNode,
  isGlobalNode,
  syncTabGraphToBackend,
  traceProtocolSource,
  downstreamProtocolOf,
  getEffectiveChannels,
} from './appStoreHelpers';

export type AppStore = ReturnType<typeof createSidebarSlice>
  & ReturnType<typeof createConnectionSlice>
  & ReturnType<typeof createProtocolSlice>
  & ReturnType<typeof createWidgetSlice>
  & ReturnType<typeof createControlTabSlice>
  & ReturnType<typeof createGraphSlice>
  & ReturnType<typeof createDataTabSlice>
  & ReturnType<typeof createDataSlice>
  & ReturnType<typeof createGraphStateSlice>
  & ReturnType<typeof createEventSlice>
  & ReturnType<typeof createDerivedSlice>
  & ReturnType<typeof createCompileStatusSlice>
  & ReturnType<typeof createCompileHirSlice>;

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
  ...createDerivedSlice(set, get),
  ...createCompileStatusSlice(set, get),
  ...createCompileHirSlice(set, get),
}));
