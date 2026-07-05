# Auth / Payment 真实样本验收

生成日期：2026-06-27

## 目的

当前仓库已经具备 CLI 侧浏览器登录、payment query 兜底、refresh/revoked、本地 entitlement 缓存、doctor 诊断与配置真源，但仓库内没有真实 auth/payment 服务端实现。本文件用于固定“拿到真实服务后怎么验”的顺序、命令与期望结果，避免后续 agent 继续靠聊天上下文回忆。

脚本模板：

- 默认可直接运行 `./scripts/auth-payment-live-verification.sh`
- 默认 `VERIFY_STEP=summary`，会打印当前需要的环境变量、CLI 步骤、支付证据清单，以及本地自动发现到的 `session_id / user_id / device_id`
- 实际联调时，再切到 `cli_login / cli_status / observe_gate / poll / exchange / payment_query / refresh / evidence_template`
- `AUTH_BASE_URL` 若未显式传入，脚本会优先尝试从 `WORKSPACE_ROOT/.hologram/delivery.json.auth.base_url` 读取，再回退到环境变量
- `poll` 若未显式传 `SESSION_ID`，会尝试从 `AUDIT_RISK_ENTITLEMENT_DIR/session.json` 读取；`payment_query / refresh` 若未显式传 `USER_ID / DEVICE_ID`，会尝试从 `entitlement.json` 读取
- `evidence_template` 会输出一份结构化 JSON 骨架，把当前自动发现到的 `base_url / session_id / user_id / device_id` 带进去，供真实联调后回填命令输出、HTTP 返回和订单证据

## 前置条件

- 已有可访问的真实服务端地址，例如 `AUTH_BASE_URL=https://auth.example.com`
- 服务端已经实现：
  - `GET /api/auth/poll`
  - `POST /api/auth/exchange`
  - `POST /api/entitlement/refresh`
  - `GET /api/payment/query`
- 服务端或支付后台可以提供 29 元/月订单证据：
  - `amount_cents=2900`
  - `currency=CNY`
  - `billing_period=monthly`
  - `channel=wechat | alipay`
  - `order_id` 或支付平台交易号
  - 支付成功回调已更新 entitlement 的时间戳
- 本地可写 `AUDIT_RISK_ENTITLEMENT_DIR`

## CLI 验收顺序

1. 初始化一个空本地 entitlement 目录：
   - `export AUDIT_RISK_ENTITLEMENT_DIR=/tmp/audit-risk-auth-live`
   - `rm -rf "$AUDIT_RISK_ENTITLEMENT_DIR" && mkdir -p "$AUDIT_RISK_ENTITLEMENT_DIR"`
2. 配置 auth 服务地址：
   - 推荐写进 workspace `.hologram/delivery.json.auth.base_url`
   - 临时也可用环境变量：`export AUDIT_RISK_AUTH_BASE_URL="$AUTH_BASE_URL"`
3. 跑 `audit-risk auth login`
   - 期望：生成 `session.json` / `device_secret`
   - 若服务端已可直连：应继续走 `poll -> exchange`
4. 跑 `audit-risk auth status`
   - 按服务端返回落在 `登录进行中 / 支付确认中 / 已登录 / 授权已撤销 / 授权已过期 / 设备绑定异常`
5. 准备 stale entitlement 后跑 `audit-risk auth status`
   - 期望：触发 `POST /api/entitlement/refresh`
6. 在已授权状态下跑 `audit-risk observe <workspace>`
   - 期望：不再被 Pro gate 拦截

对应脚本模板：

```bash
VERIFY_STEP=cli_login ./scripts/auth-payment-live-verification.sh
```

## 29 元/月支付闭环验收

本仓库没有真实订单系统、微信支付、支付宝或 auth/payment 服务端实现，因此以下证据必须来自真实服务端和支付平台，不能用 `mock://...` 代替。

必须采集并写回验收记录：

- 订单创建或支付后台记录：金额为 `29.00 CNY`，周期为 `monthly`，产品为 `pro_personal_monthly`。
- 支付渠道：微信支付或支付宝至少跑通一条真实成功样本；如果两条渠道都声明支持，则两条都要采集。
- 回调处理：`POST /api/payment/wechat/callback` 或 `POST /api/payment/alipay/callback` 后，服务端能把同一 `user_id/device_id` 的 entitlement 更新为 `plan=pro_personal_monthly`。
- 查询兜底：`GET /api/payment/query` 对同一订单能返回最终 Pro entitlement；若仍未确认，必须返回 `plan=core_free` 且 `payment_pending=true`。
- 续费字段：成功样本必须包含 `next_billing_at`，并与支付平台的月付周期一致。
- 取消或撤销：真实取消/解约/退款样本必须能让 refresh 或 query 返回 `status=revoked`，CLI `auth status` 显示“授权已撤销”。

