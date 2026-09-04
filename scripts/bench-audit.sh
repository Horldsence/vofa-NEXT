#!/usr/bin/env bash
# 顺序执行，避免不同基准互抢 CPU；原始结果保存在已忽略的 target 内。
set -euo pipefail
audit_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
audit_mode="${1:-quick}"
case "$audit_mode" in
  quick) audit_seconds=60 ;;
  full) audit_seconds=300 ;;
  *) echo '用法: bash scripts/bench-audit.sh [quick|full]' >&2; exit 2 ;;
esac
audit_out="$audit_root/src-tauri/target/bench-audit/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$audit_out"
echo "基准输出: $audit_out"
cd "$audit_root/src-tauri"
cargo bench -p buffer_databuffer -p data_plane -p display -p gpu_core --no-run 2>&1 | tee "$audit_out/build.log"
for audit_spec in 'buffer_databuffer waveform_pyramid_bench' 'data_plane ingest_bench' 'data_plane eval_bench' 'display waveform_wire' 'gpu_core envelope_bench'; do
  read -r audit_package audit_bench <<< "$audit_spec"
  cargo bench -p "$audit_package" --bench "$audit_bench" -- --noplot 2>&1 | tee "$audit_out/$audit_bench.log"
done
for audit_graph in raw math; do
  VOFA_SOAK_SECONDS="$audit_seconds" VOFA_SOAK_GRAPH="$audit_graph" VOFA_SOAK_GENERATOR=1 \
    cargo bench -p data_plane --bench pipeline_soak 2>&1 | tee "$audit_out/soak-$audit_graph.log"
done
VOFA_SOAK_SECONDS="$audit_seconds" VOFA_SOAK_GRAPH=math VOFA_SOAK_GENERATOR=0 \
  cargo bench -p data_plane --bench pipeline_soak 2>&1 | tee "$audit_out/soak-synthetic.log"
cd "$audit_root"
pnpm exec vitest bench --run --outputJson "$audit_out/frontend.json" 2>&1 | tee "$audit_out/frontend.log"
echo "完成测量: $audit_out（性能是否达标请检查丢弃、延迟与吞吐，不以退出码替代）"
