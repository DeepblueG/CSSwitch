# Claude Science 0.1.25 `A-IPC-01` 静态调查

状态：已执行；`PASS`

适用范围：CSSwitch exact source HEAD
`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd` 的 Tauri command 定义、编译模块、
production handler 注册、生产 frontend caller、条件 auto-boot caller 与
preview mock；不覆盖 build、artifact、installed 或 live IPC。

最后复核：2026-07-30

## 判定

| Sub-gate | 目标层 / scope | 结果 | 观察 |
|---|---|---|---|
| 注册集合 | `SOURCE-CONTRACT(scope=exact HEAD production handler)` | `PASS` | 43 个唯一 registered commands 均有 definition、compiled module 与 registration 路径行号 |
| caller 三态 | `SOURCE-CONTRACT(scope=production frontend / auto-boot)` | `PASS` | 38 个有生产 frontend 或 auto-boot 可达输入；5 个为 `no-caller` |
| dormant 输入 | `SOURCE-CONTRACT(scope=5 dormant IPC)` | `PASS` | `list_templates`、`validate_profile_catalog_model`、`preview_profile_preset_sync`、`apply_profile_preset_sync`、`start_proxy` 均记录 keep/remove/expose 输入；本 probe 不代替产品决策 |
| preview/mock 分离 | `SOURCE-CONTRACT(scope=browser preview only)` | `PASS` | preview case 单列，未计作生产 caller |

整体判定为 `PASS`。清单没有漏项或重复归属；静态解析也未遇到无法唯一归类的宏或
条件编译。`one_click_login` 同时有生产 frontend caller 与条件 auto-boot inner
caller，未把 auto-boot 误记成第二个 IPC。

## 身份、授权与证据

- 仓库：`/private/tmp/csswitch-main-governance-v084-20260729`
- branch：`codex/main-baseline-capability-map-v084`
- HEAD：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`
- 开始时：51 条 dirty / untracked paths，0 staged；本 probe 未覆盖或清理它们。
- CSSwitch artifact / hash / signing / build：不适用；本 probe 只读 source。
- Science binary / package / hash：不适用；未访问。
- 授权：仅本次 exact HEAD 的 A 类静态审查；无账号、网络、凭证、App、runtime、
  SSH、provider 或 B/C 授权。
- evidence：
  `/private/tmp/csswitch-science-probe-evidence/20260730T043506Z-a-ipc-01/A-IPC-01/`
- `hashes.sha256` SHA-256：
  `935a18a5bd45e3cd40d31356377aac486035112001cbd289f4193d7aee28bc6f`
- 清理：0 process、0 port、未创建 runtime root；evidence root 按规格保留，无遗留项。

## 九层状态

| 层 | 本次状态 |
|---|---|
| `EXTERNAL-OFFICIAL` | `NOT-RUN` |
| `SOURCE-CONTRACT` | `PASS(scope=exact HEAD Tauri registration and caller reachability)` |
| `SOURCE-TEST` | `NOT-RUN` |
| `PACKAGE-STATIC` | `NOT-RUN` |
| `HISTORICAL-ISOLATED-LIVE` | `NOT-RUN` |
| `FINAL-ARTIFACT` | `NOT-RUN` |
| `CURRENT-INSTALLED-STATIC` | `NOT-RUN` |
| `CURRENT-INSTALLED-LIVE` | `NOT-RUN(target=CURRENT-INSTALLED-LIVE,scope=IPC behavior)` |
| `PUBLIC-RELEASE` | `NOT-RUN` |

## 未闭合项

- 5 个 dormant IPC 的 keep/remove/expose 是后续产品决策，不是静态 probe 自动结论。
- build、compiled artifact 与 live invoke 均未运行，不能从本 `PASS` 外推。
- B/C probe 全部保持 `NOT-RUN`。
