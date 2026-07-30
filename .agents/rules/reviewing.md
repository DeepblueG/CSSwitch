# 正式独立审查规则

- 真正的正式 reviewer 默认从 clean context 启动，不继承实现窗口对话；多 Agent 调用使用 `fork_turns="none"`。
- 本规则是正式审查 overlay，不计入零或一项领域规则预算；reviewer 最多再读一项被审领域规则，无匹配项时只使用本 overlay。它不放宽一个索引、最多两份正文或 Context 的默认首轮预算。
- reviewer prompt 必须自包含：精确 worktree / branch / HEAD、候选范围、允许与禁止项、权威入口、应执行的只读检查、严重度定义和期望输出。
- 继承上下文的审查只能提供辅助线索，不能满足最终独立审查 gate。
- 默认使用常规 reviewer；只有已经报告 `HIGH` 或 `BLOCK` 时，才把该 finding 窄化后升级为 `gpt-5.6-sol xhigh` 裁决，不用高配 reviewer 代替首轮分类。
- 修复会使受影响审查线的旧 PASS 失效；必须换一位新的 clean-context reviewer 复审。
- 正式结论必须显式报告 `clean-context: YES|NO`、findings 及最终 `PASS|FAIL`，不得用“未发现”替代 gate 结果。
