#!/usr/bin/env bash
# 顺序执行，避免不同基准互抢 CPU；原始结果保存在已忽略的 target 内。
set -euo pipefail
# 固定基准的符号开销；不覆盖用户的 Release 调试/火焰图配置。
export CARGO_PROFILE_BENCH_DEBUG=0
export VOFA_SOAK_EVAL_STALL_MS=0 VOFA_SOAK_INGEST_STALL_MS=0
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
# 单项失败仍采集后续独立场景，最终统一返回失败，避免遗漏前端等审计结果。
audit_failures=0
audit_run() {
  local audit_log="$1"
  shift
  if "$@" 2>&1 | tee "$audit_out/$audit_log.log"; then
    return 0
  else
    local audit_status=$?
    audit_failures=$((audit_failures + 1))
    echo "场景失败: $audit_log (exit $audit_status)" >&2
  fi
}
cd "$audit_root/src-tauri"
cargo bench -p buffer_databuffer -p data_plane -p display -p gpu_core --no-run 2>&1 | tee "$audit_out/build.log"
for audit_spec in 'buffer_databuffer waveform_pyramid_bench' 'data_plane ingest_bench' 'data_plane eval_bench' 'display waveform_wire' 'gpu_core envelope_bench'; do
  read -r audit_package audit_bench <<< "$audit_spec"
  audit_run "$audit_bench" cargo bench -p "$audit_package" --bench "$audit_bench" -- --noplot
done
for audit_graph in raw math; do
  audit_run "soak-$audit_graph" env VOFA_SOAK_SECONDS="$audit_seconds" VOFA_SOAK_GRAPH="$audit_graph" VOFA_SOAK_GENERATOR=1 \
    cargo bench -p data_plane --bench pipeline_soak
done
audit_run soak-synthetic env VOFA_SOAK_SECONDS="$audit_seconds" VOFA_SOAK_GRAPH=math VOFA_SOAK_GENERATOR=0 \
  cargo bench -p data_plane --bench pipeline_soak
audit_run soak-stall-100ms env VOFA_SOAK_SECONDS=30 VOFA_SOAK_GRAPH=math VOFA_SOAK_GENERATOR=1 VOFA_SOAK_EVAL_STALL_MS=100 \
  cargo bench -p data_plane --bench pipeline_soak
audit_run soak-ingest-stall-500ms env VOFA_SOAK_SECONDS=30 VOFA_SOAK_GRAPH=math VOFA_SOAK_GENERATOR=1 VOFA_SOAK_INGEST_STALL_MS=500 \
  cargo bench -p data_plane --bench pipeline_soak
cd "$audit_root"
audit_run frontend pnpm exec vitest bench --run --outputJson "$audit_out/frontend.json"
if (( audit_failures > 0 )); then
  echo "审计未通过: $audit_failures 个场景失败；完整结果: $audit_out" >&2
  exit 1
fi
echo "完成测量: ${audit_out}（长跑已检查 >10 MB/s 与零丢弃；界面呈现仍须独立验收）"
