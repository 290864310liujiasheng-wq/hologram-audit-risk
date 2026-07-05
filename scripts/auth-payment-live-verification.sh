#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

STEP="${VERIFY_STEP:-summary}"
AUTH_BASE_URL="${AUTH_BASE_URL:-}"
WORKSPACE_ROOT="${WORKSPACE_ROOT:-$REPO_ROOT}"
AUDIT_RISK_ENTITLEMENT_DIR="${AUDIT_RISK_ENTITLEMENT_DIR:-/tmp/audit-risk-auth-live}"
USE_SYSTEM_AUDIT_RISK="${USE_SYSTEM_AUDIT_RISK:-0}"

SESSION_ID="${SESSION_ID:-}"
AUTH_TOKEN="${AUTH_TOKEN:-}"
USER_ID="${USER_ID:-}"
DEVICE_ID="${DEVICE_ID:-}"

ORDER_ID="${ORDER_ID:-}"
AMOUNT_CENTS="${AMOUNT_CENTS:-}"
CURRENCY="${CURRENCY:-}"
BILLING_PERIOD="${BILLING_PERIOD:-}"
CHANNEL="${CHANNEL:-}"
NEXT_BILLING_AT="${NEXT_BILLING_AT:-}"

usage() {
  cat <<'EOF'
auth-payment-live-verification.sh

用途：
  把 dev-docs/auth-payment-live-verification.md 里的 live 验收步骤收口成可执行模板。

模式：
  VERIFY_STEP=summary
  VERIFY_STEP=cli_login
  VERIFY_STEP=cli_status
  VERIFY_STEP=observe_gate
  VERIFY_STEP=poll
  VERIFY_STEP=exchange
  VERIFY_STEP=payment_query
  VERIFY_STEP=refresh
  VERIFY_STEP=evidence_template

关键环境变量：
  AUTH_BASE_URL               真实 auth/payment 服务地址；未显式传入时，优先尝试从 WORKSPACE_ROOT/.hologram/delivery.json 读取
  WORKSPACE_ROOT              要跑 observe gate 的 workspace，默认当前 repo；同时也是 delivery.json.auth.base_url 的读取根目录
  AUDIT_RISK_ENTITLEMENT_DIR  本地 entitlement 目录，默认 /tmp/audit-risk-auth-live
  USE_SYSTEM_AUDIT_RISK=1     使用系统 audit-risk；默认走 cargo run --bin audit-risk

curl 步骤额外需要：
  SESSION_ID   对应 poll；未显式传入时会尝试从 AUDIT_RISK_ENTITLEMENT_DIR/session.json 读取
  AUTH_TOKEN   对应 exchange
  USER_ID      对应 payment_query / refresh；未显式传入时会尝试从 entitlement.json 读取
  DEVICE_ID    对应 exchange / payment_query / refresh；payment_query / refresh 未显式传入时会尝试从 entitlement.json 读取

29 元支付证据建议额外记录：
  ORDER_ID AMOUNT_CENTS CURRENCY BILLING_PERIOD CHANNEL NEXT_BILLING_AT
EOF
}

require_var() {
  local name="$1"
  if [ -z "${!name:-}" ]; then
    echo "missing required env: $name" >&2
    exit 2
  fi
}

run_audit_risk() {
  if [ "$USE_SYSTEM_AUDIT_RISK" = "1" ]; then
    audit-risk "$@"
  else
    cargo run --manifest-path "$REPO_ROOT/engine/Cargo.toml" --bin audit-risk -- "$@"
  fi
}

extract_json_string() {
  local file="$1"
  local key="$2"
  sed -nE "s/.*\"$key\"[[:space:]]*:[[:space:]]*\"([^\"]+)\".*/\\1/p" "$file" | head -n 1
}

resolve_auth_base_url() {
  if [ -n "$AUTH_BASE_URL" ]; then
    return
  fi

  local delivery_file="$WORKSPACE_ROOT/.hologram/delivery.json"
  if [ -f "$delivery_file" ]; then
    AUTH_BASE_URL="$(sed -nE 's/.*"base_url"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "$delivery_file" | head -n 1)"
  fi
}