建议把真实样本追加到 `docs/auth-payment-live-samples.json` 时补 provenance：

```json
{
  "_meta": {
    "sample_kind": "live_capture",
    "captured_from": "staging-auth-payment",
    "captured_at": "2026-06-27T00:00:00Z",
    "base_url": "https://auth.example.com"
  }
}
```

也可以直接先生成一份联调证据骨架：

```bash
VERIFY_STEP=evidence_template ./scripts/auth-payment-live-verification.sh > auth-payment-live-evidence.json
```

## curl 验收顺序

### 1. poll

```bash
VERIFY_STEP=poll SESSION_ID="<session_id>" ./scripts/auth-payment-live-verification.sh

curl -sS "$AUTH_BASE_URL/api/auth/poll?session_id=<session_id>"
```

期望最小 JSON：

```json
{
  "status": "approved",
  "auth_token": "opaque-token"
}
```

### 2. exchange

```bash
VERIFY_STEP=exchange AUTH_TOKEN="opaque-token" DEVICE_ID="sha256-device-id" \
  ./scripts/auth-payment-live-verification.sh

curl -sS -X POST \
  -H "Content-Type: application/json" \
  "$AUTH_BASE_URL/api/auth/exchange" \
  -d '{
    "auth_token": "opaque-token",
    "device_id": "sha256-device-id"
  }'
```

期望最小 JSON：

```json
{
  "entitlement": {
    "user_id": "user-1",
    "plan": "pro_personal_monthly",
    "features": ["observe", "notify"],
    "issued_at": "2026-06-27T00:00:00Z",
    "valid_until": "2026-08-01T00:00:00Z",
    "device_id": "sha256-device-id",
    "last_refresh_time": "2026-06-27T00:00:00Z",
    "status": "active",
    "next_billing_at": "2026-07-31T00:00:00Z"
  },
  "signature": "base64-signature"
}
```

### 3. payment query 兜底

当 exchange 返回的 `plan != pro_personal_monthly` 时，再执行：

```bash
VERIFY_STEP=payment_query ./scripts/auth-payment-live-verification.sh

curl -sS "$AUTH_BASE_URL/api/payment/query?user_id=<user_id>&device_id=<device_id>"
```

期望两种结果：

- 成功补 Pro：返回 `plan=pro_personal_monthly`
- 仍未完成：返回 `plan=core_free` 且 `payment_pending=true`，CLI 应提示“支付确认中”

### 4. entitlement refresh

```bash
VERIFY_STEP=refresh ./scripts/auth-payment-live-verification.sh

curl -sS -X POST \
  -H "Content-Type: application/json" \
  "$AUTH_BASE_URL/api/entitlement/refresh" \
  -d '{
    "user_id": "user-1",
    "device_id": "sha256-device-id"
  }'
```

期望两种结果：

- active refresh：返回更新后的 entitlement 与新 signature
- revoked refresh：返回 `status=revoked`

## 验收结果判定

- `poll` 成功且 `auth_token` 存在：合同链路通过一段
- `exchange` 成功并落本地 `entitlement.json / entitlement.sig`：合同链路通过一段
- `payment query` 把非 Pro 升成 Pro，或在超时时落本地 `payment_pending=true`：支付兜底通过
- `refresh` 能把 stale entitlement 升级，或把 revoked 状态写回本地：refresh/revoked 通过
- `observe` 在已授权状态下不再卡在 Pro gate：Pro gate 放行通过

## 当前仓库边界

- 当前仓库内没有真实 auth/payment 服务端代码或运行入口。
- 当前仓库只能提供 CLI 侧合同、mock 验证、文档真源和验收脚本。
- 拿到真实服务后，应按本文件执行 fresh 验收，再把命令输出与返回样本写回 `dev-docs/acceptance.md`。
- 29 元/月订单、支付渠道回调、续费和取消/撤销样本必须依赖真实服务端与支付平台状态；不能在当前仓库内用本地 mock 证明完成。
