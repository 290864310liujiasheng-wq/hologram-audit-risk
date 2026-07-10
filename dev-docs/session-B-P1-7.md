# 会话B：P1-7（授权可整包复制共享）

**目标文件：`engine/src/entitlement.rs`、`engine/src/cli.rs`**  
**独立任务，可与会话A/C同时运行**

---

## 背景

当前授权签名的 canonical payload **不含 `device_id`**。  
这意味着把授权目录下3个文件（`entitlement.json` + `entitlement.sig` + `device_secret`）整包拷贝到另一台机器，签名依旧有效——一份 Pro 授权可发给无数台机器。

---

## 根因定位

**`engine/src/entitlement.rs`**（约第16-53行）：

```rust
// 签名时 canonical payload 里的字段列表
// 当前不含 device_id
let canonical = format!("{plan}:{valid_until}:{issued_at}:{user_id}");
//                                                          ^ device_id 缺席
```

**`engine/src/cli.rs`** device 校验（约第3585-3620行）：
```rust
// 目前 device 校验只是本地自比（拿存储的 device_id 比当前机器的 device_id）
// 但由于 device_id 不在签名里，拷走文件后连这个比较都可以跳过
```

---

## 必须做的修复

### 步骤1：把 `device_id` 纳入签名 payload

在 `engine/src/entitlement.rs` 里，找到构建 canonical payload 的地方，加入 `device_id`：

```rust
// 改前（近似）：
let canonical = format!("{plan}:{valid_until}:{issued_at}:{user_id}");

// 改后：
let canonical = format!("{plan}:{valid_until}:{issued_at}:{user_id}:{device_id}");
```

**注意**：这个改动会让所有已签发的旧授权验签失败（因为 canonical 格式变了）。需要处理：
- 兼容旧格式：检测 canonical 里是否含 device_id（可通过字段数判断），旧格式降级为 Core 并提示"请重新登录以绑定设备"。
- **或**（更简单）：直接让旧授权失效，要求重新登录。

### 步骤2：device 校验强化

在 `engine/src/cli.rs` 的 device 校验处（约第3585-3620行），确认：
1. 验签时用的 canonical 包含当前机器的 `device_id`
2. 若签名里的 `device_id` 与当前机器 `device_id` 不符，验签失败，降级 Core

```rust
// 验签逻辑要确保：
// 1. 从 entitlement.json 读出 device_id 字段
// 2. 获取当前机器的 device_id（现有函数）
// 3. 用 "plan:valid_until:issued_at:user_id:device_id_from_file" 做 canonical
// 4. 若 device_id_from_file != current_device_id → 拒绝（或降级 Core + 提示）
```

### 步骤3（加分项，不是必须）：release 构建下限制 `device_id` 伪造

确认 `get_device_id()` 实现不能被轻易替换（比如不依赖可随意设置的环境变量）。

---

## 验收标准

1. **正常流程不受影响**：`auth login` → `auth status` 显示 Pro，当前机器。
2. **拷贝模拟**：
   ```bash
   # 把授权文件拷到 /tmp/stolen_ent/
   cp -r $AUDIT_RISK_ENTITLEMENT_DIR /tmp/stolen_ent
   # 用 stolen 目录读取
   AUDIT_RISK_ENTITLEMENT_DIR=/tmp/stolen_ent audit-risk auth status
   # 必须显示 Core 或"设备不匹配"，不能显示 Pro
   ```
3. `cargo test` 全绿（auth相关测试仍过）。
4. `cargo +1.97.0 clippy --all-targets -- -D warnings` 无警告。

---

## 提交信息

```
fix(auth): bind entitlement signature to device_id to prevent cross-device copy
```
