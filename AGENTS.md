# Repository Guidelines

## Project Structure & Module Organization

VOFA-NEXT is a Tauri 2 desktop application. The React/TypeScript frontend lives in `src/`: UI code is under `components/`, Zustand state under `store/`, shared code under `lib/`, translations under `i18n/locales/`, and test setup under `test/`. Frontend tests are colocated in `__tests__/`. Static files belong in `public/`; documentation lives in `docs/`.

The Rust Cargo workspace is rooted at `src-tauri/`. Startup code is in `src-tauri/src/`; focused packages live in `src-tauri/crates/<crate-name>/`, with integration tests in each crate's `tests/`. Keep transport, protocol, graph, and UI concerns in their existing modules.

## Build, Test, and Development Commands

- `pnpm install` installs locked frontend and Tauri tooling dependencies.
- `pnpm dev` starts the Vite frontend only.
- `pnpm tauri dev` runs the complete desktop application locally.
- `pnpm typecheck` performs strict TypeScript checking.
- `pnpm lint` runs ESLint over the workspace (flat config, see `eslint.config.js`).
- `pnpm lint:fix` runs ESLint with `--fix` to auto-apply safe fixes.
- `pnpm lint:ci` runs ESLint with `--max-warnings 0`; reserved for the day the lint baseline hits zero.
- `pnpm test` runs the Vitest suite once; `pnpm test:watch` supports iteration.
- `pnpm build` type-checks and produces the frontend bundle.
- `cd src-tauri && cargo test --workspace` runs all Rust tests.
- `cd src-tauri && cargo clippy --workspace --all-targets` enforces backend lint policy.
- `cd src-tauri && cargo fmt --check` verifies Rust formatting.

## Lint Baseline

ESLint was introduced with the strict `recommended-type-checked` + `stylistic-type-checked` presets from `typescript-eslint`, plus the React / React Hooks / React Refresh / JSX a11y plugin stacks. The current **baseline** (informational only, does not block PRs) is **1459 lint issues + 17 typecheck errors**, mostly from pre-existing `any` usage that fans out into `no-unsafe-*` warnings, plus a handful of test stubs and a few `@ts-ignore` / `@ts-expect-error` cases. The `.github/workflows/lint.yml` job runs `pnpm lint` and `pnpm typecheck` with `continue-on-error: true`; once the baseline is cleared, remove the `continue-on-error` flags and switch to `pnpm lint:ci` so CI becomes a real gate.

## Coding Style & Naming Conventions

Use two-space indentation and single-quoted imports in TypeScript. Use `PascalCase` for components and types, `camelCase` for functions and stores, and `useXxx` for hooks. Keep TypeScript strict and remove unused symbols. Rust follows `rustfmt`, `snake_case` modules/functions, and `PascalCase` types. The workspace denies Clippy `all`, `pedantic`, `nursery`, and `cargo` warnings; address warnings instead of casually suppressing them.

## Testing Guidelines

Vitest uses jsdom, Testing Library, and shared Tauri mocks from `src/test/setup.ts`. Name frontend tests `*.test.ts` or `*.test.tsx` and colocate them near the feature. Name Rust integration tests descriptively under `<crate>/tests/`. Add or update regression tests for behavioral changes; no fixed coverage threshold is configured.

## Commit & Pull Request Guidelines

Recent history follows Conventional Commit-style subjects such as `fix(rawdata): ...`, `feat(ai): ...`, and `refactor(engine): ...`. Use an imperative, concise subject with an accurate scope. Pull requests should explain the problem and solution, link relevant issues, list verification commands, and include screenshots or recordings for visible UI changes. Before submission, run TypeScript checks and tests plus Rust formatting, Clippy, and workspace tests.

## Security & Configuration

Never commit API keys, device credentials, or generated local state. AI provider keys belong in the operating system keychain. Avoid committing build outputs such as `dist/` and `src-tauri/target/`.
