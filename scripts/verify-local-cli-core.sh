#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TMP_WS="$(mktemp -d)"
trap 'rm -rf "$TMP_WS"' EXIT

echo "[verify-local-cli-core] repo: $REPO_ROOT"
echo "[verify-local-cli-core] tmp workspace: $TMP_WS"

cd "$REPO_ROOT"

echo "[1/7] cargo test cli::tests::"
cargo test --manifest-path engine/Cargo.toml cli::tests:: -- --nocapture

echo "[2/7] auth/payment sample semantics"
python3 -m unittest tests.test_auth_payment_live_samples

echo "[3/7] auth/payment live verification script e2e"
bash tests/e2e/test_auth_payment_live_verification.sh

echo "[4/7] e2e aggregator regression"
bash tests/e2e/test_run_all_reports_failure_exit_code.sh

echo "[5/7] audit-risk init"
cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- init "$TMP_WS"

echo "[6/7] audit-risk doctor"
cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- doctor "$TMP_WS"

echo "[7/7] audit-risk report"
cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- report "$TMP_WS" --fail-on off --json

echo "[verify-local-cli-core] PASS"
