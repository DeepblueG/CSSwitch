# CSSwitch v0.8.4 真实主干基线

审计日期：2026-07-29（Asia/Shanghai）

本页固定“治理开始时”的主干定位、证据层、源码结构和 v0.8.4 hotfix 边界。它不是测试通过报告，也不代替逐版本 release evidence、真机验收或产品能力地图。可漂移的短入口见 [current-main context](../../.agents/context/current-main.md)。

## 1. 结论

治理工作的真实源码基线是：

```text
origin/main
  = local main
  = v0.8.4 peeled commit
  = 37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd
```

`v0.8.4` 是 annotated tag；tag object 为 `9c478c092e4b6f4a0bb80ca1f215bb6e60a87014`，peeled commit 为上面的 `37d5cfb`。

2026-07-29 实时远端查询还确认：GitHub Release `v0.8.4` 已公开，非 draft、非 prerelease，`publishedAt=2026-07-29T11:38:18Z`。Release 页面列出附件：

| 附件 | GitHub 元数据 |
|---|---|
| `CSSwitch_0.8.4_aarch64.dmg` | 12,884,340 bytes；`digest=sha256:23471daf2caa7832da3205bcc8ba97d96c057ad17539c6c7ed36aa43a3c5f2b2` |

这只建立 tag、公开 Release 页面和 GitHub 所报告附件元数据的事实。本文没有重新下载附件计算独立 SHA-256，也没有由这些元数据外推构建来源、包内容、安装可用性、Developer ID、notarization、stapled ticket、Gatekeeper 或 live provider。

## 2. 真相域必须分开

| 真相域 | 本次建立的事实 | 不能外推 |
|---|---|---|
| 远端 `main` | 2026-07-29 实时核验为 `origin/main=37d5cfb` | 未来远端不漂移 |
| 本地 `main` | `/Users/superjj/ccproj/CSswitch-main` 当时为 `main...origin/main`，HEAD=`37d5cfb` | 其他 worktree 等同主干或也干净 |
| tag | annotated tag object=`9c478c09`；peeled=`37d5cfb` | tag 的 GPG 签名、app 签名或发布附件身份 |
| 公开 Release | `v0.8.4` 已公开；状态、时间、附件名称、大小和 GitHub digest 如上 | 本地构建字节与附件一致，或已安装 app 来自该附件 |
| 治理工作树 | `/private/tmp/csswitch-main-governance-v084-20260729` 的分支 `codex/main-baseline-capability-map-v084` 从 `37d5cfb` 开始；这是文档治理工作树，不是 `main` | 工作树中的未提交治理文档已进入 `main` |
| 源码 | 桌面版本文件一致为 `0.8.4`；主要组件和当前入口见下文 | 源码已经构建、测试或被公开附件采用 |
| source evidence | quality records 将 Science hotfix 标为 `source-reproduced` / `source-fixed-product-pending`，并引用 exact parser regression 与隔离 updater snapshot/version oracle | 本页重跑了这些测试、完整 `GATE-SOURCE` 已通过，或产品 gate 已闭合 |
| product gate | `GATE-SCIENCE-MACHO` 仍为 active `product-open-not-run`；其他 11 个 active product gates 也是同一 release claim | hotfix 已在 final artifact、installed、live、signing 或 public 层通过 |
| artifact | Git 仓库没有跟踪 `.dmg`、`.pkg` 或 `.app` payload；公开附件元数据见上 | 本次检查过本地或下载后的 app/DMG 内容 |
| installed / live | 未读取或运行真实用户状态 | installed Science/CSSwitch、真实账号、provider、SSH 或 Skill 领域功能通过 |
| signing / distribution security | 本次未执行签名、公证或 Gatekeeper 检查 | annotated Git tag 或 GitHub digest 等于 macOS 分发签名 |

## 3. 复核入口

本次用于固定 Git 与源码事实的只读入口包括：

```bash
git status --short --branch
git worktree list --porcelain
git rev-parse HEAD
git show-ref --verify refs/heads/main
git show-ref --verify refs/remotes/origin/main
git rev-parse v0.8.4
git rev-parse 'v0.8.4^{}'
git cat-file -t v0.8.4
git cat-file -p v0.8.4
git log --reverse v0.8.3..v0.8.4
git ls-files '*.dmg' '*.app/**' '*.pkg'
```

