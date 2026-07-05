# Personal Pro 授权合同

生成日期：2026-06-27

## 单句定义

本文件定义 `audit-risk` CLI 在个人版 29 元/月场景下的浏览器登录、支付确认、entitlement 本地缓存、刷新与撤销的当前内部合同真源。

## 产品边界

- Core 免费版保留：零参数中文首页、`help/tour`、`init`、`doctor`、`check`、`watch`、`diff`、基础白话解释、基础修复建议与 diff 预览、修复方案二次审计验证、基础规则包、JSON/Markdown 基础报告。
- Pro 个人版解锁：高级规则包、历史风险对比、增强报告导出、审计日志哈希链签名导出包、`observe`、`notify`、个人规则自定义加载。
- `observe`、`notify`、`watch --observe` 必须由 entitlement gate 控制；修复验证保持免费。

## 服务端接口合同

固定入口：

- `POST /api/auth/send-code`
- `POST /api/auth/verify-code`
- `GET /api/auth/poll`
- `POST /api/auth/exchange`
- `POST /api/entitlement/refresh`
- `GET /api/payment/query`
- `POST /api/payment/wechat/callback`
- `POST /api/payment/alipay/callback`

当前 CLI owner 已消费的最小响应合同：

```ts
interface AuthPollResponse {
  status: string;
  auth_token?: string;
}

interface PersonalEntitlement {
  user_id: string;
  plan: string;
  features: string[];
  issued_at: string;
  valid_until: string;
  device_id: string;
  last_refresh_time: string;
  status: string;
  payment_pending?: boolean;
  next_billing_at?: string;
}

interface AuthEntitlementEnvelope {
  entitlement: PersonalEntitlement;
  signature: string;
}
```

## 本地缓存合同

目录：

- `{app_support_dir}/audit-risk/entitlement/`

最少文件：

- `device_secret`
- `session.json`
- `entitlement.json`
- `entitlement.sig`

`session.json` 当前合同：

```ts
interface AuthSessionDocument {
  session_id: string;
  status: string;
  created_at: string;
  expires_at: string;
  poll_interval_seconds: number;
  timeout_seconds: number;
  poll_url: string;
  exchange_url: string;
  login_url: string;
}
```

补充要求：

- 未配置 auth 服务时，`poll_url / exchange_url / login_url` 默认指向 `https://auth.audit-risk.local/...` 占位入口。
- 配置了 `delivery.json.auth.base_url` 或 `AUDIT_RISK_AUTH_BASE_URL` 后，`session.json` 里的 `poll_url / exchange_url / login_url` 必须同步切到该服务地址，不能继续保留默认占位域名。

`device_id` 规则：

- `SHA256(device_secret + os + hostname)`

## CLI 状态机

- `missing`
- `active`
- `grace`
- `expired`
- `revoked`
- `device_mismatch`
- `invalid`

补充状态：

- `payment_pending=true` 时，`auth status` 必须显示“支付确认中”，不能退回泛化未登录。
- `session.status=pending` 且 entitlement 缺失时，`auth status` 必须显示“登录进行中”。

## CLI 行为合同

`auth login`

- 未配置 auth 服务时：只生成本地 `session.json` / `device_secret`，不伪造 Pro 成功。
- 配置 auth 服务时：执行 `poll -> exchange`；如果 exchange 已返回 Pro，直接落盘 entitlement。
- exchange 不是 Pro 时：继续走 `GET /api/payment/query` 兜底。
- payment query 超时：返回中文“支付确认中，请稍后运行 audit-risk auth status 查看状态”，并把 `payment_pending=true` 的基础 entitlement 留在本地。

`auth status`

- 必须区分：未登录、登录进行中、支付确认中、已登录、授权已过期、授权已撤销、设备绑定异常、授权文件无效。

`auth logout`

- 删除 `session.json`、`entitlement.json`、`entitlement.sig`。
- 不删除 `device_secret`。

## 诊断合同

`audit-risk doctor` 至少暴露：

- `auth_service`
- `entitlement_cache`

其中 `auth_service` 失败时必须返回结构化错误码：

- `network_unreachable`
- `bad_json`
- `timeout`
- `auth_service_error`

## 当前验证边界

- 当前仓库已完成 CLI owner、本地缓存、状态机、支付查询兜底、refresh/revoked、doctor 诊断。
- 当前仓库仍未完成真实远端样本验收；`mock://...` 只用于本地合同验证，不是发布级证据。
