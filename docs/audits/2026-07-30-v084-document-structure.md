# CSSwitch v0.8.4 文档结构与拆分审计

审计日期：2026-07-30（Asia/Shanghai）

状态：第三期 A 文档治理审计；不代表产品测试、artifact、installed/live、签名、公证或公开发布 PASS。

适用范围：`/private/tmp/csswitch-main-governance-v084-20260729`、branch `codex/main-baseline-capability-map-v084`、基线 HEAD `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`，以及本任务开始时受保护的全部未提交 Markdown。

失效条件：worktree、branch、HEAD、受保护 dirty 基线、文档目录结构或本页引用的当前正文发生变化时，必须重新审计受影响范围。

## 1. 输入与边界

任务开始时共有 101 份 tracked + untracked Markdown；ignored handoff `.agents/handoffs/2026-07-30-phase2-to-phase3a.md` 另计，审计输入合计 102 份。本期新增独立审查规则与本审计页；两个已到删除条件的 tracked 兼容指针随后删除，最终链接检查仍覆盖 102 份 Markdown。

本轮只判断文档的稳定问题、维护原因、失效条件、权威归属和索引路由。没有读取真实凭证、Keychain、真实 `~/.claude-science`，没有运行 Science、账号、provider 或 SSH server，没有修改产品代码、quality JSON/schema/test，也没有执行 commit、push、tag 或 release。

审计使用[文档治理合同](../operations/document-lifecycle.md)的四种结论：

- `keep`：同一个稳定问题、维护触发器和失效条件；
- `shrink`：删除重复权威，只留问题路由或版本化入口；
- `split`：至少两个独立稳定问题，且维护原因或失效条件不同；
- `delete-pointer`：兼容指针的可判定删除条件已经满足。

行数不参与单独判定；Audit / Evidence 只要仍绑定同一日期、版本、环境或 artifact，就不因长而拆。

## 2. 全库 Markdown 压力结论

| 输入组 | 覆盖范围 | 结论 | 理由 |
|---|---|---|---|
| Agent 入口与规则 | `AGENTS.md`、`CLAUDE.md`、`.agents/rules/*.md` | `keep`；路由收口 | `AGENTS.md` 仍是总入口；rules 按安全、Git、文档、测试、发布、Science、Skill、SSH 与独立审查的稳定行为问题分开。任务路由只存在于 rules 索引，不建立第二套 Agent 手册。 |
| Context | `.agents/context/*.md` | `keep` + `shrink` + `delete-pointer` | `current-main`、known issues 各回答独立的可漂移问题；过期的 current-release / verified-state 缩成警示与 dated audit/evidence 入口。worktrees 与旧 document-audit 只形成无信息回路，移除入站链接后删除。 |
| Handoff | `.agents/handoffs/README.md` 与 ignored Phase 2 handoff | README `keep`；旧 handoff `expired-protected` | README 只路由生命周期。Phase 2 handoff 固定的 25 条 status 已因本期候选变化而失效；本轮不覆盖或删除该用户数据，也不再把它当当前 checkpoint，任务完成后由精简 successor handoff 接续。 |
| 产品入口 | 根 `README.md`、`README.en.md`、`CHANGELOG.md` | `keep` | 两种语言是不同读者入口但各自完整；CHANGELOG 是单一版本时间线。它们不是架构或证据正文，不能因长度迁移。 |
| 文档索引 | `docs/README.md` 与 architecture / feature / operation / evidence / reference README | `keep` + `shrink` | 总入口与分目录索引只回答去哪里解决什么问题；移除状态结论和重复 owner / evidence 摘要。 |
| Architecture | `docs/architecture/README.md` 与 6 份正文 | `keep` + `shrink`；`split=0` | 第二期已经按总览、Desktop 控制面、状态事务、Gateway/provider 路由、Science runtime、Science 能力依赖拆成不同稳定问题。可达性词表只在 Desktop 控制面定义，总览缩成提醒与链接。 |
| Feature | `docs/features/README.md`、6 份当前正文与 1 个兼容指针 | `keep` + `shrink` | 6 份正文分别维护用户合同；UI 页改为 v0.8.0 起的当前合同；Codex bridge 删除过期七阶段 Plan 和残留阶段/RM gate 语境，并把重复 RM 矩阵缩成 operation 链接。`product-science-capability-map.md` 只保留用户可见 support、限制与 non-target；owner、内部机制、版本身份、证据定义和探针仍分别归 architecture 与 dated audit。Codex 旧 implementation-plan 只是兼容指针。 |
| Operation | `docs/operations/README.md` 与 8 份正文 | `keep`；路由收口 | 开发、测试、quality kernel、frozen source-gate spec、真机验收、发布、升级回滚与文档生命周期有不同执行触发器。真机验收明确为跨版本累积的当前运维合同，每次执行另绑 exact candidate；索引只回答各页解决的问题。 |
| Audits | `docs/audits/*.md` | `keep` | 每页绑定特定版本、日期与调查范围；后续结果不能改写旧候选当时的结论，也不因长度拆。 |
| Evidence | `docs/evidence/**/*.md` | `keep` | investigation 与 release 页分别绑定日期、环境、tag 或 artifact；总页与二级 README 只路由。长时间线和证据链不是正文拆分理由。 |
| External reference | `docs/references/**/*.md` | `keep` | CSNative 当前正文绑定 reviewed commit；旧大写路径只做兼容指针。外部参考不升级成 CSSwitch 当前事实。 |
| Packaged Markdown | `desktop/src-tauri/resources/skills/csswitch-external-skill-tools/*.md`、模型图标 `LICENSES.md` | `keep` | `SKILL.md` 是打包资源；4 个 legacy 变体由 Rust 测试 `include_str!` 绑定为兼容 fixture，不是重复的当前维护正文；许可证清单有独立法律用途。 |
| 旧路径 | docs 根迁移页、Codex 旧 implementation-plan、`docs/references/CSNATIVE.md`、`test/REAL_MACHINE_TEST.md` | `keep pointer=10`；`delete-pointer=2` | 保留的 10 个指针不保存当前正文，只链接各问题的唯一入口并写明删除条件。context worktrees 与旧 document-audit 的条件已满足并删除；不预建 archive。 |

