#!/usr/bin/env node
/**
 * check-crate-layers.mjs — Rust workspace 分层规则机械校验
 *
 * 解析 `cargo metadata`,校验 docs/architecture/crate-layers.md 定义的规则:
 *   1. rank 规则: 依赖只允许 rank(被依赖方) ≤ rank(依赖方), 同层可互依
 *      foundation(0) < protocol/transport/node(1) < pipeline(2) < app/ai(3) < cmd(4) < 二进制(5)
 *   2. cmd 层隔离: cmd/* 禁止被任何非 cmd crate 依赖 (二进制除外)
 *   3. testkit 规则: node/testkit 只能作为 dev-dependency
 *
 * 用法: pnpm check:layers  (要求本机有 cargo)
 */
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workspaceRoot = join(repoRoot, "src-tauri");

// ---- 分层定义 (与 docs/architecture/crate-layers.md 保持一致) ----
const LAYERS = {
  foundation: 0,
  protocol: 1,
  transport: 1,
  node: 1,
  pipeline: 2,
  app: 3,
  ai: 3,
  cmd: 4,
  binary: 5,
};

/** crate 名 → 层目录 (与 workspace members 一致; 改动时两处同步) */
const CRATE_TO_LAYER = {
  error: "foundation",
  can_types: "foundation",
  vofa_core: "foundation",
  logic_types: "foundation",
  diagnostic: "foundation",
  schema_types: "foundation",
  buffer_ring: "foundation",
  buffer_raw: "foundation",
  buffer_graph: "foundation",
  buffer_databuffer: "foundation",
  dsp_window: "foundation",
  dsp_filter: "foundation",
  dsp_fft: "foundation",
  subscription: "foundation",
  gpu_core: "foundation",
  notify_events: "foundation",
  protocol_engine: "protocol",
  protocol_float: "protocol",
  protocol_can_bridge: "protocol",
  logic_decoder: "protocol",
  schema_engine: "protocol",
  serial: "transport",
  net: "transport",
  can_bridge: "transport",
  transport_core: "transport",
  automotive_can: "transport",
  automotive_isotp: "transport",
  automotive_diag: "transport",
  kind: "node",
  hir: "node",
  plane: "node",
  lower: "node",
  eval: "node",
  engine: "node",
  frame_decoder: "node",
  trigger: "node",
  testkit: "node",
  data_plane: "pipeline",
  data_bus: "pipeline",
  stream: "pipeline",
  dispatcher: "pipeline",
  app_state: "app",
  graph_ops: "app",
  menu_shell: "app",
  update_flow: "app",
  provider: "ai",
  session: "ai",
  chat: "ai",
  mcp_client: "ai",
  mcp_server: "ai",
  ai: "cmd",
  buffer: "cmd",
  can_load: "cmd",
  can_transport: "cmd",
  debug: "cmd",
  display: "cmd",
  graph: "cmd",
  pipeline: "cmd",
  rawdata: "cmd",
};

const BINARY = "vofa-next";
const TESTKIT = "testkit";
const CMD_LAYER = "cmd";

// ---- cargo metadata ----
let meta;
try {
  meta = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
      cwd: workspaceRoot,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
} catch (err) {
  console.error("cargo metadata 执行失败 (需要本机安装 cargo):", err.message);
  process.exit(2);
}

const errors = [];

for (const pkg of meta.packages) {
  const layer = pkg.name === BINARY ? "binary" : CRATE_TO_LAYER[pkg.name];
  if (layer === undefined) {
    errors.push(`未登记的 crate "${pkg.name}" — 请加入 check-crate-layers.mjs 的 CRATE_TO_LAYER`);
    continue;
  }
  const rank = LAYERS[layer];

  for (const dep of pkg.dependencies) {
    const depLayer = dep.name === BINARY ? "binary" : CRATE_TO_LAYER[dep.name];
    if (depLayer === undefined) continue; // 外部依赖不校验
    const depRank = LAYERS[depLayer];

    // 规则 1: rank 方向 (仅运行时依赖; dev-dependency 只进测试, 不进发布依赖图。
    // 已知特例: data_plane 的测试经 dev-dep 借用 app_state 构造集成测试环境)
    if (depRank > rank && pkg.name !== BINARY && dep.kind !== "dev") {
      errors.push(
        `层级倒置: ${pkg.name}(${layer}, rank ${rank}) → ${dep.name}(${depLayer}, rank ${depRank})`,
      );
    }

    // 规则 2: cmd 隔离 (binary 例外)
    if (depLayer === CMD_LAYER && layer !== CMD_LAYER && pkg.name !== BINARY) {
      errors.push(
        `cmd 层被非命令层依赖: ${pkg.name}(${layer}) → ${dep.name}(${CMD_LAYER})`,
      );
    }

    // 规则 3: testkit 只能 dev-dependency
    if (dep.name === TESTKIT && dep.kind !== "dev" && pkg.name !== TESTKIT) {
      errors.push(`testkit 被非 dev 依赖: ${pkg.name} → testkit (kind=${dep.kind})`);
    }
  }
}

// workspace 成员必须全部登记
for (const pkg of meta.packages) {
  if (pkg.name !== BINARY && !(pkg.name in CRATE_TO_LAYER)) {
    // 已在上面报过, 这里跳过
  }
}

if (errors.length > 0) {
  console.error(`✗ crate 分层校验失败 (${errors.length} 处):\n`);
  for (const e of errors) console.error(`  - ${e}`);
  console.error("\n规则见 docs/architecture/crate-layers.md");
  process.exit(1);
}

const memberCount = meta.packages.length - 1;
console.log(`✓ crate 分层校验通过 — ${memberCount} 个 workspace crate + 1 个二进制, 依赖规则无违例`);
