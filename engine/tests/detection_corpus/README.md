# detection_corpus — 检测质量语料

`detection_quality` 测试用它测量扫描器的召回率与误报率，作为回归基线。

- `bad/`   — 每个文件必须产生 ≥1 条 finding（召回率）
- `clean/` — 每个文件必须产生 0 条 finding（误报率）
- `gaps/`  — 真实风险但当前扫描器已知会漏（覆盖边界，纯参考，不计入门禁）

## 关于 `__CORPUS_SECRET__` 占位符

少数 `bad/` 样本（Slack webhook/token、Stripe key）若以真实形态提交，会触发
GitHub 推送保护。因此这些文件里用 `__CORPUS_SECRET__` 占位，真实触发值在
`detection_quality.rs` 的 `inject_secret()` 里用分片拼接还原后再喂给扫描器——
仓库里不留任何可被扫描器/GitHub 命中的连续密钥字面量，同时检测能力照测不误。
