# VOFA-NEXT Frontend Architecture (Target)

> Status: **PLANNED** — this documents the target directory restructure and the rework
> phases that will execute it. The restructure is **not yet applied** to the codebase;
> today the frontend still lives in the flat `src/` layout described in the README.

## Motivation

The current `src/` tree grew organically: `components/displays/` holds every display
widget in one flat directory, and `src/lib/` mixes Tauri bindings, buffers,
subscriptions, hooks, and pure utilities. As the display/widget surface expands,
finding a file and testing a module require context the flat layout does not provide.

The rework moves to feature-grouped directories and adds a Vitest harness so pure
utils, stores, and presentational components can be tested without a Tauri runtime.

## Target directory tree

```
src/
├── components/
│   └── displays/
│       ├── waveform/          # WaveformChart + helpers (AxisSettings, WaveformTimeline,
│       │                      #   cursor overlay, series/render, chart export/hooks)
│       ├── can/               # CanView / CanSender / CanFrameList / CanLoadView / shared
│       ├── logic/             # LogicView / LogicTimingChart / DecodedEventList
│       ├── decoder/           # FrameDecoder + panels (LivePanel / ManualPanel / BlockEditor / shared)
│       ├── command/           # CommandSender + CommandSenderBlockEditor / shared
│       ├── rawdata/           # RawDataView / RawDataRow / helpers
│       └── widgets/           # Gauge / LED / PieChart / SpectrumChart / NumberDisplay /
│                              #   ImageViewer / Model3DWidget / TableView / MathWidget /
│                              #   FilterWidget / CustomWidget
├── lib/
│   ├── tauri/                 # tauri.ts / appExport.ts / notifications.ts
│   ├── buffers/               # dataBuffer / rawDataNodeBuffer / rawDataSubscription /
│   │                          #   canBuffer / logicBuffer / subscriptions / graphSubscription
│   ├── hooks/                 # useSelection / useGraphInput / useContextMenu / useUpdater
│   └── utils/                 # commandParser / clipboard / checksum / nodeDef / createWidget / scopeUtils
```

Files that stay in place (out of scope of this rework):

- `src/components/{controls,layout,nodes,onboarding,panels,ui}/`
- `src/i18n/`, `src/settings/`, `src/store/`, `src/types/`
- `src/App.tsx`, `src/main.tsx`
- `src-tauri/` (Rust backend, untouched by the frontend rework)

## Rework phases

| Phase | Scope | Deliverable |
| --- | --- | --- |
| T0 — Baseline lock + test scaffolding | **done** | `pnpm tsc --noEmit` / `pnpm build` verified green; Vitest harness (`vitest.config.ts`, `src/test/setup.ts` with `@tauri-apps/*` mocks, jest-dom); `test` / `test:watch` / `typecheck` scripts; 3 seed tests (pure util, store with Tauri mocks, component render); this document. |
| T1 — `src/lib` restructure | Move files | `lib/tauri/`, `lib/buffers/`, `lib/hooks/`, `lib/utils/` created; imports updated via barrel-free direct paths; tsc + build + tests stay green. |
| T2 — `src/components/displays` restructure | Move files | Seven feature directories (`waveform`, `can`, `logic`, `decoder`, `command`, `rawdata`, `widgets`); imports updated; tsc + build + tests stay green. |
| T3 — Test coverage expansion | New tests | Per-module suites for `lib/utils` pure functions, `lib/buffers`, store slices, and `displays/*` presentational components using the existing Tauri mocks. |
| T4 — CI integration | CI only | Add `pnpm test` + `pnpm typecheck` to `.github/workflows/` so the harness gates every PR. |

## Testing contract

- Tests live next to the code they cover: `src/**/__tests__/*.{test,spec}.{ts,tsx}`.
- `src/test/setup.ts` registers jest-dom matchers and stubs every `@tauri-apps/*`
  module the frontend imports (`api`, `api/core`, `api/event`, `plugin-log`,
  `plugin-store`, `plugin-dialog`, `plugin-notification`, `plugin-fs`,
  `plugin-opener`, `plugin-process`, `plugin-updater`) so stores and components run
  without a live Tauri runtime.
- Shared mock state is exposed as `tauriMock` from `src/test/setup.ts`
  (e.g. `tauriMock.seedFile('settings.json', 'app', {...})` for store-load tests).
- Production code is never modified to accommodate tests; no `as any`, no
  `@ts-ignore`, no tsconfig strictness reduction.