autofill_from_session() {
  local session_file="$AUDIT_RISK_ENTITLEMENT_DIR/session.json"
  if [ -z "$SESSION_ID" ] && [ -f "$session_file" ]; then
    SESSION_ID="$(extract_json_string "$session_file" "session_id")"
  fi
}

autofill_from_entitlement() {
  local entitlement_file="$AUDIT_RISK_ENTITLEMENT_DIR/entitlement.json"
  if [ -f "$entitlement_file" ]; then
    if [ -z "$USER_ID" ]; then
      USER_ID="$(extract_json_string "$entitlement_file" "user_id")"
    fi
    if [ -z "$DEVICE_ID" ]; then
      DEVICE_ID="$(extract_json_string "$entitlement_file" "device_id")"
    fi
  fi
}

print_summary() {
  resolve_auth_base_url
  autofill_from_session
  autofill_from_entitlement
  cat <<EOF
Live 验收摘要
  repo_root: $REPO_ROOT
  workspace_root: $WORKSPACE_ROOT
  entitlement_dir: $AUDIT_RISK_ENTITLEMENT_DIR
  auth_base_url: ${AUTH_BASE_URL:-<missing>}
  session_id: ${SESSION_ID:-<missing>}
  user_id: ${USER_ID:-<missing>}
  device_id: ${DEVICE_ID:-<missing>}
  use_system_audit_risk: $USE_SYSTEM_AUDIT_RISK

当前脚本不会伪造任何远端结果。
真实支付闭环仍需要外部证据：
  order_id: ${ORDER_ID:-<missing>}
  amount_cents: ${AMOUNT_CENTS:-<missing>}
  currency: ${CURRENCY:-<missing>}
  billing_period: ${BILLING_PERIOD:-<missing>}
  channel: ${CHANNEL:-<missing>}
  next_billing_at: ${NEXT_BILLING_AT:-<missing>}

推荐顺序：
  1. VERIFY_STEP=cli_login AUTH_BASE_URL=... ./scripts/auth-payment-live-verification.sh
  2. 从 $AUDIT_RISK_ENTITLEMENT_DIR/session.json 读取 session_id / login_url
  3. VERIFY_STEP=poll SESSION_ID=... ./scripts/auth-payment-live-verification.sh
  4. VERIFY_STEP=exchange AUTH_TOKEN=... DEVICE_ID=... ./scripts/auth-payment-live-verification.sh
  5. VERIFY_STEP=payment_query ./scripts/auth-payment-live-verification.sh
  6. VERIFY_STEP=refresh ./scripts/auth-payment-live-verification.sh
  7. VERIFY_STEP=cli_status ./scripts/auth-payment-live-verification.sh
  8. VERIFY_STEP=observe_gate WORKSPACE_ROOT=... ./scripts/auth-payment-live-verification.sh
  9. VERIFY_STEP=evidence_template ./scripts/auth-payment-live-verification.sh > auth-payment-live-evidence.json
EOF
}

print_evidence_template() {
  resolve_auth_base_url
  autofill_from_session
  autofill_from_entitlement
  cat <<EOF
{
  "meta": {
    "sample_kind": "live_capture",
    "captured_from": "",
    "captured_at": "",
    "base_url": "${AUTH_BASE_URL:-}",
    "workspace_root": "$WORKSPACE_ROOT",
    "entitlement_dir": "$AUDIT_RISK_ENTITLEMENT_DIR"
  },
  "cli": {
    "login": {
      "session_id": "${SESSION_ID:-}",
      "stdout_path": "",
      "notes": ""
    },
    "status": {
      "stdout_path": "",
      "expected_label": ""
    },
    "observe_gate": {
      "stdout_path": "",
      "allowed": null
    }
  },
  "http": {
    "poll": {
      "session_id": "${SESSION_ID:-}",
      "response_path": ""
    },
    "exchange": {
      "auth_token": "${AUTH_TOKEN:-}",
      "device_id": "${DEVICE_ID:-}",
      "response_path": ""
    },
    "payment_query": {
      "user_id": "${USER_ID:-}",
      "device_id": "${DEVICE_ID:-}",
      "response_path": ""
    },
    "refresh": {
      "user_id": "${USER_ID:-}",
      "device_id": "${DEVICE_ID:-}",
      "response_path": ""
    }
  },
  "payment": {
    "order_id": "${ORDER_ID:-}",
    "amount_cents": "${AMOUNT_CENTS:-}",
    "currency": "${CURRENCY:-}",
    "billing_period": "${BILLING_PERIOD:-}",
    "channel": "${CHANNEL:-}",
    "next_billing_at": "${NEXT_BILLING_AT:-}",
    "callback_path": "",
    "revoked_sample_path": ""
  }
}
EOF
}

