# Claude Science 0.1.25 `A-PLUGIN-01` 静态/package 调查

状态：已执行；`FAIL(reason=package-hash-mismatch-and-immediate-stop-not-met)`

适用范围：Claude Science 0.1.25 固定 DMG package identity 与 CSSwitch exact source
HEAD `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd` 的 root-Skill / Plugin-candidate
importer 分支；不覆盖原生 Plugin runtime lifecycle。

最后复核：2026-07-30

## 判定

| Sub-gate | 目标层 / scope | 结果 | 观察 |
|---|---|---|---|
| package identity | `PACKAGE-STATIC(scope=Claude Science 0.1.25 DMG)` | `FAIL` | 冻结 hash 应为 `cdc064…d6ce`；`/private/tmp/claude-science-latest-20260728.dmg` 实际为 `29e9c6…ffa1c` |
| stop contract | G0 case control | `FAIL` | mismatch 后仍尝试 stat / `hdiutil imageinfo`（失败、未挂载）、搜索 `/private/tmp` DMG hash 并继续 source importer 检查，没有立即停止 |
| package surface | `PACKAGE-STATIC(scope=manifest/UI/hooks/MCP/agents/permission/enable-disable/update)` | `INCONCLUSIVE` | 未挂载、未解包、未执行；各格保持 `unknown` |
| CSSwitch importer | `SOURCE-CONTRACT(scope=exact HEAD local archive importer)` | `INCONCLUSIVE` | 详细矩阵在 required stop 之后完成，只保留为 post-stop observation，不作为本 run 的 sub-gate PASS |

整体判定为 `FAIL`。hash mismatch 本身命中 probe 的 `FAIL` 条件；同时本 case
没有遵守全局立即停止合同。post-stop matrix 观察到 CSSwitch source 会从部分
Plugin layout 中提取 Skill 集合，并拒绝 hooks、`mcpServers`、agents 与
`${CLAUDE_PLUGIN_ROOT}` runtime 依赖，但这些观察不能授予本 run 的
`SOURCE-CONTRACT` PASS，也不是 Science 原生 Plugin 生命周期支持。

## 身份、授权与证据

- 仓库：`/private/tmp/csswitch-main-governance-v084-20260729`
- branch：`codex/main-baseline-capability-map-v084`
- HEAD：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`
- 开始时：51 条 dirty / untracked paths，0 staged；本 probe 未覆盖或清理它们。
- CSSwitch artifact / hash / signing / build：不适用；CSSwitch 侧只读 source。
- Science package 期望：0.1.25 build `b7190511`，DMG SHA-256
  `cdc0642061983c80e371cbb529035ac3dd8d341a4a8dfd04c8de3085e12bd6ce`。
- 唯一 Science-named candidate：
  `/private/tmp/claude-science-latest-20260728.dmg`，86,880,188 bytes，SHA-256
  `29e9c6ebc737eb268d7c815b2865aa615449f48d6eadadca983c6c5cc82ffa1c`；
  未接受、未挂载、未执行；hash mismatch 后还发生一次失败的静态 `imageinfo`
  尝试与只读 candidate search，已作为 stop-contract failure 记录。
- Science binary / hash：未进入；没有读取或启动 package executable。
- 授权：只允许本地固定 package 与 exact source 的 A 类静态检查；禁止网络、
  package 启动、真实 Plugin 导入、用户 marketplace / Skill、账号、provider 和
  B/C probe。
- evidence：
  `/private/tmp/csswitch-science-probe-evidence/20260730T043506Z-a-plugin-01/A-PLUGIN-01/`
- `hashes.sha256` SHA-256：
  `abe97ffc3064835abde748dbd3417a1aeb2107b8166245d2925bfe6584617a29`
- 清理：0 process、0 port、未挂载 package、未创建 runtime root；candidate 未改，
  evidence root 保留，无运行残留；`immediate_stop_met=false`。

## 九层状态

| 层 | 本次状态 |
|---|---|
| `EXTERNAL-OFFICIAL` | `NOT-RUN` |
| `SOURCE-CONTRACT` | `INCONCLUSIVE(reason=post-stop-observation-not-admissible,scope=exact HEAD CSSwitch importer paths)` |
| `SOURCE-TEST` | `NOT-RUN` |
| `PACKAGE-STATIC` | `FAIL(reason=package-hash-mismatch,scope=Claude Science 0.1.25 Plugin surface)` |
| `HISTORICAL-ISOLATED-LIVE` | `NOT-RUN` |
| `FINAL-ARTIFACT` | `NOT-RUN` |
| `CURRENT-INSTALLED-STATIC` | `NOT-RUN` |
| `CURRENT-INSTALLED-LIVE` | `NOT-RUN(target=CURRENT-INSTALLED-LIVE,scope=Plugin lifecycle)` |
| `PUBLIC-RELEASE` | `NOT-RUN` |

## 未闭合项

- 需要已有且 SHA-256 精确匹配冻结值的 0.1.25 package 副本，并从新 run 严格
  遵守 mismatch 立即停止，才能重跑；本窗口不得联网补取。
- Science Plugin manifest、UI、permission、enable/disable 与 update 仍为 `unknown`。
- `B-PLUGIN-01` 不得开始；其前置 A 没有整体 `PASS`。
- B/C probe 全部保持 `NOT-RUN`。
