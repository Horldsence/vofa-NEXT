import { defineConfig } from 'vitest/config';

/// Vitest 配置 — 与 vite.config.ts 相互独立, 不影响生产构建
///
/// - environment: jsdom — 组件测试需要 DOM
/// - globals: true — @testing-library/react 自动 cleanup 依赖全局 afterEach
/// - setupFiles: 注册 jest-dom 匹配器 + @tauri-apps/* 全量 mock (无需 Tauri 运行时)
export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
  },
});
