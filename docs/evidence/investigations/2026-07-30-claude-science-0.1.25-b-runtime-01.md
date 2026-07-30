# Claude Science 0.1.25 `B-RUNTIME-01` 隔离 runtime identity 与生命周期调查

状态：已执行；`INCONCLUSIVE(reason=artifact-or-binary-identity)`

适用范围：2026-07-30 指定 v0.8.4 public DMG 本地副本、Claude Science 0.1.25
本地 identity gate 与 G1 隔离 lifecycle；本次未到达产品 runtime。

最后复核：2026-07-30

## 判定

| Sub-gate | 目标层 / scope | 结果 | 观察 |
|---|---|---|---|
| 基线与前置 | `SOURCE-CONTRACT` 输入 | `PASS` | 指定 root / branch / HEAD 与 handoff 一致；开始时 56 条 dirty / untracked、0 staged；`A-EVIDENCE-01=PASS` 且 hashes 回验全 `OK` |
| CSSwitch container identity | `FINAL-ARTIFACT` 前置 | `PASS`（仅容器字节） | 本地 `CSSwitch_0.8.4_aarch64.dmg` 为 12,884,340 bytes，SHA-256 `23471d…f2b2`，与冻结的公开 Release metadata 一致 |
| CSSwitch executable / signing / build identity | `FINAL-ARTIFACT` | `INCONCLUSIVE` | Science identity 已先阻断；本次没有 mount / 解包 DMG，未固定 bundle 内 executable path/hash、签名和 build identity |
| Science binary identity | runtime 前置 | `INCONCLUSIVE` | `/private/tmp` 内 37 个名为 `claude-science` 的文件没有一个匹配冻结的 0.1.25 standalone 或 App-seeded hash；唯一现成 Science DMG 是已冻结的 hash mismatch |
| start / open / reopen / status / stop / restart | `CURRENT-INSTALLED-LIVE(scope=isolated-local-mock)` | `NOT-RUN` | identity gate 前置不满足；没有启动 executable、mock、daemon、listener 或生命周期动作 |
| 清理 | probe-owned runtime root | `PASS` | 0 process、0 port；仅创建的 5 个空目录与 exact runtime root 已删除，0 residual |

整体判定为
`INCONCLUSIVE(reason=artifact-or-binary-identity)`。本次没有到达有效 fixture
下的产品行为，不能写成产品 `FAIL`；也不能把 public DMG 容器字节匹配外推为
bundle executable、installed 或 live identity。

## 身份、授权与证据