远端引用、Release 页面、worktree 和 dirty state 都会变化；使用本页前仍须实时复核。证据措辞以 [测试与证据规则](../../.agents/rules/testing-and-evidence.md) 为准。

## 4. v0.8.4 相对 v0.8.3 的精确增量

`v0.8.3` peeled commit `c8ca30f024a974621feaaf8cb94376d60bfb16db` 到 `v0.8.4` 恰好 3 个 commit：

| Commit | 主题 | 源码树中的作用 |
|---|---|---|
| `6cec769253578c53ebf3452f7bf72ecc40ba6332` | `fix(science): accept app-seeded updater identity` | 产品版本升至 0.8.4；Science 固定 updater 路径接受两个 exact identifier；更新 source test identity |
| `6d2c8006a720dc6c226abfcebeb13cb383b21238` | `test(science): close updater identity evidence` | 补强 exact spoof regression、调查证据和 bug/change source 边界 |
| `37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd` | `docs(science): align updater identity contract` | 对齐 requirement 与 product gate 的 source-fixed / product-pending 口径 |

差异涉及 15 个文件、106 insertions / 80 deletions；没有新增产品模块或协议面。除版本文件和质量/证据索引外，生产代码变化集中在 `desktop/src-tauri/src/runtime/science.rs` 的 updater embedded identity 判断。

## 5. v0.8.4 Science updater hotfix

### 5.1 source-reproduced 与 source-fixed

v0.8.3 只允许固定 updater 路径中的 `com.anthropic.operon`。现场证据表明该路径也可能包含由 App seed 写入、identifier 为 `com.anthropic.operon.cli` 的有效 0.1.25 executable，因此 v0.8.3 会在 embedded metadata 边界拒绝这个已观察到的精确形态。

v0.8.4 源码只接受以下两组 exact metadata：

```text
Identifier=com.anthropic.operon
TeamIdentifier=Q6L2SF6YDW

Identifier=com.anthropic.operon.cli
TeamIdentifier=Q6L2SF6YDW
```

判断仍按整行精确匹配；未知 identifier、错误 Team ID、identifier 前缀伪造和 Team ID 后缀伪造继续拒绝。固定路径、当前用户所有权、目录/文件不可被 group/world 写入、有界 Mach-O、same-open copy、SHA-256 内容寻址只读 snapshot、source stability recheck 与 snapshot reverification 边界均保留。

机器记录现在写明：

- `BUG-083-SCIENCE-MACHO.reproduction_state=source-reproduced`；
- `BUG-083-SCIENCE-MACHO.resolution_state=source-fixed-product-pending`；
- `CHG-SCIENCE-MACHO.claim_state=confirmed`，同时保留 `does_not_claim_fix=true`；
- exact parser regression 为 `official_updater_identity_parser_accepts_only_known_exact_variants`；
- 隔离 source oracle 为 `real_updated_runtime_candidate_is_eligible_without_reading_real_science_data`。

这些是仓库中已登记的 source-test 事实。本页没有重跑测试，因此也不新增一次 test result。

### 5.2 产品 gate 尚未闭合

`GATE-SCIENCE-MACHO` 仍为 active product gate：

```text
evidence_layer=source-test
release_claim=product-open-not-run
candidate_policy=required
```

因此“source-reproduced/source-fixed”不能改写成“v0.8.4 产品已验证”。最终 app/DMG、installed runtime、live Science/provider、macOS signing、公证/Gatekeeper 和公开附件内容仍分别需要对应层证据。embedded identifier / Team ID 本身也只是格式与误选护栏，不是官方来源的密码学证明。

## 6. 源码版本与主要产品块

以下版本文件在 `37d5cfb` 一致为 `0.8.4`：

- `desktop/package.json`
- `desktop/package-lock.json` 的根 package
- `desktop/src-tauri/Cargo.toml`
- `desktop/src-tauri/Cargo.lock` 的 desktop package
- `desktop/src-tauri/tauri.conf.json`

