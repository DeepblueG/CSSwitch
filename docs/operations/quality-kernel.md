# Quality kernel v1

`quality/` 是 v0.8.3 质量内核的机器事实源：严格 schema、requirements、change records、bug records、test catalog、gates、lineage 和 production path policy 共同描述“应验证什么、证据能证明到哪一层”。变化登记必须随生产路径变化一起更新；记录问题不等于修复问题。

本节点只提供 source/unit 级的 metadata、impact-pr 和 impact-release validator，以及隔离 focused tests。`source-test` / `source-green` 只表示当前源码和机器元数据验证，不表示 artifact、installed、live provider/Science、signing/notarization 或 public release。

旧 `test/run_all.sh` / S0 门仍登记为 `legacy-known-unreliable`，在后续节点切换前不能被本页或 quality kernel 重新命名为 release-ready。产品问题与质量体系缺陷分别登记；任何 `open-not-fixed` 记录都不能写成 fixed。

`impact-pr` 必须显式给出 target ref，并以 Git merge-base 计算影响范围；`impact-release` 固定使用 lineage 中 v0.8.2 annotated tag 的 peeled commit，并要求 clean、非 shallow、祖先关系和生产路径闭合。audit baseline 只用于审计参照，不能作为 release impact base。

lineage 不保存当前 worktree 的 candidate SHA，也不把 live HEAD 写回机器事实源；未来 run/release evidence 运行时绑定 candidate SHA，并同时记录其 source/test-result 证据。`TestImpactV1` 只允许 `add`、`update`、`existing-sufficient`、`manual-evidence`、`not-yet-automatable` 五态；高风险变化不能用后两态逃避自动回归。Bug 的 `confirmed_facts` 与 `hypotheses` 分开保存，后者不构成 observed 或 fixed 证据。

未来 `TestResultV1` 同时记录 `outcome`、`classification`、`gate_decision` 和真实记录的 `exit_code`；PASS 必须是 `PASS/NONE/PASS/0`，任何非 PASS 结果不能伪装成零退出。`evidence-manifest` 的每个结果采用与 `test-result.v1` 完整等价的六分支 `oneOf` 语义（包括 FLAKY、QUARANTINED、readiness timeout 和 blocked），不能只依赖 `result_schema` 标记。`run-manifest`、`evidence-manifest` 与 `release-candidate` 均绑定 `test-result.v1`，但本节点不实现 runner。

生产路径只能由 active matching ChangeRecord 覆盖；该 change 的 `test_impact.required_suite_ids` 与 `required_gate_ids` 必须分别包含具体 policy 的全部 required suites/gates。retired-only、无关 suite 或无关 gate 均 fail closed。