## 3. 重点压力点

### 第二期 Architecture 已足够

Architecture 的 6 份正文具有不同维护原因和失败条件：控制面注册/调用、状态事务、Gateway/provider 路由、Science executable/data-dir identity、Science capability ownership，以及总览导航。它们已经满足最小独立问题拆分，不需要因源码巨型文件、后续机器地图或本次文档审计再增加正文。`compiled / registered / product-reachable / test-only / legacy-orphan` 只由 Desktop 控制面定义；总览不再复制词表。

后续 command/event/DTO、compiled/registered/frontend-reachable/test-only/legacy 机器地图属于另一个任务。第三期 A 不提前创建该结构。

### 产品 / Science 能力地图不再分流

`docs/features/product-science-capability-map.md` 保持用户视角：入口、官方/第三方模式 support、限制和 non-target。它不维护 owner、内部机制、版本/package identity、八层证据词表或 A/B/C 探针正文。索引中原先会把读者引向 owner/证据权威的摘要已收窄。

### 大文件不是拆分结论

- `docs/operations/quality-source-gate.md`：单一 frozen source-gate specification，路径还被 quality records 引用；结论 `keep`。
- `docs/audits/v083-test-system-audit.md`、v0.8.0～v0.8.2 change audits、v0.8.4 两份 dated audits：各自绑定一个版本/调查；结论 `keep`。
- `docs/evidence/investigations/2026-07-28-claude-science-0.1.25-compatibility.md`：同一上游升级与环境证据链；结论 `keep`。
- `docs/features/codex-science-bridge.md`：当前用户、安全与信任合同 `keep`；已发布功能的旧七阶段 Plan、残留阶段/RM gate 语境删除，重复 RM-35～RM-45 表缩成 `real-machine-acceptance.md` 唯一正文链接，结论为 `keep + shrink`。
- `docs/operations/real-machine-acceptance.md`：一个从隔离护栏到收尾的 end-to-end operation；结论 `keep`。

## 4. 实际结构改动

