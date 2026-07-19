import {
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  type Node,
  type Edge,
  type NodeChange,
  type EdgeChange,
  type Connection,
  MarkerType,
} from '@xyflow/react';
import { nanoid } from 'nanoid';
import { api } from '../../lib/tauri';
import { setInputValue as apiSetInputValue, submitCustomOutput as apiSubmitCustomOutput } from '../../lib/graphSubscription';
import { CHANNEL_SOURCE_ID, createChannelSourceNode, syncTabGraphToBackend } from '../appStoreHelpers';

export interface GraphSlice {
  rfNodes: Node[];
  rfEdges: Edge[];
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  getTabNodes: (tabId: string) => Node[];
  getTabEdges: (tabId: string) => Edge[];
  syncTabGraph: (tabId: string) => void;
  removeTabGraph: (tabId: string) => void;
  setInputValue: (widgetId: string, value: number) => void;
  submitCustomOutput: (widgetId: string, outputs: Record<string, number>) => void;
}

export function createGraphSlice(set: any, get: any): GraphSlice {
  return {
    rfNodes: [createChannelSourceNode('default', 4)],
    rfEdges: [],

    syncTabGraph: (tabId) => {
      void syncTabGraphToBackend(tabId);
    },

    removeTabGraph: (tabId) => {
      void api.removeTabGraph(tabId);
    },

    setInputValue: (widgetId, value) => {
      void apiSetInputValue(widgetId, value);
    },

    submitCustomOutput: (widgetId, outputs) => {
      void apiSubmitCustomOutput(widgetId, outputs);
    },

    onNodesChange: (changes) => {
      set((s: any) => ({
        rfNodes: applyNodeChanges(changes, s.rfNodes),
      }));
    },

    onEdgesChange: (changes) => {
      const affectedTabs = new Set<string>();
      for (const ch of changes) {
        if ('source' in ch && (ch as any).source) {
          const node = get().rfNodes.find((n: Node) => n.id === (ch as any).source);
          if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
        }
        if ('target' in ch && (ch as any).target) {
          const node = get().rfNodes.find((n: Node) => n.id === (ch as any).target);
          if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
        }
      }
      set((s: any) => ({
        rfEdges: applyEdgeChanges(changes, s.rfEdges),
      }));
      affectedTabs.forEach((tabId) => get().syncTabGraph(tabId));
    },

    onConnect: (connection) => {
      const newEdge: Edge = {
        ...connection,
        id: nanoid(8),
        animated: true,
        markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
      };
      let tabId: string | undefined;
      const sourceNode = get().rfNodes.find((n: Node) => n.id === connection.source);
      tabId = sourceNode?.data?.tabId as string | undefined;
      if (!tabId) {
        const targetNode = get().rfNodes.find((n: Node) => n.id === connection.target);
        tabId = targetNode?.data?.tabId as string | undefined;
        if (!tabId && connection.source.startsWith(CHANNEL_SOURCE_ID)) {
          tabId = connection.source.slice(CHANNEL_SOURCE_ID.length + 1);
        }
      }
      set((s: any) => ({
        rfEdges: addEdge(newEdge, s.rfEdges),
      }));
      if (tabId) get().syncTabGraph(tabId);
    },

    getTabNodes: (tabId) => {
      const { rfNodes } = get();
      return rfNodes.filter(
        (n: Node) => n.data.tabId === tabId || (n.type === 'channelSource' && n.id === `${CHANNEL_SOURCE_ID}-${tabId}`)
      );
    },

    getTabEdges: (tabId) => {
      const { rfNodes, rfEdges } = get();
      const tabNodeIds = new Set(
        rfNodes
          .filter((n: Node) => n.data.tabId === tabId || (n.type === 'channelSource' && n.id === `${CHANNEL_SOURCE_ID}-${tabId}`))
          .map((n: Node) => n.id)
      );
      return rfEdges.filter((e: Edge) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target));
    },
  };
}