辅助 Rust crate 使用自己的内部包版本（例如 Gateway `0.0.0`、Skill package `0.1.0`），不能拿这些 crate 版本推翻桌面产品版本。

以下是当前源码中可以定位的主要产品/代码块。“源码存在”只表示代码与合同入口存在，不表示本次运行验证：

| 产品/代码块 | 当前源码入口 | 所有权与边界 |
|---|---|---|
| Desktop UI | `desktop/src/index.html`、`desktop/src/main.js`、页面 state 模块、`styles.css` | 模型连接、Skill & MCP、状态、设置等 WebView 表面；用户可见合同以根 README 与 feature docs 为准 |
| Tauri 控制面 | `desktop/src-tauri/src/lib.rs`、`commands/` | 注册 UI command，持有 app state，编排配置、Gateway 与 Science；不承担上游 provider 协议实现 |
| Profile、配置与模型目录 | `config.rs`、`config_legacy.rs`、`commands/profiles.rs`、`model_catalog.rs`、`catalog/*.json` | CSSwitch 的 provider/profile/settings 与严格模型映射 source of truth |
| Runtime 与一键开始 | `runtime/`、`commands/runtime.rs`、`lifecycle.rs` | Gateway/Science 生命周期、切换事务、恢复、诊断和失败补偿 |
| Rust Gateway | `desktop/gateway/` | loopback sidecar；Anthropic/OpenAI/Codex 协议、SSE、模型与 provider 路由；生产路径没有 Python proxy fallback |
| Codex 实验能力 | `commands/codex.rs`、`desktop/gateway/src/codex_*`、`desktop/codex-network/` | CSSwitch 自有 browser-only OAuth、私有状态、动态模型和 Responses bridge；默认关闭，不读取原生 `~/.codex` |
| Science runtime 集成 | `runtime/science.rs`、`runtime/sandbox_session.rs`、`science_control.rs` | executable 选择、内容寻址 snapshot、隔离 data-dir、健康/身份、启动/恢复/停止 |
| 外部 Skill package/bridge | `desktop/skill-package/`、`commands/skill_install.rs`、`commands/skill_listing.rs`、`runtime/skill_install_bridge.rs`、Gateway `skill_install.rs` | 公开 GitHub URL与本地 `.zip`/`.skill` 的窄安装/卸载桥；不是通用 Skill 市场或通用 MCP 管理器 |
| 系统 SSH bridge | `runtime/ssh_bridge.rs`、`scripts/ssh-bridge/` | 默认关闭；用户 opt-in 后按安全合同 fail closed；不复制真实 `.ssh` |
| 诊断与发布入口 | `commands/diagnostics.rs`、`scripts/`、`docs/operations/` | doctor、日志/issue/release 入口与运维脚本；不等于已建立 installed/live 证据 |
| 质量与证据内核 | `quality/`、`test/quality/`、`test/` | requirements、bug/change records、test catalog、source gate 与分层证据合同 |

稳定架构边界从 [架构总览](../architecture/overview.md) 与 [Science runtime 合同](../architecture/science-runtime.md)进入。当前产品能力和 Science/第三方边界由 [产品 / Claude Science 能力地图](../features/product-science-capability-map.md)维护，不从本表的代码路径猜测。

`desktop/src-tauri/src/skill_manager/` 与 `commands/skills.rs` 仍保留在源码树中，但当前 `lib.rs` 没有声明 `skill_manager`，`commands/mod.rs` 没有声明 `skills`，Tauri invoke handler 也没有注册其中命令；它们是未编译、未参与当前 runtime 的 legacy 源码，不能列作现行产品入口。

## 7. 治理与测试入口

当前仓库已经有分层治理骨架：

| 入口 | 用途 |
|---|---|
| [`AGENTS.md`](../../AGENTS.md) | 最短规则入口、阅读顺序与权威顺序 |
| [`.agents/rules/`](../../.agents/rules/README.md) | 安全、Git/worktree、测试/证据、发布、Science、Skill、SSH 的稳定行为规则 |
| [`.agents/context/`](../../.agents/context/README.md) | 可漂移当前状态的短索引；使用前实时复核 |
| [`docs/architecture/`](../architecture/README.md) | 跨版本架构、所有权、数据流、失败边界 |
| [`docs/features/`](../features/README.md) | 用户行为和功能合同 |
| [`docs/operations/`](../operations/README.md) | 开发、测试、真机、发布、升级与质量内核 |
| [`docs/evidence/`](../evidence/README.md) | 日期化调查和逐版本发布证据 |
| [`quality/`](../../quality/) | 机器可读 requirements、change/bug records、test catalog、gates 和 schema |

