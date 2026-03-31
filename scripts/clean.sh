#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "정리 중..."

cargo clean 2>/dev/null || true
rm -rf dist/

echo "완료."