- 在 `.agents/rules/README.md` 增加最小 task type → rule → architecture/feature/operation → quality/test/gate → audit/evidence 路由。
- 新增 `.agents/rules/reviewing.md`，独立保存 clean-context formal review 合同；继承上下文只能辅助，只有已有 HIGH/BLOCK 才升级 `gpt-5.6-sol xhigh`。
- 在文档生命周期正文固定 keep / shrink / split / delete-pointer 判据。
- 收窄 docs 与 feature 索引的能力地图描述；补 operation 的问题型路由。
- 将过期 current-release / verified-state 重复快照缩成失效警示和唯一 dated evidence 入口。
- 将架构总览中的重复可达性词表缩成 Desktop 控制面链接；UI 信息架构收口为 v0.8.0 起的当前 Feature Contract。
- 从 Codex bridge 当前合同删除过期开发 Plan 与残留阶段/RM gate 语境，并将重复真机矩阵缩成 operation 唯一入口；没有新增第二套 architecture。
- 将真机验收正文与兼容指针的版本范围统一为跨版本累积的当前 operation；矩阵存在仍不代表执行 PASS。
- 为兼容指针补齐或确认当前权威入口与可判定删除条件；保留 10 个，删除已到期且形成无信息回路的 context worktrees 与旧 document-audit 两页。
- 新增权威正文拆分：0；实际 Feature body shrink：1。没有候选需要通过新建权威正文来满足“独立稳定问题 + 不同维护原因或失效条件 + 不制造重复权威”。

未创建 `docs/decisions/`、`docs/drafts/`、`archive/`、`old-docs/` 或其他空结构。

## 5. 验证与审查

首轮 findings 修复后的候选本地验证：

| 检查 | 结果 |
|---|---|
| `git diff --check` | PASS |
| Markdown 相对链接与 anchor | 102 份现存 Markdown（含 ignored Phase 2 handoff），247 个相对目标，0 错误 |
| Agent 索引可达性 | `AGENTS.md` 到要求的 rules 节点 11 / 11 |
| Docs 索引可达性 | `docs/README.md` 到当前 architecture / feature / operation / audit / evidence / reference 节点 36 / 36 |
| 元数据与生命周期 | 保留的 10 个兼容指针均有状态与失效条件；2 个到期 pointer 已删除；4 个 current context 均有最后复核与失效条件；Phase 2 handoff 元数据齐全但已按自身 status 条件失效 |
| changed-path | 45 条当前 status 全为 Markdown；0 staged |
| dirty 保护 | 起始 digest manifest 26 / 26 路径存在；17 份字节一致，9 份变化，变化集合与下列 allowed-edit 集合精确相等；ignored Phase 2 handoff 字节一致 |
| 空结构 | 没有 `docs/decisions/`、`docs/drafts/`、archive / old-docs / deprecated-docs |
| 产品测试 | NOT-RUN；文档检查不记为产品 test PASS |

最终双 PASS 与 reviewer identity 由完成时的 successor handoff 记录；本页只保存候选与验证，避免双 PASS 后再次改动正文。

### 任务开始时 dirty digest manifest

以下 SHA-256 在任何本期编辑前捕获，覆盖 handoff 列出的 25 条 dirty Markdown 及其本身。Digest 可以独立证明最终哪些路径字节一致、哪些发生变化；它不能单独证明修改发生的时间或语义正确性，后者还须审查 diff。

本期 allowed-edit 集合固定为以下 9 个起始 dirty 路径；manifest 中其余 17 个路径属于 byte-preserve 集合：

| Allowed-edit 路径 | 允许变化 |
|---|---|
| `.agents/context/README.md` | 移除已满足条件的两条兼容 pointer 入站链接 |
| `.agents/rules/README.md` | 最小 task route、普通工程 fallback 与 review overlay 路由 |
| `AGENTS.md` | 统一零或一项领域规则、Context 与 review overlay 阅读预算 |
| `docs/README.md` | 问题型索引、当前 audit 入口与能力地图摘要收窄 |
| `docs/architecture/overview.md` | 删除重复可达性词表，链接唯一 Desktop 控制面定义 |
| `docs/features/README.md` | 当前 feature 问题路由与 support/non-target 摘要 |
| `docs/operations/README.md` | operation / quality / test / gate 问题路由 |
| `.agents/rules/documentation.md` | 统一文档任务的领域规则与 overlay 阅读预算 |
| `docs/operations/document-lifecycle.md` | 拆分判据、阅读预算与当前六份 architecture 收口边界 |

最终复核必须确认 26 / 26 路径仍存在，byte-preserve 集合逐项一致，changed 集合精确等于 allowed-edit 集合；allowed-edit 的语义仍需从当前 diff 判断。

