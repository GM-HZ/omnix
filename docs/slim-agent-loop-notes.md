# Slim Agent Loop — 实施笔记

## 失败操作及原因

### 1. Python 正则批量删除 match arm（不可行）

**尝试**: 用正则脚本删除 `CodexAuth::BedrockApiKey` 变体及所有 match arm。

**失败原因**:
- 正则无法精确匹配多行 match arm 的边界
- 留下孤立语法错误（trailing `|`、空 match arm）
- 误删了 `tui/onboarding/auth.rs:955`、`cli/doctor.rs:1326/2464`、`app-server/auth_mode.rs:11` 等 5 个文件的合法 arm
- 恢复方式: `git checkout` 所有受影响文件

**教训**: 枚举变体删除需手动逐文件处理，不可用正则脚本。

### 2. 批量删除 codex-api 模块（不可行）

**尝试**: 同时删除 `responses.rs` + `responses_websocket.rs` + `sse/responses.rs`，再修复 client.rs。

**失败原因**:
- 这 3 个模块的类型被 `client.rs` 中 6 个方法的**类型签名**引用
- `ResponsesClient`、`ResponsesWebsocketConnection` 等类型出现在函数参数和返回值中
- Rust 要求类型必须存在才能编译，即使函数体是 `unreachable!()`
- 方法间存在交叉引用链: `prewarm_auth → connect_websocket → stream_responses_websocket → stream_responses_api`

**教训**: 保留类型定义 + stub 方法体，而非删除整个模块。

### 3. Python 括号计数删除函数（不可行）

**尝试**: 用括号深度计数器定位 `stream_responses_api` 等 6 个方法的边界并删除。

**失败原因**:
- 方法体内部有嵌套的 `{` `}`（闭包、match、struct 字面量）
- 计数器找到了顶层闭合括号，但遗留了方法之间的交叉引用
- 错误: `cannot find value responses_metadata`、`no method named connect_websocket`

**教训**: 不可用脚本删除 2500 行文件中相互引用的大型方法块。

### 4. Edit 字符串未覆盖完整函数体（可避免）

**尝试**: 用 Edit 工具替换 `emit_feedback_request_tags_with_auth_env` 函数。

**失败原因**:
- `old_string` 只包含了函数体前 30 行，未覆盖完整 50+ 行体
- 尾部残留 `)` 等字符导致语法错误

**教训**: 大型函数替换前需先读取完整函数体。

### 5. 删除 `tokio_tungstenite` 导入（不可行）

**尝试**: 删除 `use tokio_tungstenite::tungstenite::{Error, Message}` 导入。

**失败原因**:
- `on_ws_event` 方法实现了 `WebsocketTelemetry` trait（来自 `codex-api/src/telemetry.rs`）
- 该 trait 的 `record_websocket_event` 方法签名要求 `Result<Message, Error>` 类型
- 类型依赖链: `client.rs → codex-api telemetry → tungstenite`

**教训**: trait 定义中的类型使用比方法体更难消除。

### 6. 删除 `keyring` workspace 依赖（可完成但需级联处理）

**尝试**: 从 workspace Cargo.toml 删除 `keyring = { version = "3.6" }`。

**失败原因（初版）**:
- `secrets/Cargo.toml`、`rmcp-client/Cargo.toml`、`login/Cargo.toml` 的 `[dev-dependencies]` 仍引用 `keyring = { workspace = true }`
- Cargo 解析时 workspace 依赖缺失即报错，不管是否为 dev-dependency

**最终方案**: 先 stub `keyring-store`（消除生产依赖），再逐个移除 3 个 crate 的 dev-dependency 并用 `std::io::Error` 替换测试中的 `KeyringError`，最后从 workspace 移除。

## 成功方案总结

| 策略 | 适用场景 | 示例 |
|------|----------|------|
| **保留类型 + stub 方法体** | 类型被其他模块签名引用 | `responses.rs: stream_encoded → Err(...)` |
| **完整类型重写（保持 API）** | 类型仅本模块使用 | `responses_websocket.rs: 1100→109 行` |
| **macro no-op** | macro 调用点分散 | `feedback_tags! → {}` |
| **函数体替换为 `{}`** | 函数签名保留即可 | `emit_feedback_auth_recovery_tags` |
| **模块替换为最小 stub** | 仅 export 少数类型 | `bedrock_api_key.rs: 260→40 行` |
| **先 stub 依赖，再删外部 crate** | 外部 crate 通过中间层使用 | `keyring-store stub → 删除 keyring` |

## 当前剩余事项（架构约束）

| 项 | 原因 | 预估工时 |
|----|------|----------|
| CodexAuth 枚举简化 | 4 枚举 × 8+ 文件，33 match arm 连锁 | 1-2 天 |
| feedback crate 删除 | 6 crate 引用，`metadata_layer` 返回 `impl Layer<S>` 泛型 | 2-3 天 |
| tungstenite patches | `WebsocketTelemetry` trait 签名绑定 `tungstenite::Message/Error` 类型 | 2-3 天 |