- 仓库：`/private/tmp/csswitch-main-governance-v084-20260729`
- branch：`codex/main-baseline-capability-map-v084`
- HEAD：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`
- 开始时：56 条 dirty / untracked paths，0 staged；本 probe 未覆盖或清理它们。
- CSSwitch container：
  `/private/tmp/csswitch-v084-public.bKlYq0/CSSwitch_0.8.4_aarch64.dmg`；
  12,884,340 bytes；SHA-256
  `23471daf2caa7832da3205bcc8ba97d96c057ad17539c6c7ed36aa43a3c5f2b2`。
- CSSwitch bundle executable / signing / build identity：未固定。
- Claude Science 0.1.25 build：`b7190511`；允许的本地 executable identity：
  standalone arm64
  `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7`
  或 App-seeded arm64
  `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`。
- 本机允许范围内匹配数：0。已知
  `/private/tmp/claude-science-latest-20260728.dmg` 实际 SHA-256
  `29e9c6…ffa1c`，不等于冻结的 0.1.25 DMG SHA-256
  `cdc064…d6ce`，未使用。
- OS：macOS 26.5.2（25F84），Darwin 25.5.0，arm64。
- runtime root：
  `/private/tmp/csswitch-science-probe-runtime/20260730T050532Z-b-runtime-01/`；
  外层 HOME、data-dir、CSSwitch state、mock state、logs 只创建为空目录后精确删除。
- 动态端口：未分配；loopback deterministic inference mock：未启动；deadline
  未进入 lifecycle 阶段。
- 授权：仅 `B-RUNTIME-01` 的 G1 隔离动作；禁止其他 B/C、账号、provider、
  SSH、网络、`/Applications`、真实 HOME、Keychain、8765、build 与 Git 写。
- evidence：
  `/private/tmp/csswitch-science-probe-evidence/20260730T050532Z-b-runtime-01/B-RUNTIME-01/`
- `hashes.sha256` SHA-256：
  `c58ee15679a8f300020c2a0bf111aee1b7bac4dd7e733b80f58555d7a7334067`。

## 生命周期与清理

| 动作 | 状态 | PID / executable / port / data-dir 观察 |
|---|---|---|
| `start` | `NOT-RUN` | 未创建 PID；未固定 executable；未分配 port |
| `open` | `NOT-RUN` | 无 status response |
| `reopen` | `NOT-RUN` | daemon peak count 为 0，不能验证复用 |
| `status` | `NOT-RUN` | 无 response |
| `stop` | `NOT-RUN` | 无归属进程或端口需要释放 |
| `restart` | `NOT-RUN` | 不能验证 binary / data-dir 保持 |

`inventory-before.json` 与 `inventory-after.json` 只记录 probe-owned 对象。没有观察
或接管现有 daemon，没有访问 8765。清理后 runtime root 不存在；evidence root
按规格保留。

## 九层状态

| 层 | 本次状态 |
|---|---|
| `EXTERNAL-OFFICIAL` | `NOT-RUN` |
| `SOURCE-CONTRACT` | `PASS(scope=A-EVIDENCE-01 prerequisite only)` |
| `SOURCE-TEST` | `NOT-RUN` |
| `PACKAGE-STATIC` | `NOT-RUN` |
| `HISTORICAL-ISOLATED-LIVE` | `NOT-RUN` |
| `FINAL-ARTIFACT` | `INCONCLUSIVE(reason=artifact-or-binary-identity,scope=public DMG container matched; internal executable/signing/build not fixed)` |
| `CURRENT-INSTALLED-STATIC` | `NOT-RUN` |
| `CURRENT-INSTALLED-LIVE` | `NOT-RUN(target=CURRENT-INSTALLED-LIVE,scope=isolated-local-mock)` |
| `PUBLIC-RELEASE` | `NOT-RUN`；只复用冻结 metadata 做本地容器 byte match，不重验当前远端 |

## 未闭合项

- 需要一个当前本机可读、来源可记录且 SHA-256 精确匹配上述两种允许 identity
  之一的 Claude Science 0.1.25 executable；不能从错误 DMG、模拟 binary、
  `/Applications` 或真实 HOME 代替。
- 两项 identity 前置同时满足后，才可在新 run 中 mount exact v0.8.4 DMG，
  固定 bundle executable / signing / build identity，并执行动态端口、local mock 与
  start / open / reopen / status / stop / restart。
- `B-RUNTIME-01` 未 PASS，因此其他依赖它的 B probe 和全部 C probe均不得进入；
  `CURRENT-INSTALLED-LIVE(scope=isolated-local-mock)` 继续为 `NOT-RUN`。

## 2026-07-30 identity gate 重试

UTC `2026-07-30T05:19:10Z` 在同一允许范围内只重验
`/private/tmp` 中当前可读、文件名为 `claude-science` 的 regular file：

- 候选仍为 37 个；
- standalone
  `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7`
  与 App-seeded
  `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`
  的精确匹配数仍为 0；
- 没有扩大到不可读临时 mount、`/Applications`、真实 HOME、网络或其他来源，
  也没有使用已冻结的错误 DMG 或模拟 binary。

因此本次没有创建新 run、runtime root 或 evidence root，没有 mount / 解包
CSSwitch DMG，也没有执行 mock、端口、process 或 lifecycle 动作。上一 run
`20260730T050532Z-b-runtime-01` 的整体判定继续保持
`INCONCLUSIVE(reason=artifact-or-binary-identity)`；`FINAL-ARTIFACT` 内部 identity
仍未固定，`CURRENT-INSTALLED-LIVE(scope=isolated-local-mock)` 仍为 `NOT-RUN`。

## 2026-07-30 identity gate 第二次重试

UTC `2026-07-30T05:26:33Z` 在同一允许范围内再次只重验
`/private/tmp` 中当前可读、文件名为 `claude-science` 的 regular file：

- 候选仍为 37 个；
- standalone
  `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7`
  与 App-seeded
  `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`
  的精确匹配数仍为 0；
- 开始时实时 root / branch / HEAD 与冻结基线一致，完整
  `git status --short` 为 57 条 unstaged changed / untracked、0 staged；
- 没有扩大到不可读临时 mount、`/Applications`、真实 HOME、网络或其他来源，
  也没有使用已冻结的错误 DMG 或模拟 binary。

因此本次仍没有创建新 run、runtime root 或 evidence root，没有 mount / 解包
CSSwitch DMG，也没有执行 mock、端口、process 或 lifecycle 动作。上一 run
`20260730T050532Z-b-runtime-01` 继续保持
`INCONCLUSIVE(reason=artifact-or-binary-identity)`；`FINAL-ARTIFACT` 内部 identity
仍未固定，`CURRENT-INSTALLED-LIVE(scope=isolated-local-mock)` 仍为 `NOT-RUN`。

## 2026-07-30 identity gate 第三次重试

UTC `2026-07-30T05:35:26Z` 在同一允许范围内再次只重验
`/private/tmp` 中当前可读、文件名为 `claude-science` 的 regular file：

- 候选仍为 37 个；
- standalone
  `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7`
  与 App-seeded
  `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`
  的精确匹配数仍为 0；
- 开始时实时 root / branch / HEAD 与冻结基线一致，完整
  `git status --short` 为 57 条 unstaged changed / untracked、0 staged；
- 没有扩大到不可读临时 mount、`/Applications`、真实 HOME、网络或其他来源，
  也没有使用已冻结的错误 DMG 或模拟 binary。

因此本次仍没有创建新 run、runtime root 或 evidence root，没有 mount / 解包
CSSwitch DMG，也没有执行 mock、端口、process 或 lifecycle 动作。上一 run
`20260730T050532Z-b-runtime-01` 继续保持
`INCONCLUSIVE(reason=artifact-or-binary-identity)`；`FINAL-ARTIFACT` 内部 identity
仍未固定，`CURRENT-INSTALLED-LIVE(scope=isolated-local-mock)` 仍为 `NOT-RUN`。
