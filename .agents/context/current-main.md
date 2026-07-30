# 当前主干定位

最后复核：2026-07-29（Asia/Shanghai）

失效条件：`origin/main`、本地 `main` 或 peeled `v0.8.4` 不再同时指向 `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`，或当前维护基线改变时立即失效并更新。

- 当前维护基线：`v0.8.4`。
- 实时核验时 `origin/main`、本地 `main` 与 peeled `v0.8.4` 均指向 `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`。
- `v0.8.4` 相对 `v0.8.3` 只包含 Science updater identity hotfix 的 3 个 commit；source-fixed 与产品 gate 未闭合的边界见完整审计。
- Git、公开 Release、源码、测试、artifact、installed/live 与签名真相域的完整边界见 [v0.8.4 真实主干基线](../../docs/audits/2026-07-29-v084-main-baseline.md)。
- worktree、远端引用和公开 Release 会变化；使用本页前仍须实时复核。不要把本页当测试通过或 installed runtime 证据。