`37d5cfb` 的真实默认 source-gate 入口不是旧五层命令合同。当前 `test/run_all.sh` 只接受：

```bash
bash test/run_all.sh --output-root ABS_EMPTY_0700_DIR
```

它转发到：

```bash
/usr/bin/python3 -I test/quality/source_gate/cli.py run \
  --output-root ABS_EMPTY_0700_DIR
```

机器事实源 `quality/release-gates.v1.json` 当前共有：

- 4 个 active non-product gates：3 个 quality metadata/impact gates（`metadata`、`impact-pr`、`impact-release`）与 1 个 `source` profile 的 `GATE-SOURCE`；
- 12 个 active product gates，`release_claim` 全部为 `product-open-not-run`。

`GATE-SOURCE` 固定 15 个 source/unit suites，覆盖 quality metadata/focused/inventory、run-evidence contract、Python offline/loopback、四个 Rust manifest、shell、frontend、两个历史 orphan suite 和 source-gate contract。四个 Rust 主测试模块边界分别是：

| Suite | Cargo manifest |
|---|---|
| `SUITE-RUST-DESKTOP` | `desktop/src-tauri/Cargo.toml` |
| `SUITE-RUST-GATEWAY` | `desktop/gateway/Cargo.toml` |
| `SUITE-RUST-CODEX-NETWORK` | `desktop/codex-network/Cargo.toml` |
| `SUITE-RUST-SKILL-PACKAGE` | `desktop/skill-package/Cargo.toml` |

本次文档基线没有执行 source gate 或组件测试。因此本页不声明 `RUN-EVIDENCE-GREEN`、`SOURCE-GREEN`、`current-env clean` 或 `release-ready green`。即使未来 `GATE-SOURCE` 得到有效 PASS，它仍只是 exact-HEAD source/unit 证据。

## 8. 巨型文件热点

以下数据来自 `37d5cfb` 的 `wc -l`。第二列定义为“文件末尾主要 `#[cfg(test)]` 测试模块开始之前的行数”；文件中可能另有零散 `cfg(test)` helper，因此它是拆分定位指标，不是纯 production SLOC。

| 文件 | 总行数 | 末尾主要测试模块前 | 初步风险 |
|---|---:|---:|---|
| `desktop/src-tauri/src/runtime/sandbox_session.rs` | 9,711 | 6,203 | 启动事务、authority snapshot、恢复/补偿与大量回归集中 |
| `desktop/src-tauri/src/commands/runtime.rs` | 7,484 | 984 | 生产 command 较薄，但约 6,500 行尾部测试集中 |
| `desktop/src-tauri/src/skill_manager/store.rs` | 4,405 | 3,149 | 未编译 legacy 源码热点；store、路径/权限、持久化与恢复边界集中，但不参与当前 runtime |
| `desktop/gateway/src/server.rs` | 3,977 | 2,215 | HTTP server、请求处理与 Skill host 路径集中 |
| `desktop/src-tauri/src/config.rs` | 3,707 | 2,449 | schema、迁移、校验与持久化集中 |
| `desktop/gateway/src/codex_protocol.rs` | 3,517 | 2,445 | Codex/Anthropic 会话与协议状态复杂 |
| `desktop/src-tauri/src/runtime/science.rs` | 3,412 | 2,250 | runtime 选择、双 exact updater identity、健康和停止合同集中 |
| `desktop/src-tauri/src/commands/codex.rs` | 3,325 | 2,093 | OAuth/config/模型 command 与测试集中 |
| `desktop/src-tauri/src/skill_manager/deployment.rs` | 2,896 | 1,617 | 未编译 legacy 源码热点；deployment 编排和回滚集中，但不参与当前 runtime |
| `desktop/src/main.js` | 2,865 | 2,865（无 Rust 式尾部测试模块） | 前端页面总控制器仍然集中 |

