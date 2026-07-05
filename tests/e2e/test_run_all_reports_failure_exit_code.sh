#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cat > "$TMP_DIR/test_pass.sh" <<'EOF'
#!/bin/bash
exit 0
EOF
chmod 755 "$TMP_DIR/test_pass.sh"

cat > "$TMP_DIR/test_fail.sh" <<'EOF'
#!/bin/bash
exit 7
EOF
chmod 755 "$TMP_DIR/test_fail.sh"

OUTPUT="$(
  TEST_E2E_DIR="$TMP_DIR" \
  bash "$ROOT/tests/e2e/run_all.sh" 2>&1 || true
)"

if [[ "$OUTPUT" != *"test_fail.sh ... FAIL (exit code 7)"* ]]; then
  echo "FAIL: run_all.sh did not report failing script exit code"
  echo "$OUTPUT"
  exit 1
fi

if [[ "$OUTPUT" != *"=== Results: 1 passed, 1 failed ==="* ]]; then
  echo "FAIL: run_all.sh did not report correct pass/fail totals"
  echo "$OUTPUT"
  exit 1
fi

echo "PASS: run_all.sh preserves failing script exit code in output"