```text
a730fa149e628f2e18d2b9cb06952e7b624bc1454353db0d7a6ea91157e7f8ce  .agents/context/README.md
e86880db030bf91c332bb98d6e08711a19ec0e47a48536d588d82ae7d8ba2a27  .agents/handoffs/README.md
35f937688f963e9087e4911efb5dc0c9fe4aac492d923c174241f59bc00634e7  .agents/rules/README.md
acb1ac3ea7b525e25a6485e73775ce417dbc05dfcd48153bf641e5a6a2554685  .agents/rules/system-ssh.md
dc96f8055fadfa523c55068dbd679b81de8207d4064433e64f491c84370dedbd  AGENTS.md
de9f5e71cce9e7b0146c9fb89ac1b8edee06cf79d98cd253c34070c2d51bfb60  docs/README.md
f40e515b8b7e67b99e46136f60b72727e4298cd6a3ab7dfa40e46d688f867886  docs/architecture/README.md
9abad30f4dd80cffbeac51f403ed4a8eae9a4d1e59be62eefe379baf4cae69ba  docs/architecture/overview.md
52c9e300182e500a6cc8683e02429d4d09dd923c66a3c88824ad803c6ea9a80a  docs/architecture/science-runtime.md
716728716699edaeaa3c21c36da25b1804b09f3af1febd15e7fc43d67d02f667  docs/evidence/investigations/2026-07-28-claude-science-0.1.25-compatibility.md
9c5ec987da7a9aa71d376aad3ca4706cdbaae3c434ae03b96023dd200ad3a784  docs/features/README.md
169842c49fc89acf27a7fcb433f7710bdd2a3e58dd2a24a1fde6fa9616020b06  docs/features/codex-browser-login-models-implementation-plan.md
0943cd536e3c93472e632c2481f5314a771d68e18ca7b89bf209a01d0b931bfe  docs/features/system-ssh.md
51ce212411507082c03694f20ea3211dffa44f8114965045eef934d97f97e0c2  docs/operations/README.md
aad67c82cd193c5367eafd6c19726074e3c283866a191b4f86387f5053e73e1b  docs/operations/development.md
6479a2c7a6e2b8f6bcb50ad0c40ae4ea0ddc577c769121d365d4507d146afdb2  .agents/context/current-main.md
9ee48c6bc337a7adf41dd66f684baeaa90e804c1f76134e0cfb029d21ed5b21d  .agents/rules/documentation.md
95d519c2e5cc1ac00e4fe4f491080a97c085cbda40c40b9f47a14573a145ff85  docs/architecture/desktop-control-plane.md
1d5a682b68bf7d884739638cfcc6ac10bcd156b3e9b37f34507b1b5f621672c7  docs/architecture/gateway-provider-routing.md
d1623bd509c281f98aba7f04f50703de2a7cafbf2001a8669c80080c95da5317  docs/architecture/runtime-state-transactions.md
e25cf38c5643ab69f15df6ce264372be8f4293254e766ca1cfebaf017f7d8cb0  docs/architecture/science-capability-dependencies.md
41e8ac3a70252c811445e2d1e38b0e98f75635c05857b88ef5dc7c1c35f71a09  docs/audits/2026-07-29-v084-main-baseline.md
244dcd0bb34a2e9140badfa63ed1e18f86b80d3e152a63b00262b8f5a36b7c96  docs/audits/2026-07-30-v084-architecture-reconnaissance.md
b72744b7965c79a43270a00f971e6f3b08bbb35b7aa7a749b3d7654e3457387e  docs/features/product-science-capability-map.md
3d5f004dfef0015db658af61dfb77d3f24a0aae525925b6c50b14026a45252a1  docs/operations/document-lifecycle.md
cfde03146c06baa5a8b182cb725a92704bf17fd8b90396fea6454501e43fbb33  .agents/handoffs/2026-07-30-phase2-to-phase3a.md
```

## 6. 未进入的后续范围

- 第三期 B：Markdown/link/index/lifecycle 最小自动门禁及 quality/test catalog 连接；
- Science A/B/C 动态探针合同与 fixture；
- CSSwitch compiled/registered/frontend-reachable/test-only/legacy 机器地图；
- command/event/DTO、状态事务、诊断与测试映射；
- 产品代码重构、quality JSON/schema/test 修改或真实 runtime 验收。
