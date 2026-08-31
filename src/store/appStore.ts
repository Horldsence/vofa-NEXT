import { create } from 'zustand';
import { createSidebarSlice, type SidebarSlice } from './slices/sidebar';
import { createConnectionSlice, type ConnectionSlice } from './slices/connection';
import { createProtocolSlice, type ProtocolSlice } from './slices/protocol';
import { createWidgetSlice, type WidgetSlice } from './slices/widgets';
import { createControlTabSlice, type ControlTabSlice } from './slices/controlTabs';
import { createGraphSlice, type GraphSlice } from './slices/graph';
import { createDataTabSlice, type DataTabSlice } from './slices/dataTabs';
import { createDataSlice, type DataSlice } from './slices/data';
import { createGraphStateSlice } from './slices/graphState';
import { createEventSlice, type EventSlice } from './slices/events';
import { createDerivedSlice, type DerivedSlice } from './slices/derived';
import { createCompileStatusSlice, type CompileStatusSlice } from './slices/compileStatus';
import { createCompileHirSlice, type CompileHirSlice } from './slices/compileHir';

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

export type AppStore = SidebarSlice
  & ConnectionSlice
  & ProtocolSlice
  & WidgetSlice
  & ControlTabSlice
  & GraphSlice
  & DataTabSlice
  & DataSlice
  & ReturnType<typeof createGraphStateSlice>
  & EventSlice
  & DerivedSlice
  & CompileStatusSlice
  & CompileHirSlice;

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
