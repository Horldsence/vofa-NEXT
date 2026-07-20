#!/usr/bin/env bash
set -euo pipefail

# Build the egui desktop app (release mode).
# Usage: ./scripts/build.sh [target]

TARGET="${1:-}"

if [ -n "$TARGET" ]; then
  cargo build --release -p vofa-next-app --target "$TARGET"
else
  cargo build --release -p vofa-next-app
fi
