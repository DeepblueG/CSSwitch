# 当前主干定位

最后复核：2026-07-30（Asia/Shanghai）

失效条件：peeled `v0.8.4`、v0.8.4 release source 或当前维护版本改变时立即失效；
本地/远端 `main` 的普通后继提交必须实时查询，不再要求永远等于 release tag。

- 当前维护基线：`v0.8.4`。
- v0.8.4 peeled tag / release source：
  `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`。
- 2026-07-30 治理合并前的实时核验中，`origin/main` 与本地 `main` 仍指向该
  release source；治理文档合并后，本地或远端 `main` 可以成为它的 docs/test
  后继，而不改变 v0.8.4 发布身份。
- `v0.8.4` 相对 `v0.8.3` 只包含 Science updater identity hotfix 的 3 个 commit；source-fixed 与产品 gate 未闭合的边界见完整审计。
- Git、公开 Release、源码、测试、artifact、installed/live 与签名真相域的完整边界见 [v0.8.4 真实主干基线](../../docs/audits/2026-07-29-v084-main-baseline.md)。
- v0.8.4 的后续 source gate、artifact、installed identity、signing 与公开回读见
  [release evidence](../../docs/evidence/releases/v0.8.4.md)；它补充而不改写上述
  2026-07-29 治理起点审计。
- worktree、`main`、远端引用和公开 Release 会变化；使用本页前仍须实时复核。
  不要把主干 ancestry 当测试通过或 installed runtime 证据。
