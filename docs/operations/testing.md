# 自动测试与证据判定

## 权威 source gate

当前唯一完整 source/unit 入口是固定的 `GATE-SOURCE`：

```bash
GATE_ROOT="$(mktemp -d /private/tmp/csswitch-source-gate.XXXXXX)"
chmod 700 "$GATE_ROOT"
bash test/run_all.sh --output-root "$GATE_ROOT"
```

`GATE_ROOT` 必须是绝对、canonical、当前用户拥有、空的 `0700` 目录，且不能位于
仓库内。`test/run_all.sh` 只是兼容命令名，只接受上述 `--output-root` 参数并转发到
隔离 Python CLI；无参数调用和旧 `--require-release-ready` 均返回 usage error。

Gate 只接受 clean、non-shallow 的 exact `HEAD`，固定顺序执行
`quality/release-gates.v1.json` 中 `GATE-SOURCE.required_suite_ids` 的 15 个 suite。
命令、测试 identity、允许环境、timeout、无 retry 与聚合证据均由
`quality/test-catalog.v1.json` 和 trusted run-evidence 合同绑定。公共 CLI 不能选择
子集。

报告至少记录命令、退出码、exact `HEAD`、输出目录、最终 completion seal / aggregate
判定与 15 个 suite 状态。只有递归验证后的 PASS seal 才建立
`RUN-EVIDENCE-GREEN` 与 `SOURCE-GREEN`；stdout 摘要或某个组件通过都不是权威。
完整冻结合同见 [Trusted source gate v1](quality-source-gate.md)。

## 聚焦诊断

`test/run-offline.sh`、`test/run-loopback.sh`、`test/run-scripts.sh`、
`test/run-rust.sh` 和 `test/run-frontend.sh` 仍可用于定位相应组件问题，但它们不是
当前完整 gate，也不能单独建立 `SOURCE-GREEN`。旧 `S0_LAYER`、
`current-env clean` 与 `release-ready green` 只用于解释历史 evidence，不是当前
候选的结果词汇。

文档治理的定向入口是：

```bash
python3 -m unittest test.test_document_governance -v
```

它登记在 `quality/test-catalog.v1.json` 的既有 `SUITE-PY-OFFLINE`，由完整
`GATE-SOURCE` 执行；覆盖范围与不能外推的证据层以
[文档治理合同](document-lifecycle.md)为准。

## 自动化没有证明的层

`GATE-SOURCE` PASS 仍不自动证明：

- `.app` / DMG 从目标 commit 构建且内容正确；
- 临时安装副本或 installed runtime 可用；
- 当前 Claude Science 版本兼容；
- 外部 Skill 的自然语言路由、领域功能或重启持久化；
- 特定真实 provider / SSH server 可用；
- Developer ID 签名、notarization、Gatekeeper 或公开 release 附件一致。

这些层分别使用[真机验收](real-machine-acceptance.md)、[发布流程](release.md)和 dated evidence。

## 报告词汇

- `PASS`：目标 gate / suite 已执行、身份绑定且满足判据；
- `失败`：已执行但不满足；
- `PREFLIGHT / ENV-BLOCKED`：当前环境或候选前置不满足，不能视为通过；
- `NEEDS-REAL-MACHINE`：必须在指定真机 / artifact 上执行；
- `未执行`：没有取得该层证据；
- `需人工判断`：机器结果不足以自动确定。

mock / loopback、built artifact、installed copy、runtime、live provider 与发布附件必须分栏记录。
