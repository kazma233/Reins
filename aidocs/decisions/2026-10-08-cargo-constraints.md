# Cargo 依赖与包配置约束

- 日期：2026-10-08
- 状态：生效中

## 背景

这些取舍原先以注释形式放在 Cargo.toml 内，版本升级时容易过期；Cargo.toml 恢复为纯声明，取舍和约束统一在这里维护。

## serde_yaml（实际包名 yaml_serde）

- 上游 `serde_yaml` 0.9.34 已废弃，实际依赖是 YAML 官方组织维护的 `yaml_serde` 0.10.x，通过 Cargo 包重命名引用，Rust 代码里的 `use serde_yaml::` 保持不变。
- `yaml_serde` 是 serde_yaml 的兼容延续，`!!` 双叹号标签被解析成普通标量等既有行为不变，dsh patch 的哨兵方案仍然必要（背景见 `context/2026-10-07-dsh-session-storage-exploration.md`）。

## ruzstd

- dsh 转录是逐批追加的独立 zstd 帧拼接；`ruzstd` 是纯 Rust 解码器，避免引入 C 工具链。

## reqwest

- 供 providers 域拉取模型目录。
- 只启用 rustls，避免 Windows 上引入 openssl。
- `system-proxy` 跟随系统代理，但不读 macOS 例外列表，局域网地址也会经代理，属已知限制。

## dunce（仅 Windows）

- Windows 上 `std::fs::canonicalize` 返回 `\\?\` verbatim 路径；统一走 `dunce` 取普通拼写。

## default-run

- 测试专用的假 CLI bin（`src/bin/reins-fake-cli.rs`）让包出现第二个 binary，`cargo run` 需要显式默认目标 `reins`。
