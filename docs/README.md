# 交付与接入文档

- [phase5-delivery.md](phase5-delivery.md)：AI 编码风控平台的初始化、接入、CI/hook 和管理员导出路径。
- [auth-payment-live-samples.json](auth-payment-live-samples.json)：auth/payment 合同样例；用于字段形状对齐，不代表真实远端已验收。
- `../scripts/auth-payment-live-verification.sh`：真实 auth/payment 联调脚本模板；支持 `summary / cli_login / cli_status / observe_gate / poll / exchange / payment_query / refresh / evidence_template`。
- `../scripts/verify-local-cli-core.sh`：本地 CLI / Core 产品验收脚本；会跑 CLI 单测、样例校验、脚本 e2e，以及 `init / doctor / report` smoke。
