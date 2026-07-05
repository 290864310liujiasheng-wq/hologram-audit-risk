#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

ENTITLEMENT_DIR="$TMP_DIR/entitlement"
mkdir -p "$ENTITLEMENT_DIR"
WORKSPACE_DIR="$TMP_DIR/workspace"
mkdir -p "$WORKSPACE_DIR/.hologram"

cat > "$ENTITLEMENT_DIR/session.json" <<'EOF'
{
  "session_id": "session-from-file",
  "status": "pending",
  "created_at": "2026-06-27T00:00:00Z",
  "expires_at": "2999-01-01T00:00:00Z",
  "poll_interval_seconds": 2,
  "timeout_seconds": 300,
  "poll_url": "https://auth.example.com/api/auth/poll?session_id=session-from-file",
  "exchange_url": "https://auth.example.com/api/auth/exchange",
  "login_url": "https://auth.example.com/login?session_id=session-from-file"
}
EOF

cat > "$WORKSPACE_DIR/.hologram/delivery.json" <<'EOF'
{
  "auth": {
    "base_url": "https://delivery-auth.example.com"
  }
}
EOF

cat > "$ENTITLEMENT_DIR/entitlement.json" <<'EOF'
{
  "user_id": "user-from-file",
  "plan": "core_free",
  "features": [],
  "issued_at": "2026-06-27T00:00:00Z",
  "valid_until": "2999-01-01T00:00:00Z",
  "device_id": "device-from-file",
  "last_refresh_time": "2026-06-27T00:00:00Z",
  "status": "active",
  "payment_pending": true
}
EOF

FAKE_BIN="$TMP_DIR/bin"
mkdir -p "$FAKE_BIN"
cat > "$FAKE_BIN/curl" <<'EOF'
#!/bin/bash
echo "$*"
EOF
chmod 755 "$FAKE_BIN/curl"

SUMMARY_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  WORKSPACE_ROOT="$WORKSPACE_DIR" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="summary" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

if [[ "$SUMMARY_OUTPUT" != *"session_id: session-from-file"* ]]; then
  echo "FAIL: summary did not include auto-discovered session_id"
  exit 1
fi

if [[ "$SUMMARY_OUTPUT" != *"user_id: user-from-file"* ]]; then
  echo "FAIL: summary did not include auto-discovered user_id"
  exit 1
fi

if [[ "$SUMMARY_OUTPUT" != *"device_id: device-from-file"* ]]; then
  echo "FAIL: summary did not include auto-discovered device_id"
  exit 1
fi

POLL_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  AUTH_BASE_URL="https://auth.example.com" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="poll" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

if [[ "$POLL_OUTPUT" != *"/api/auth/poll?session_id=session-from-file"* ]]; then
  echo "FAIL: poll step did not auto-read session_id from session.json"
  exit 1
fi

DELIVERY_POLL_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  WORKSPACE_ROOT="$WORKSPACE_DIR" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="poll" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

if [[ "$DELIVERY_POLL_OUTPUT" != *"https://delivery-auth.example.com/api/auth/poll?session_id=session-from-file"* ]]; then
  echo "FAIL: poll step did not prefer delivery.json.auth.base_url"
  exit 1
fi

QUERY_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  AUTH_BASE_URL="https://auth.example.com" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="payment_query" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

if [[ "$QUERY_OUTPUT" != *"/api/payment/query?user_id=user-from-file&device_id=device-from-file"* ]]; then
  echo "FAIL: payment_query did not auto-read user_id/device_id from entitlement.json"
  exit 1
fi

REFRESH_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  AUTH_BASE_URL="https://auth.example.com" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="refresh" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

if [[ "$REFRESH_OUTPUT" != *'"user_id":"user-from-file","device_id":"device-from-file"'* ]]; then
  echo "FAIL: refresh did not auto-read user_id/device_id from entitlement.json"
  exit 1
fi

EVIDENCE_OUTPUT="$(
  PATH="$FAKE_BIN:$PATH" \
  WORKSPACE_ROOT="$WORKSPACE_DIR" \
  AUDIT_RISK_ENTITLEMENT_DIR="$ENTITLEMENT_DIR" \
  VERIFY_STEP="evidence_template" \
  "$ROOT/scripts/auth-payment-live-verification.sh"
)"

EVIDENCE_FILE="$TMP_DIR/evidence.json"
printf '%s\n' "$EVIDENCE_OUTPUT" > "$EVIDENCE_FILE"

python3 - <<'EOF' "$EVIDENCE_FILE"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["meta"]["base_url"] == "https://delivery-auth.example.com"
assert payload["cli"]["login"]["session_id"] == "session-from-file"
assert payload["http"]["poll"]["session_id"] == "session-from-file"
assert payload["http"]["payment_query"]["user_id"] == "user-from-file"
assert payload["http"]["payment_query"]["device_id"] == "device-from-file"
assert payload["http"]["refresh"]["user_id"] == "user-from-file"
assert payload["http"]["refresh"]["device_id"] == "device-from-file"
EOF

echo "PASS: auth/payment live verification script auto-discovers local session and entitlement fields"
