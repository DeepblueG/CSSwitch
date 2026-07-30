# Claude Science 0.1.25 `A-SSH-01` 静态调查

状态：已执行；`PASS`

适用范围：CSSwitch exact source HEAD
`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd` 的 repository wrapper、
Tauri resource mapping、packaged-path validator、launch env 与 source tests；
不覆盖用户 SSH 状态、Science parser、实际 OpenSSH invocation 或 server。

最后复核：2026-07-30

## 判定

| Sub-gate | 目标层 / scope | 结果 | 观察 |
|---|---|---|---|
| wrapper expectation | `SOURCE-CONTRACT(scope=repository wrapper)` | `PASS` | source 为 232 bytes，SHA-256 `0828ac…c5c`；后续 artifact 期望固定 exact bytes/hash、owner relation、`0755`、nlink 1、非 group/world writable |
| package mapping | `SOURCE-CONTRACT(scope=Tauri resource destination)` | `PASS` | `../../scripts/ssh-bridge/ssh` 映射为 `scripts/ssh-bridge/ssh`；source mapping 不独立 pin hash/owner/mode/nlink |
| validator | `SOURCE-CONTRACT(scope=packaged wrapper acceptance)` | `PASS` | 实际只检查目录链 non-symlink directory、wrapper non-symlink regular file、≤128 KiB、至少一个 executable bit；content/hash/owner/nlink/group-world writable/exact mode 明确未检查 |
| runtime invocation source | `SOURCE-CONTRACT(scope=launch and wrapper source)` | `PASS` | launch 把 wrapper dir 放到 PATH 并设置绝对 config env；wrapper 最终 `exec /usr/bin/ssh -F "$config" "$@"` |
| source tests | `SOURCE-TEST` | `NOT-RUN` | 只记录 test pointers，未从测试存在推出 PASS |

整体判定为 `PASS`：wrapper source 与调用约定不冲突，且 validator 比完整 identity
期望更宽的范围已在 rule、feature 与本 evidence 中显式标出，没有被误写成已校验。

## 身份、授权与证据

- 仓库：`/private/tmp/csswitch-main-governance-v084-20260729`
- branch：`codex/main-baseline-capability-map-v084`
- HEAD：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`
- 开始时：51 条 dirty / untracked paths，0 staged；本 probe 未覆盖或清理它们。
- CSSwitch artifact / hash / signing / build：不适用；未读取 built artifact。
- Science binary / package / hash：不适用；未访问。
- wrapper source：`scripts/ssh-bridge/ssh`，232 bytes，SHA-256
  `0828acbda9f296983c127149879526e92a3eb915cbc9874747e8cecbeab87c5c`。
- 授权：仅 repository SSH bridge 的 A 类静态审查；禁止真实 `~/.ssh`、key、
  agent、known_hosts、`ssh` 执行、host/network/server、账号和 B/C probe。
- evidence：
  `/private/tmp/csswitch-science-probe-evidence/20260730T043506Z-a-ssh-01/A-SSH-01/`
- `hashes.sha256` SHA-256：
  `8e8e62d7f1f5ad4d066ab5c8bb4568a7d46ebb4dc4db7a54cecd77aea603ba17`
- 清理：0 process、0 port、未创建 runtime root、未访问用户 SSH 状态；evidence
  root 保留，无遗留项。

## 九层状态

| 层 | 本次状态 |
|---|---|
| `EXTERNAL-OFFICIAL` | `NOT-RUN` |
| `SOURCE-CONTRACT` | `PASS(scope=exact HEAD wrapper expectation, mapping, validator and invocation source)` |
| `SOURCE-TEST` | `NOT-RUN(status=not-run)` |
| `PACKAGE-STATIC` | `NOT-RUN` |
| `HISTORICAL-ISOLATED-LIVE` | `NOT-RUN` |
| `FINAL-ARTIFACT` | `NOT-RUN` |
| `CURRENT-INSTALLED-STATIC` | `NOT-RUN` |
| `CURRENT-INSTALLED-LIVE` | `NOT-RUN(target=CURRENT-INSTALLED-LIVE,scope=SSH parser/invocation/server)` |
| `PUBLIC-RELEASE` | `NOT-RUN` |

## 未闭合项

- 后续固定 final artifact 必须核对 wrapper exact hash、owner relation、`0755`、
  nlink 1 与非 group/world writable；当前 validator 不做这些检查。
- `B-SSH-01` Science parser acceptance、`B-SSH-02` recorder invocation 与
  `C-SSH-01` real server connectivity 均未运行，不能互相外推。
- source tests 未运行；B/C probe 全部保持 `NOT-RUN`。
