import { useEffect } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { ActivityBar } from './components/layout/ActivityBar';
import { Sidebar } from './components/layout/Sidebar';
import { ControlPanel } from './components/layout/ControlPanel';
import { DataPanel } from './components/layout/DataPanel';
import { AiPanel } from './components/layout/AiPanel';
import { StatusBar } from './components/layout/StatusBar';
import { useAppStore } from './store/appStore';
import './App.css';

function App() {
  const initEventListeners = useAppStore((s) => s.initEventListeners);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const sidebarVisible = useAppStore((s) => s.sidebarVisible);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    initEventListeners().then((fn) => {
      cleanup = fn;
    });
    refreshPorts();
    return () => {
      cleanup?.();
    };
  }, [initEventListeners, refreshPorts]);

  return (
    <div className="app">
      <div className="app-body">
        <ActivityBar
          activeView={sidebarVisible ? sidebarView : null}
          onSelect={toggleSidebar}
        />
        <PanelGroup direction="horizontal" autoSaveId="sp-main">
          {sidebarVisible && (
            <>
              <Panel defaultSize={18} minSize={12} maxSize={35} order={1}>
                <Sidebar view={sidebarView} />
              </Panel>
              <PanelResizeHandle className="resize-handle-horizontal" />
            </>
          )}
          <Panel order={2}>
            <PanelGroup direction="vertical" autoSaveId="sp-center">
              <Panel defaultSize={40} minSize={15} order={1}>
                <ControlPanel />
              </Panel>
              <PanelResizeHandle className="resize-handle-vertical" />
              <Panel defaultSize={60} minSize={15} order={2}>
                <DataPanel />
              </Panel>
            </PanelGroup>
          </Panel>
          <PanelResizeHandle className="resize-handle-horizontal" />
          <Panel defaultSize={20} minSize={12} maxSize={40} order={3}>
            <AiPanel />
          </Panel>
        </PanelGroup>
      </div>
      <StatusBar />
    </div>
  );
}

export default App;
