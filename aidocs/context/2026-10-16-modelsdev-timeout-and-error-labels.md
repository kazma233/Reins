# models.dev 拉取：「不是 JSON」误报根因与修复

- 日期：2026-10-16
- 性质：排查记录 + 决策记录。覆盖根因、修复方案、回归测试 seam 与文案取舍。
- 范围：`src-tauri/src/providers/catalog.rs` 的 `load_modelsdev_data` 网络层，以及 `src-tauri/src/tests/providers_catalog.rs` 的网络层回归测试。

## 现象

用户在前端「从 models.dev 补全」对话框里看到 "models.dev 返回的不是 JSON"，但上游接口实际返回的是合法 JSON。

## 根因

`catalog.rs` 此前 `REQUEST_TIMEOUT = Duration::from_secs(10)` 是聚合平台模型列表与 models.dev 目录共用的超时。`https://models.dev/api.json` 响应在 5MB 量级，本机实测完整下载 ~29.7s。

10s 超时把 `reqwest::blocking::Response::json()` 在读 body 流时截断。该方法内部错误带 `kind: Request` + `source: TimedOut`，被上游 `.context("models.dev 返回的不是 JSON")?` 二次包装，于是「请求超时」与「真非 JSON」共用同一条错误串，用户与维护者都看不出真实原因。

证据链：

| 工具 | 结果 |
|---|---|
| `curl https://models.dev/api.json` 全量下载 | `time_total=29.72s size=5,676,881B http=200`，`python json.load` 成功解析 230 个 provider |
| `curl --max-time 15` | 收到 3,161,674 字节，body 末尾停在 `"reasoning":t`（JSON 中段被砍） |
| Rust 复现脚本（`reqwest::blocking::Client::timeout(10s)`） | `status=200`，`response.json()` 在 10.6s 抛 `kind: Request, source: TimedOut` |

Cloudflare 端实情：`cf-cache-status: HIT`、`content-type: application/json`、无 `content-encoding` 头，响应是缓存命中的真体；问题在 reqwest 这边。

## 决策

### 1. 拆开两个超时

聚合平台模型列表（KB 级）与 models.dev 目录（MB 级）不再共用同一个常量：

```rust
const PROVIDER_MODELS_TIMEOUT: Duration = Duration::from_secs(10);
const MODELSDEV_TIMEOUT: Duration = Duration::from_secs(45);
```

聚合平台列表保留 10s 防止挂起服务商卡住用户；models.dev 提升到 45s 覆盖冷下载 + 安全余量。`build_client(timeout: Duration)` 接受超时参数。

### 2. 抽 `fetch_modelsdev` 网络层 + 错误分支

`fetch_modelsdev(client, url)` 走 `bytes()` + `serde_json::from_slice()`，与 `fetch_provider_models_inner` 同模式，错误按 `reqwest::Error::is_timeout / is_connect` 分层：

- 超时 → `拉取 models.dev 目录失败：请求超时（>45s），可稍后重试或点「从 models.dev 补全」刷新。`
- 无法连接 → `拉取 models.dev 目录失败：无法连接到上游（<reqwest 错误串>）`
- 真非 JSON → `models.dev 返回的不是 JSON：<响应体开头，单行 1000 字截断>`

「不是 JSON」文案只在响应体不是 JSON 时出现，与超时彻底分开。

### 3. 网络层做测试 seam

`fetch_modelsdev(client: &reqwest::blocking::Client, url: &str)` 是 `pub(crate)`，测试构造一个真实 `reqwest::blocking::Client`（用更短超时模拟慢响应）+ `TcpListener` 上的 fake server。回归测试在 `tests/providers_catalog.rs`：

- `fetch_modelsdev_reports_non_json_body_head` — fake server 返回 `Content-Type: text/html` 的 HTML 错误页，断言错误串包含「返回的不是 JSON」+ 响应体内容 + 单行化。
- `fetch_modelsdev_reports_timeout_when_exceeds_client_timeout` — fake server 只 accept 不写响应，client 用 100ms 超时，断言错误串包含「请求超时」且不包含「不是 JSON」。
- `fetch_modelsdev_returns_parsed_value_for_valid_json` — fake server 返回合法 JSON，断言解析结果字段。

`fetch_modelsdev` 接受 URL 参数，测试不需要把 `https://models.dev/api.json` 替换成测试 server。

## 验收

- `cargo test` 495 项全过（含 3 条新增）。
- 文案修复后用户能区分「上游服务器慢（重试）」与「上游服务器挂了 / 接口改了（升级）」两类失败。

## 后续观察

- models.dev 目录当前 ~30s 完整下载；如再涨到 >45s，需要再调高 `MODELSDEV_TIMEOUT`，或考虑分页 / 流式解析。届时响应体的 `cf-cache-status` 也可能从 HIT 变 MISS，第一次冷拉取会更慢。
- 这条修复只覆盖 models.dev，聚合平台 `fetch_provider_models_inner` 仍用 `bytes()` + `from_slice` 的两阶段解析，错误分支已能区分 JSON 解析失败与网络/超时，不在本次范围。