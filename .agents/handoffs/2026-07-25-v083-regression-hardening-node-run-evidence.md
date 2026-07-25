# v0.8.3 regression hardening：NODE-RUN-EVIDENCE 后交接

创建时间：2026-07-25（Asia/Shanghai）

本文件取代 RUE-05 交接，只记录当前本机 checkpoint 与证据。继任窗口必须重新读取规则、known issues、quality kernel、audit 与实时 Git 状态；不得把本文件提升为 release 或产品事实来源。

## 开始前实时复核

所有命令必须显式在以下 worktree 执行：

```text
/private/tmp/CSswitch-v083-regression-hardening
```

先运行：

```bash
git status --short --branch
git worktree list --porcelain
git rev-parse HEAD
git rev-parse origin/main
```

交接时身份：

- branch：`codex/v083-regression-hardening`
- trusted runner kernel code checkpoint：`2d8454b7a1cdfd6b61c789cd315505eaea813b93`
- `origin/main`：`4e0af6ba7909dca22f1257b168172ecbe4af4836`
- 本 handoff 由后续 docs-only 本地 checkpoint 固化；最终 HEAD 以实时 `git rev-parse HEAD` 为准
- 预期 worktree：clean

不得吸收、修改、stage、commit 或清理其他 worktree，尤其：

- `/Users/superjj/ccproj/CSswitch`
- `/Users/superjj/ccproj/CSswitch-main`

## 已完成

从 RUE-05 clean checkpoint `789c1b0c61f89231b735c6733a42da7c27c31e8d` 出发，已完成当前批准范围内的可信 runner kernel：

1. RUE-06 固定 retry：
   - attempt-1 仅对持久化的 `READINESS / READINESS_TIMEOUT / RC 13` 资格开放；
   - attempt-0 与 attempt-1 保存独立真实 RC、cache 与 attempt evidence；
   - recovered 结果只能是 `FLAKY_RETRY / BLOCKED / RC 11`，二次 readiness timeout 为 `READINESS_EXHAUSTED / BLOCKED / RC 13`；
   - public attempt API 无 scenario、argv、policy 或环境 override，默认无隐藏 retry。
2. 单 suite aggregation 与 completion seal：
   - result、evidence manifest 与 seal 均绑定 canonical bytes、snapshot、run manifest、attempt history 与固定 identity；
   - missing、partial、contradictory、replayed、ignored、skipped、ENV、REAL 与 adapter corruption 均 fail closed；
   - completion slot one-way，seal 使用专用 exclusive rename 作为成功线性化点。
3. 最薄固定 CLI / catalog / node governance：
   - 唯一入口：`/usr/bin/python3 -I test/quality/run_evidence/cli.py run --output-root ABS_EMPTY_0700_DIR`；
   - 只选择 `SUITE-RUE05A / ENTRY-RUE05A-ATTEMPT0`；
   - `sys.flags.isolated`、精确 dependency version/RECORD/payload/origin、Python/Git tool digest、Git HEAD/origin-main/merge-base、clean snapshot、input/environment/output-root binding 均 fail closed；
   - catalog suite 与 selection rule 固定，当前 node 不进入任何 release gate；
   - stdout 只在 valid seal 后输出，decision、runner exit、run id 与 seal/result 一致。
4. adversarial E2E 已覆盖：
   - pass marker + child RC 7；
   - readiness retry、hard timeout、missing/malformed/extra adapter；
   - ignored/skipped/ENV/REAL；
   - 非 `-I` 启动、catalog/input/tool drift、replay、preoccupied output；
   - post-seal output-root rebind；
   - post-seal helper-owned public fd 与 final root fd close-report OSError；
   - stat/open/fstat/mode/nlink/inode binding failure 仍然 fail closed。

code checkpoint：

```text
2d8454b test(quality): complete trusted node runner kernel
```

## 最终 clean 证据

在真实 clean code checkpoint `2d8454b7a1cdfd6b61c789cd315505eaea813b93` 上执行固定 CLI：

```text
NODE-RUN-EVIDENCE PASS
scope=fixed-one-suite-focused-source-unit
runner_exit=0
run_id=318cd59d0a05ad5b37f2619c497cf022
evidence_path=/private/tmp/csswitch-node-evidence-final.qmx2xI/evidence/runs/318cd59d0a05ad5b37f2619c497cf022
```

独立回读确认：

- `completion-seal.json`：`PASS / runner_exit 0`
- `results/SUITE-RUE05A.json`：`kind PASS / gate_decision PASS / runner_exit 0`
- `run-manifest.json`：`head_sha=2d8454b7a1cdfd6b61c789cd315505eaea813b93`
- stdout、seal、result 的 run id、decision 与 RC 完全一致

clean checkpoint 上门禁：

- run-evidence：`170/170 PASS`
- quality-kernel：`13/13 PASS`
- metadata：PASS
- impact-pr against `origin/main`：PASS
- impact-release：PASS
- `git diff --check`：PASS

测试与 CLI 使用临时 HOME、临时 state/evidence、假 fixture 与只读固定 Python dependency inventory；未访问网络、端口 8765、真实凭证、Keychain、数据库、SSH、Science、`/Applications`、artifact/install/signing/notarization 或 public release。

完成过程经过每阶段 Sol high Spec/复审/实现/冻结/双审/root 复验；整个 node 阶段最后经 fresh Sol xhigh 完成度审查明确 `PASS / SAFE to checkpoint`。多次 reviewer BLOCK 均在修复后使旧 snapshot/review 失效并重新走门禁。

## 精确证据边界

允许声明：

```text
NODE-RUN-EVIDENCE PASS
fixed-one-suite-focused-source-unit
```

不得声明：

- `RUN-EVIDENCE-GREEN`
- `SOURCE-GREEN`
- 完整 source-test catalog 已迁移
- release-ready
- artifact、temporary install、installed runtime、live provider/Science
- signing、notarization 或 public release PASS

当前 catalog/CLI 只证明一个固定 executable suite 的真实纵向闭环；旧 Shell/Python/MJS/Rust entrypoint、旧 source gate 与完整 source catalog 迁移属于下一大目标。

## 已知 follow-up，不阻塞当前 node

1. 完整 source-test catalog / fixed runner migration：
   - 把批准的旧入口逐项迁入可信 runner；
   - 替换旧 source gate；
   - 重新定义并验证更高层 `RUN-EVIDENCE-GREEN` / `SOURCE-GREEN`，不得反向扩大本 checkpoint。
2. supporting unittest discovery 在 bare 临时 HOME 下不会自行发现用户 site-packages 的 `jsonschema`；审查使用临时 HOME 加显式只读固定 site-packages 路径。固定公开 CLI 不依赖此入口，它会在 `-I` 下自行校验 exact dependency inventory。此项是未来 test-support 可复现性 follow-up，不是当前 NODE claim blocker。
3. 用户报告“完整交互结束后切换模型会出问题”。另一个脏 worktree `/Users/superjj/ccproj/CSswitch-main/.agents/context/known-issues.md` 曾有未提交措辞，只允许未来窗口只读重新确认；不得复制、修改、stage 或把它当成目标树既成事实。该问题登记为未来产品/source-gate 回归输入，本阶段没有构造出 runner false green 或错误证据升级，因此不阻塞 `NODE-RUN-EVIDENCE`。

## 下一步与暂停

本大阶段到此暂停。下一窗口如获用户明确授权，可从最终 clean HEAD 规划“完整 source-test catalog 与 source gate 迁移”大目标；不得自动进入产品缺陷修复、artifact/install/live/signing/release，也不得 push、tag、PR、rebase、删除分支或清理 worktree。
