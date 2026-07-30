#!/usr/bin/env bash
# 聚焦离线诊断：无 loopback / 无网络 / 无上游。
# 完整 SUITE-PY-OFFLINE 由 source gate 的固定 command_argv 执行；本 wrapper
# 故意不运行需要受控 Rust toolchain PATH 的 test_build_sidecar_identity。
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
if ! command -v python3 >/dev/null 2>&1; then
  echo "S0_LAYER offline env-blocked (no python3)"; exit 0
fi
if python3 -m unittest test.test_capability test.test_capability_catalog test.test_document_governance test.test_process_ownership_policy test.test_codex_browser_auth_contract test.test_profile_pin_contract -v; then
  echo "S0_LAYER offline pass"; exit 0
else
  echo "S0_LAYER offline fail"; exit 1
fi