run_cli_login() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  rm -rf "$AUDIT_RISK_ENTITLEMENT_DIR"
  mkdir -p "$AUDIT_RISK_ENTITLEMENT_DIR"
  AUDIT_RISK_ENTITLEMENT_DIR="$AUDIT_RISK_ENTITLEMENT_DIR" \
  AUDIT_RISK_AUTH_BASE_URL="$AUTH_BASE_URL" \
  run_audit_risk auth login

  local session_file="$AUDIT_RISK_ENTITLEMENT_DIR/session.json"
  if [ -f "$session_file" ]; then
    echo
    echo "session.json: $session_file"
    echo "session_id: $(extract_json_string "$session_file" "session_id")"
    echo "login_url: $(extract_json_string "$session_file" "login_url")"
    echo "poll_url: $(extract_json_string "$session_file" "poll_url")"
    echo "exchange_url: $(extract_json_string "$session_file" "exchange_url")"
  fi
}

run_cli_status() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  AUDIT_RISK_ENTITLEMENT_DIR="$AUDIT_RISK_ENTITLEMENT_DIR" \
  AUDIT_RISK_AUTH_BASE_URL="$AUTH_BASE_URL" \
  run_audit_risk auth status
}

run_observe_gate() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  AUDIT_RISK_ENTITLEMENT_DIR="$AUDIT_RISK_ENTITLEMENT_DIR" \
  AUDIT_RISK_AUTH_BASE_URL="$AUTH_BASE_URL" \
  run_audit_risk observe "$WORKSPACE_ROOT"
}

run_poll() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  autofill_from_session
  require_var SESSION_ID
  curl -sS "$AUTH_BASE_URL/api/auth/poll?session_id=$SESSION_ID"
  echo
}

run_exchange() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  require_var AUTH_TOKEN
  require_var DEVICE_ID
  curl -sS -X POST \
    -H "Content-Type: application/json" \
    "$AUTH_BASE_URL/api/auth/exchange" \
    -d "{\"auth_token\":\"$AUTH_TOKEN\",\"device_id\":\"$DEVICE_ID\"}"
  echo
}

run_payment_query() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  autofill_from_entitlement
  require_var USER_ID
  require_var DEVICE_ID
  curl -sS "$AUTH_BASE_URL/api/payment/query?user_id=$USER_ID&device_id=$DEVICE_ID"
  echo
}

run_refresh() {
  resolve_auth_base_url
  require_var AUTH_BASE_URL
  autofill_from_entitlement
  require_var USER_ID
  require_var DEVICE_ID
  curl -sS -X POST \
    -H "Content-Type: application/json" \
    "$AUTH_BASE_URL/api/entitlement/refresh" \
    -d "{\"user_id\":\"$USER_ID\",\"device_id\":\"$DEVICE_ID\"}"
  echo
}

case "$STEP" in
  summary)
    print_summary
    ;;
  cli_login)
    run_cli_login
    ;;
  cli_status)
    run_cli_status
    ;;
  observe_gate)
    run_observe_gate
    ;;
  poll)
    run_poll
    ;;
  exchange)
    run_exchange
    ;;
  payment_query)
    run_payment_query
    ;;
  refresh)
    run_refresh
    ;;
  evidence_template)
    print_evidence_template
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    echo "unknown VERIFY_STEP: $STEP" >&2
    usage >&2
    exit 2
    ;;
esac
