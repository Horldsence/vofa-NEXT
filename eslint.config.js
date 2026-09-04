// @ts-check
import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import react from 'eslint-plugin-react';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import jsxA11y from 'eslint-plugin-jsx-a11y';

export default tseslint.config(
  // 1. 全局忽略
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      'src-tauri/**',
      'public/**',
      'coverage/**',
      'docs/**',
      '**/*.d.ts',
      'promo*.html',
      'index.html',
      'eslint.config.js',
    ],
  },

  // 2. JS 基础推荐规则
  js.configs.recommended,

  // 3. TypeScript 严格档:type-checked(类型相关 bug)+ stylistic(风格一致)
  ...[
    ...tseslint.configs.recommendedTypeChecked,
    ...tseslint.configs.stylisticTypeChecked,
  ].map(config => ({ ...config, files: ['**/*.{ts,tsx}'] })),

  // 4. 给所有 TS/TSX 开 projectService,自动按文件选正确的 tsconfig
  //    (src/* → tsconfig.json, vite.config.ts → tsconfig.node.json)
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },

  // 5. 源码:浏览器 + React 生态
  {
    files: ['src/**/*.{ts,tsx}'],
    plugins: {
      react,
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
      'jsx-a11y': jsxA11y,
    },
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.es2024,
      },
    },
    settings: {
      react: { version: '19.0' },
    },
    rules: {
      // React 生态
      ...react.configs.recommended.rules,
      ...react.configs['jsx-runtime'].rules,
      ...reactHooks.configs.recommended.rules,
      ...jsxA11y.configs.recommended.rules,
      'react-refresh/only-export-components': [
        'error',
        {
          allowConstantExport: true,
          // 这些导出是同模块组件所需的纯工具、类型上下文或 R3F 配置；
          // 明确列出以保持 Fast Refresh 的其余约束有效。
          allowExportNames: [
            'BITRATE_PRESETS', 'BLOCK_TYPE_CONFIG', 'CHECKSUM_COVER_OPTIONS',
            'CHECKSUM_OPTIONS', 'CHECKSUM_POSITION_OPTIONS', 'CanvasErrorTooltip',
            'CompileDot', 'CustomWidget', 'FIELD_TYPE_OPTIONS', 'FRAME_DECODER_ADDABLE_TYPES',
            'FRAME_EXAMPLES', 'GROUP_SIZE', 'HISTORY_KEY', 'HISTORY_MAX', 'HeaderBytes',
            'IdLoadDistribution', 'MeasureItem', 'NumericPortStatus', 'PillRect', 'ROW_HEIGHT',
            'SlidingPill', 'THRESHOLD_PRESETS', 'WINDOW_PRESETS', 'WidgetCard',
            'WidgetEmbeddedContext', 'blockSummary', 'byteToAscii', 'byteToHex',
            'compileErrorMessage', 'directionColorClass', 'directionSymbol', 'evalCustomWidgetDef',
            'formatBitrate', 'formatFps', 'formatFreq', 'formatNumericValue', 'formatPercent',
            'formatTime', 'getCompileStatus', 'getOutputPortNames', 'hexColorClass',
            'isPrintable', 'isWindowsPlatform', 'loadColor', 'loadHistory',
            'numericValueOr', 'saveHistory', 'useCanvasNodeError', 'useSlidingPill',
          ],
        },
      ],

      // 严格 TS 风格
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'separate-type-imports' },
      ],
      '@typescript-eslint/consistent-type-definitions': ['error', 'interface'],
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],

      // JS 通用安全
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      'no-console': ['warn', { allow: ['warn', 'error', 'info'] }],
      'prefer-const': 'error',
      'no-var': 'error',
    },
  },

  // 6. 测试:放宽 any / 非空断言 / console
  {
    files: [
      'src/**/*.{test,spec}.{ts,tsx}',
      'src/test/**/*.{ts,tsx}',
    ],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-non-null-assertion': 'off',
      'no-console': 'off',
    },
  },

  // 7. 根目录配置文件:vite/vitest 等走 Node 环境
  {
    files: ['*.config.{js,ts,mjs,cjs}', 'scripts/**/*.{js,mjs,cjs}'],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      'no-console': 'off',
    },
  },

  // 8. 严格规则：所有诊断均阻断 lint。
  {
    files: ['**/*.{ts,tsx}'],
    rules: {
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/ban-ts-comment': 'error',
      '@typescript-eslint/no-base-to-string': 'error',
      '@typescript-eslint/no-empty-function': 'error',
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-implied-eval': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/no-redundant-type-constituents': 'error',
      '@typescript-eslint/no-unsafe-argument': 'error',
      '@typescript-eslint/no-unsafe-assignment': 'error',
      '@typescript-eslint/no-unsafe-call': 'error',
      '@typescript-eslint/no-unsafe-member-access': 'error',
      '@typescript-eslint/no-unsafe-return': 'error',
      '@typescript-eslint/only-throw-error': 'error',
      '@typescript-eslint/prefer-for-of': 'error',
      '@typescript-eslint/prefer-nullish-coalescing': 'error',
      '@typescript-eslint/prefer-optional-chain': 'error',
      '@typescript-eslint/require-await': 'error',
      '@typescript-eslint/unbound-method': 'error',
      'no-fallthrough': 'error',
      'no-useless-assignment': 'error',
      'preserve-caught-error': 'error',
    },
  },
  {
    files: ['src/**/*.{ts,tsx}'],
    rules: {
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/no-unused-vars': 'error',
      'jsx-a11y/anchor-is-valid': 'error',
      'jsx-a11y/click-events-have-key-events': 'error',
      'jsx-a11y/label-has-associated-control': 'error',
      'jsx-a11y/no-autofocus': 'error',
      'jsx-a11y/no-noninteractive-element-interactions': 'error',
      'jsx-a11y/no-noninteractive-tabindex': 'error',
      'jsx-a11y/no-static-element-interactions': 'error',
      'no-console': 'error',
      'react-hooks/exhaustive-deps': 'error',
      // React Three Fiber 使用 JSX 属性映射 Three.js 对象；这些并非 DOM 属性。
      'react/no-unknown-property': [
        'error',
        {
          ignore: [
            'args',
            'emissive',
            'emissiveIntensity',
            'intensity',
            'object',
            'position',
            'rotation',
            'transparent',
          ],
        },
      ],
    },
  },
);
