# Claude Science 0.1.25 `A-EVIDENCE-01` 用语门禁调查

状态：已执行；`PASS`

适用范围：2026-07-30 九层证据词表、probe schema 与合成正反例；只证明
`SOURCE-CONTRACT` 的 evidence 用语约束，不产生任何产品能力证据。

最后复核：2026-07-30

## 判定

| Sub-gate | 目标层 / scope | 结果 | 观察 |
|---|---|---|---|
| 九层 schema | `SOURCE-CONTRACT(scope=evidence vocabulary)` | `PASS` | 只允许 9 个层；`NOT-RUN` 是带 target 的缺证状态，`RELEASE-METADATA` 是 `PUBLIC-RELEASE` 未闭合子状态 |
| 正例 | `SOURCE-CONTRACT(scope=synthetic evidence)` | `PASS` | 5 / 5 保留 exact layer、scope、artifact identity；合法 `NOT-RUN(target=...)` 被接受 |
| 越层反例 | `SOURCE-CONTRACT(scope=synthetic cross-layer claims)` | `PASS` | 6 / 6 被拒绝，包括 source→live、package→installed、historical→current、metadata→public、`NOT-RUN` 当层和缺 scope |

整体判定为 `PASS`。合成校验没有读取历史 PASS 来填充当前字段，也没有把
`RELEASE-METADATA` 升级为 `PUBLIC-RELEASE`。

## 身份、授权与证据

- 仓库：`/private/tmp/csswitch-main-governance-v084-20260729`
- branch：`codex/main-baseline-capability-map-v084`
- HEAD：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`
- 开始时：51 条 dirty / untracked paths，0 staged；本 probe 未覆盖或清理它们。
- CSSwitch artifact / hash / signing / build：不适用；只读 evidence contract。
- Science binary / package / hash：不适用；未访问。
- 词表版本：2026-07-30；入口：
  `docs/audits/2026-07-30-v084-architecture-reconnaissance.md#2-证据词表`。
- 授权：仅文档与合成 evidence 的 A 类静态审查；禁止 runtime、账号、网络、
  历史 PASS 自动填充和 B/C probe。
- evidence：
  `/private/tmp/csswitch-science-probe-evidence/20260730T043506Z-a-evidence-01/A-EVIDENCE-01/`
- `hashes.sha256` SHA-256：
  `8e3d70e3114ceb1643bb70abe7bc8c7aee42d0890fa970ee8409b396772b9e7d`
- 清理：0 process、0 port、未创建 runtime root；evidence root 保留，无遗留项。

## 九层状态

| 层 | 本次状态 |
|---|---|
| `EXTERNAL-OFFICIAL` | `NOT-RUN` |
| `SOURCE-CONTRACT` | `PASS(scope=nine-layer evidence wording contract)` |
| `SOURCE-TEST` | `NOT-RUN` |
| `PACKAGE-STATIC` | `NOT-RUN` |
| `HISTORICAL-ISOLATED-LIVE` | `NOT-RUN` |
| `FINAL-ARTIFACT` | `NOT-RUN` |
| `CURRENT-INSTALLED-STATIC` | `NOT-RUN` |
| `CURRENT-INSTALLED-LIVE` | `NOT-RUN(target=CURRENT-INSTALLED-LIVE,scope=all product capabilities)` |
| `PUBLIC-RELEASE` | `NOT-RUN` |

## 未闭合项

- 本 probe 只证明用语/schema 合同；任何后续 capability 仍需取得自己的 actual
  evidence。
- `RELEASE-METADATA` 继续是未闭合子状态；没有本轮 public attachment 证据。
- B/C probe 全部保持 `NOT-RUN`。
