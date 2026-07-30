# 文档治理规则

- 创建、修改、引用或删除文档前，先从 [`docs/README.md`](../../docs/README.md) 判断权威类型；生命周期与元数据以[文档治理合同](../../docs/operations/document-lifecycle.md)为准。
- 一个事实只能有一份当前权威正文。索引只说明“这份文档回答什么问题”，兼容路径只做指针。
- 已接受的 spec 必须把耐久结论提炼进 architecture、features、operations 或 rules；提炼完成后删除 spec，不并存两份权威正文。
- Plan、Draft Spec 和 Handoff 必须写明失效或删除条件；任务结束、基线失效或正文已提炼后及时删除。Git 保存历史，不建立 `archive/`、`old-docs/` 或类似坟场。
- 默认首轮阅读预算是：`AGENTS.md`、安全/Git 规则、实时 status，再加零或一项领域规则、一个索引和最多两份相关正文或 context。正式独立审查额外读取 `reviewing.md` overlay，但不增加其余预算。只有当前问题需要时才逐级展开。
- 普通任务不得全量读取 `.agents/context/`、`docs/audits/`、`docs/evidence/` 或所有架构文档；审计、事故、发布和明确的证据核验任务除外，但仍按范围读取。
- 架构文档按稳定问题、所有权或维护原因拆分，由索引路由；没有当前正文时不创建空文件或空目录。
- 删除、移动或改写未提交文档前，仍按用户数据保护，并遵守 Git / worktree 规则。