对照项：`desktop/src-tauri/src/lib.rs` 为 684 行。此前把后端入口拆成 `commands/`、`runtime/` 等当前模块及后来退出编译路径的 `skill_manager/`，确实降低了根模块集中度，但复杂度已在若干当前中心模块和 legacy 源码中重新聚集。是否拆分以及如何拆分必须先区分当前编译图与 legacy 清理，再结合依赖/状态机审查，不能只按行数判断。

## 9. 已确认的文档漂移

本轮把 `docs/README.md` 的当前维护基线改为 v0.8.4，并接入本审计与能力地图。下表只列本轮仍未关闭、保留为后续治理输入的漂移：

| 位置 | 当前文字 | 与 `37d5cfb` 的关系 |
|---|---|---|
| `.agents/context/current-release.md` | 当前 macOS 正式版本仍为 v0.8.2 | 落后于已实时核验的公开 v0.8.4 |
| `.agents/context/verified-state.md` | 当前维护基线和已验证状态仍为 v0.8.2 | 历史 v0.8.2 证据不能自动升级；需要单独整理 v0.8.4 已建立/未建立层 |
| `.agents/context/known-issues.md` | 标题仍以“v0.8.2 已发布后”为主 | 部分边界仍有效，但需逐项对照 v0.8.3/v0.8.4 change/bug records |
| `docs/architecture/overview.md` | 自称 v0.7.0 当前架构合同 | 内容含后续能力，但版本标签和部分 release-specific 语句已陈旧 |
| `docs/architecture/science-runtime.md` | 自称 v0.7.0 稳定合同，仍用单数“当前 Science 身份”描述 updater embedded identity | 树中已有 0.1.25 与 v0.8.4 双 exact identifier hotfix，需按稳定合同/日期化证据重审 |
| `docs/operations/development.md` | 自称 v0.8.1；仍给出无参数 `run_all.sh` 和旧 `--require-release-ready` | 与当前 fixed source-gate CLI 不一致 |
| `docs/operations/testing.md` | 仍描述旧五层 S0、`current-env clean` / `release-ready green` | 与 active `GATE-SOURCE` 和当前 wrapper 不一致 |
| `docs/operations/real-machine-acceptance.md` | 标题及入口仍为 v0.8.1/旧五层 | 验收矩阵可作输入，但测试启动与证据词汇需要重新对齐 |
| `docs/operations/quality-source-gate.md` | 自称 v0.8.3 regression-hardening 的 frozen implementation specification | 可解释现有 gate 设计，但不是 v0.8.4 当前运行结果 |
| `docs/operations/upgrade-and-rollback.md` | 适用于 v0.8.1 | 不能作为 v0.8.4 当前升级说明直接引用 |
| `docs/evidence/releases/README.md` | 尚无 v0.8.3 或 v0.8.4 release evidence 入口 | GitHub Release 元数据存在，但仓库内没有对应逐层发布证据页 |

`docs/audits/v083-test-system-audit.md` 是当时的 `BLOCK` 审计和历史输入；后续 source-gate 实现已进入树中。它不能单独代表 `37d5cfb` 当前 gate 已通过或仍以同一原因 BLOCK。

## 10. 下一步不得外推

在取得对应证据前，后续治理文档不得把以下说法写成已确认事实：

- “v0.8.4 源码已全绿”；
- “Science updater hotfix 已在最终产品闭环”；
- “公开 DMG 从 `37d5cfb` 可复现构建且包内容正确”；
- “当前已安装 CSSwitch/Science 就是公开附件/runtime”；
- “Claude Science 0.1.25 所有功能在第三方模式可用”；
- “所有 provider、Codex、SSH、Skill 或 MCP live 路径通过”；
- “annotated tag、embedded Team ID、ad-hoc seal 或 GitHub digest证明 Developer ID、公证或 Gatekeeper”；
- “公开 Release 元数据证明附件内容采用本 hotfix”；
- “巨型文件行数大，所以既有架构没有作用”——当前证据只说明复杂度重新聚集。

后续每次刷新基线，应更新日期化审计或新建一页，不把机器路径、worktree 数量和一次性 test result 堆进短 `current-main`。
