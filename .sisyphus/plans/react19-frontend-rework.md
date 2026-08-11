# VOFA-NEXT Frontend Rework Plan — React 19 / Perf / Restructure / Design

Scope (user-approved, Slint dropped as infeasible): React 19 modernization, performance optimization, directory reorganization, design polish.

## Verified constraints

- 155 .ts/.tsx files, React 19.1 + TS 5.8 + Vite 7 + Tailwind v4 + zustand 5 + Tauri 2 webview.
- NO test infra exists (vitest/jsdom must be scaffolded first).
- Only one React context: `WidgetEmbeddedContext` (WidgetCard.tsx). No `useShallow`, `startTransition`, `useActionState`, `React.lazy` in codebase.
- Tailwind v4 unknown utilities are silently dropped → token rename risks silent visual drift.
- `build` = `tsc && vite build`; no lint step.
- DOM-essential (stay React): React Flow, CodeMirror, uPlot, Three.js, TanStack Virtual, iframes, react-resizable-panels.

## Target tree (T1)

```
src/components/displays/{waveform,can,logic,decoder,command,rawdata,widgets}/
  waveform ← WaveformChart + ~10 helpers
  can      ← CanView, CanSender, CanFrameList, CanLoadView + shared
  logic    ← LogicView, LogicTimingChart, DecodedEventList
  decoder  ← FrameDecoder + LivePanel/ManualPanel/BlockEditor/shared
  command  ← CommandSender + BlockEditor + shared
  rawdata  ← RawDataView + Row + helpers
  widgets  ← Gauge, LED, PieChart, SpectrumChart, NumberDisplay, ImageViewer,
             Model3DWidget, TableView, MathWidget, FilterWidget, CustomWidget
src/lib/{tauri,buffers,hooks,utils}/
  tauri    ← tauri.ts, appExport.ts, notifications.ts
  buffers  ← dataBuffer, rawData*, canBuffer, logicBuffer, *Subscription, graphSubscription
  hooks    ← useSelection, useGraphInput, useContextMenu, useUpdater
  utils    ← commandParser, clipboard, checksum, nodeDef, createWidget, scopeUtils
```
No new barrel files (keep imports explicit). store/, types/, i18n/, settings/ stay as-is.

## Tasks & waves

| Wave | Task | Depends | Category / Skills | Gate |
|---|---|---|---|---|
| 1 | T0 baseline lock + vitest scaffold + target-tree doc | — | unspecified-low / git-master | tsc+build+test green |
| 2 | T1 directory restructure (single atomic commit) | T0 | deep / git-master | tsc+build green; git diff -M = paths only |
| 3 | T2a lazy+Suspense (Model3D, CodeEditor, modals) | T1 | deep / [] | split chunks; fallback test |
| 3 | T2b startTransition (tabs/sidebar/settings) | T1 | deep / [] | tab switch non-blocking |
| 3 | T2c useActionState (7 transport forms + settings) | T1 | quick / [] | pending/error/reset tests |
| 4 | T2d memo heavy components + onMount/onUnmount | T2a | deep / ai-slop-remover | render-count tests |
| 5 | T3a store subscription granularity (App/DockCardFrame) | T2b, T2d | deep / [] | isolated re-render tests |
| 5 | T3b high-frequency data paths (waveform/CAN/raw) | T2d | ultrabrain / review-work | 60fps at high rate; contract intact |
| 5 | T3c render-cascade reduction (DataTabContent/WidgetCard/AnimatedSwitch/virtual) | T2d | deep / [] | sibling-tab isolation test |
| 6 | T4a token audit + semantic scale + theme migration + CSS split | T3a/b/c | visual-engineering / frontend-ui-ux | migration test; zero orphans; screenshot diff |
| 7 | T4b interaction states + motion (CSS single owner) | T4a | visual-engineering / frontend-ui-ux | focus/interaction consistency |
| 7 | T4c layout chrome refinement (TSX only) | T4a | visual-engineering / frontend-ui-ux | chrome checklist; CSS via T4b |
| 8 | T5 hardening + full QA + review-work oracles | T4b, T4c | unspecified-high / review-work, git-master | all gates green |

Critical path: T0 → T1 → T2a → T2d → T3b → T4a → T4b → T5.

## Do-not-touch

- src-tauri/ (all Rust, tauri.conf.json, capabilities, icons); Rust↔JS event contract; existing i18n YAML keys (additive only); persisted store schema without migration; main.tsx StrictMode; index.html; vite.config.ts; public/; CI workflows; scripts/.

## React Compiler decision

Do NOT adopt in this rework. Revisit post-T3 as flagged pilot on isolated slice. Rationale: heavy leaves are imperative/isolated already; no eslint-hooks baseline; StrictMode compounding; manual memo + selector granularity capture same wins debuggably.

## Commit strategy

One task = one commit, green before commit (tsc + build + test). T1 is one atomic commit. Conventional scoped messages. TDD per task where feasible (T2c, T3 render guards, T4a migration).
