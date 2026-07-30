# CSSwitch v0.8.4 架构与 Claude Science 边界调研

审计日期：2026-07-30（Asia/Shanghai）

源码基线：`37d5cfb6600a0022d5e1bbbe9a7e181a917b12dd`

工作分支：`codex/main-baseline-capability-map-v084`

本文固定一次源码与上游边界调研，保存版本、内部符号、外部资料和后续探针。稳定边界已经提炼到[架构索引](../architecture/README.md)；本文不冒充当前产品合同，也不证明最终 artifact、安装态、live runtime 或公开附件内容。

## 1. 范围与方法

输入是三份已完成的只读调研：

- CSSwitch 源码架构调研任务 `019faeab-41d2-7582-a8ac-962e8022de0c`；
- Claude Science 边界核实任务 `019faeab-8044-7162-9514-4b8d4d778f93`；
- Claude Science 0.1.25 功能全景与 installed-static 调研任务
  `019fb10b-812d-76a3-8c70-6e3cd0a9ca18`。

三份最终报告均在落盘前完整读取。随后在唯一目标 worktree 中抽查：

1. `lib.rs` 的 Rust module graph、Tauri command 注册、event 与 auto-boot；
2. `main.js` 的生产 caller/listener 与 preview-only `mockInvoke`；
3. `AppState`、`Config`、`Lifecycle`、operation trace、runtime journal、authority snapshot 与 managed receipt；
4. Gateway server、scratch、`codex-auth`、`skill-install-mcp`、`science-control`；
5. Science data-dir、隔离 HOME、protected projection、opaque roots、SSH stub/wrapper 和 Plugin 子集解析；
6. Anthropic / Claude 官方公开文档与本地 0.1.25 package/evidence。

本轮没有运行 Science、账号、provider、SSH 或 B/C 动态探针；没有读取真实 `~/.claude-science`、Keychain、OAuth、SSH key 或账号数据库；没有重跑产品测试。

## 2. 证据词表

| 层 | 本文中的含义 | 不能外推 |
|---|---|---|
| `EXTERNAL-OFFICIAL` | Anthropic / Claude 官方公开资料在访问日说明的上游能力或合同 | 指定 Science package 已实现、第三方模式可达、账号有 entitlement |
| `SOURCE-CONTRACT` | exact HEAD 的编译、注册、状态或协议边界 | source test PASS、final artifact 或 installed/live |
| `SOURCE-TEST` | exact HEAD 中存在的单测、边界测试或 quality 登记 | 本轮已运行或产品 gate 已闭合 |
| `PACKAGE-STATIC` | 指定获取包、DMG 或 updater 的 CLI、route、字符串、metadata 或静态表面 | 该包当前已安装、route 成功、账号能力或 live 行为 |
| `HISTORICAL-ISOLATED-LIVE` | 指定旧候选、旧 Science 和临时 HOME/data-dir 的历史运行结果 | 当前 HEAD、当前 0.1.25 或最终 artifact 仍成功 |
| `FINAL-ARTIFACT` | 已固定 hash、内容和构建身份的候选 App/DMG | 已安装、运行或公开附件与其相同 |
| `CURRENT-INSTALLED-STATIC` | 调研时 `/Applications` 中指定 App/CLI 的版本、架构、identifier、hash 或签名检查 | App 已启动、账号/能力可见、行为成功或公开附件相同 |
| `CURRENT-INSTALLED-LIVE` | 精确 artifact 与 Science 版本在隔离安装态实际运行 | 真实账号/provider、其他机器或其他 artifact |
| `PUBLIC-RELEASE` | 已核实的公开 tag、Release 页面和独立验证过的附件内容 | 未经独立下载/hash/内容检查的构建来源、签名或 installed/live |

`RELEASE-METADATA` 只作为 `PUBLIC-RELEASE` 的未闭合子状态：页面报告的名称、大小或 digest 不能自动升级为附件内容证据。

`NOT-RUN` 是缺证状态，必须绑定目标证据层，例如 `target=CURRENT-INSTALLED-LIVE`；它不是第十层。真实账号、provider、entitlement、组织和区域是 live evidence 的 scope，也不是新证据层：旧候选的真实账号结果仍是 `HISTORICAL-ISOLATED-LIVE(scope=real-account)`，不能升级为当前 installed/live。

## 3. 主要 findings

### F1｜`production-source` 不等于 compiled / registered / reachable

当前源码至少有五层：

| 层 | 判定 |
|---|---|
| `compiled` | 进入当前 Rust module graph 或 frontend bundle |
| `registered` | compiled 后进入 Tauri invoke/event 或其他运行注册面 |
| `product-reachable` | 当前生产 UI、auto-boot 或受管 runtime 能实际到达 |
| `test-only` | 只在 `cfg(test)`、ignored E2E 或 preview/mock 路径进入 |
| `legacy/orphan` | 文件存在，但未进入当前 module、invoke 或 frontend 产品图 |

`quality/production-paths.v1.json` 对 `desktop/` 的目录级 `production-source` 分类是保守的影响审查入口，不证明上述任一可达层。

具体例子：

- `desktop/src-tauri/src/lib.rs::run` 是当前 Tauri 注册入口；
- `runtime/profile_switch.rs::set_active_profile_txn` 已编译但生产入口不可达，调用位于测试；
- `commands/skills.rs` 与 `skill_manager/**` 没有进入当前 module/invoke graph；
- `config_legacy.rs` 虽带 legacy 名称，仍参与当前配置迁移；
- `runtime/legacy_proxy.rs` 仍承担旧 Python listener 的精确清理。

### F2｜状态所有权与事务不是单一 `AppState`

- `AppState` 保存当前进程内 Gateway child、Gateway/Science runtime identity、boot 状态、一次性恢复引用、Science version observation cache，以及 pending authority cleanup manifest 的内存重试镜像；当前产品启动脚本退出后不保存 Science daemon child，daemon ownership 由 runtime identity、managed receipt 与 live listener 组合建立；持久 manifest 才是 cleanup 的跨重启权威；
- `Config` 持久化 profile、端口、模式、SSH/Codex 设置、path secret、runtime binding 与 transaction journal；
- `Lifecycle` 是 runtime/profile/mode 等复合操作的最外层串行器，取得该锁时的顺序为 `Lifecycle -> AppState -> config::update`；生产本地 Skill 安装不取得 Lifecycle，而以文件 picker 前后两次 runtime-context 复核、package commit 和 attach/readback 局部边界为准，安装/attach 完成后没有第三次 context 复核；
- authority snapshot、Science managed receipt、Skill marker/manifest/journal、SSH sidecar 各有独立所有权；
- config 不是跨进程共享数据库；外部并发主要由 pinned fd、no-follow、提交前比较、原子 rename/fsync 与 CAS 检测。
- `AppState` 并非所有路径都短持有：`stop_all` 当前持锁跨越 stop script 与 TERM/KILL 等同步等待。

### F3｜一键启动有三个不一致的阶段域

- `runtime/operation.rs::OperationStage` 是 typed、进程内 trace；
- `config.rs::RuntimeTransactionJournal.stage` 是持久化自由字符串；
- frontend DTO 只看到 `prepare`、`gateway_start`、`catalog_verify`、`science_stop`、`science_start` 等较粗阶段。

`commands/runtime.rs::science_failure_stage()` 根据中文/英文错误字符串推断公共阶段。authority snapshot、sandbox login、DB reverify、prior-Science stop 等内部阶段会被压缩或误投影。这是已确认架构缺口，本期不改产品代码。

### F4｜auto-boot 丢失手动一键启动的结构化错误

手动 `one_click_login` 的普通失败 resolved 为包含 `status/stage/recovery_status/environment_status` 的对象；部分 Codex typed auth 错误走 invoke rejection。auto-boot 在 `lib.rs::boot_result_error()` 只提取 `message`，再通过 `boot://failed` 发字符串，结构化字段丢失。

### F5｜5 个 registered IPC 没有当前生产 frontend caller

- `list_templates`
- `validate_profile_catalog_model`
- `preview_profile_preset_sync`
- `apply_profile_preset_sync`
- `start_proxy`

它们在 `lib.rs::run` 注册，也出现在 preview `mockInvoke` 的部分 case 中，但没有当前生产调用点。源码无法证明它们是预留公共面还是遗留面，稳定合同标为 `dormant registered / UNKNOWN`。

### F6｜Gateway 一个二进制承担四种入口

`desktop/gateway/src/main.rs` 按参数分派：

- 默认：正式或 scratch HTTP Gateway server；
- `codex-auth`：Codex 认证 CLI；
- `skill-install-mcp`：CSSwitch 外部 Skill 的窄 local stdio MCP；
- `science-control`：loopback nonce/CSRF 约束下配置 OPERON route Skill/connector。

正式与 scratch 共用 Gateway binary 和 launch contract，但端口、secret、intent、生命周期与证据结论不同。Gateway raw `CONNECT` 在 path-secret 认证前分派，只按 Anthropic/Claude hostname denylist 拒绝目标；DNS resolver 本身没有 deadline，DNS 返回后的地址连接共享剩余 10 秒预算，建立后的双向转发没有 session deadline、idle timeout、byte cap 或并发连接/session-count 上限。listener 虽为 loopback，任何本机进程仍可使用这条 TCP transport；它不是通用 HTTP/streamable MCP 管理器。

### F7｜doctor 是会改变 Skill 路由状态的 reconcile

`commands/diagnostics.rs::run_doctor()` 在诊断脚本之后进入 `Lifecycle` serializer，
强制调用 `force_third_party_reconcile()`，不是纯只读检查。Science 健康运行时，
`science-control configure-third-party` 可绑定 route Skill 和 connector、清理旧
connector、解除 `customize` 并更新 managed prompt。Science 停止、身份未知、
binary/version 变化或 bridge 身份不足时，该路径也可能使 route marker 失效。
它不会仅为 doctor 启动 Science 或 Gateway；这些 source 副作用不能外推为
artifact、installed 或 live PASS。

### F8｜auto-boot 与 Science WebView 都是环境变量条件入口

`lib.rs::decide_launch()` 只有在 `CSSWITCH_AUTO_BOOT_ON_LAUNCH=1` 时才进入
auto-boot 判定；`sandbox_session.rs::open_science_surface()` 只有在
`CSSWITCH_SCIENCE_WEBVIEW_SPIKE=1` 时才尝试内嵌 WebView，否则使用系统浏览器。
仓内未发现生产 UI 或 packaging producer。源码只能证明条件分支存在，普通安装行为
保持 `UNKNOWN`。

### F9｜外部 GitHub Skill 的 source chain 不等于 Agent runtime chain

源码存在 route 配置、GitHub package 获取/校验、host bridge、安装和 `OPERON`
attach/detach 链。当前调研没有运行最终 artifact 或 Science Agent，因此不能确认
Agent 实际 load Skill、调用 install/poll tool、完成卸载或 restart 后继续可用；
这些阶段均为 artifact/runtime `UNKNOWN`。

### P1｜Science 的 `data_dir`、HOME 与环境状态必须分开

官方资料说明：data directory 保存 per-org conversations、artifacts、delegation 和 workspace；认证 token 与 shared package environment 固定在 `~/.claude-science`，不随 `data_dir` 移动。CSSwitch 第三方模式让隔离 HOME 与 data-dir 根共址，这是部署选择，不是 Science 通用语义。

环境还要继续拆分：

- starter Conda 环境固定在 `~/.claude-science`；
- named task environment 在同机项目间复用；
- Python/R kernel 属于 session；
- 环境级安装持久，cell 内安装只到 kernel restart；
- Node 的 Science 所有权与生命周期没有同等级官方/源码证据，保持 `UNKNOWN`。

### P2｜当前 protected projection 是旧 full-tree evidence 的后继

0.1.25 兼容调查记录了旧候选对整棵 authority 做 snapshot 时遇到 Conda 大文件、runtime symlink、4.3 GiB 和 75,588 entries。当前 HEAD 改为：

- 只快照 10 个 protected entries；
- 对 `conda`、`runtime`、`seed-assets`、`r-libs`、`sbx-bind-src` 只固定顶层 no-follow owner/mode/dev/inode；
- 不递归读取、复制、fsync、删除或恢复这些 opaque roots。

两者是前后时序，不是同一合同的冲突。

### P3｜Plugin 不能与 Skills/MCP 合并，也不能声称全路径统一拒绝

官方 Science admin 文档存在 org-published plugin 表面；0.1.25 package-static 还观察到 marketplace/plugin 状态与 stage/apply/revoke/update 信号。但通用 Plugin overview 的公开 availability 只列 Claude Code/Cowork，Science 的 manifest、UI、hooks、权限和完整 lifecycle 仍不确定。

CSSwitch 没有 Plugin 产品类型或 runtime。`archive.rs` 能在特定 Plugin candidate 路径提取 Skill 子集，并在该路径拒绝 hooks/MCP/agents；根 `SKILL.md` 等其他路径可能更早短路，`${CLAUDE_PLUGIN_ROOT}` 扫描也不是所有路径共有。因此稳定表述只能是“部分解析路径拒绝；不提供 Plugin runtime”。

### P4｜SSH 必须拆成三道产品 gate

1. **Science parser acceptance**：CSSwitch 能生成 alias inventory、`ssh_hosts` 与 V2 stub；当前 0.1.25 是否接受仍未运行。
2. **OpenSSH invocation**：wrapper 源码执行 `/usr/bin/ssh -F <real config> ...`；当前 Science 是否实际经过它仍需隔离 recorder。
3. **real server**：key、agent、known_hosts、DNS、network、server 与远端命令需要另行授权，不能由前两层推出。

V2 stub 是 marker + `Host <aliases...>` + absolute `Include`，不是“只有一条 Include”。packaged wrapper validator 只检查 path components 非 symlink/目录、wrapper 为普通非 symlink 文件、大小上限与存在 executable bit；不验证内容/hash、owner、nlink、group/world writable 或精确 mode。

## 4. 当前产品可达性与缺口

| 能力面 | 当前层 | 结论 |
|---|---|---|
| Desktop -> Tauri -> profile/runtime | compiled + registered + reachable | 当前产品主控制面 |
| Gateway formal server | compiled + reachable | loopback 推理与 provider/Codex 协议 |
| scratch Gateway | compiled + reachable | 只用于候选模型/连接探测，不提交正式运行态 |
| Skill install/listing | compiled + registered + reachable | 窄 bridge；load/trigger/domain execution 分层 |
| internal Skill stdio MCP | compiled + reachable | 只服务 CSSwitch 外部 Skill |
| auto-boot | compiled + env-gated | 仅 `CSSWITCH_AUTO_BOOT_ON_LAUNCH=1`；普通安装 `UNKNOWN` |
| Science WebView | compiled + env-gated | 仅 `CSSWITCH_SCIENCE_WEBVIEW_SPIKE=1`；否则系统浏览器 |
| generic MCP/Plugin management | 无 CSSwitch 产品入口 | Science 原生面或 `UNKNOWN`，非 CSSwitch 管理能力 |
| `profile_switch::set_active_profile_txn` | compiled + test-only caller | 不算当前切换产品路径 |
| `commands/skills.rs`、`skill_manager/**` | legacy/orphan | 不算当前 runtime |
| 5 个 dormant IPC | compiled + registered，非 frontend-reachable | 用途 `UNKNOWN` |

本期明确记录但不修的产品问题：

- production-source 与 compiled/reachable 语义没有机器级分层；
- `science_failure_stage()` 使用字符串推断；
- auto-boot 丢失结构化 DTO；
- doctor 会改变第三方 Skill/connector/prompt 路由状态；
- raw CONNECT 不受 path-secret 保护，且建立后没有 session/idle/byte/并发连接上限；
- 本地 Skill 安装不取得 Lifecycle；第二次 runtime-context 复核后仍可能与
  stop/switch 交错；
- auto-boot 与 Science WebView 的生产 producer 未确认；
- 5 个 registered/no-frontend-caller IPC；
- Plugin 的部分解析路径与统一拒绝表述不一致；
- MCP boundary 与 SSH 三道产品 gate 未闭合。

## 5. Science 能力差距摘要

| 域 | Science owner/source of truth | CSSwitch 当前状态 | 当前缺口 |
|---|---|---|---|
| whole-app macOS/Linux/WSL | Science App/CLI 与部署主机 | macOS 受管；Linux/WSL 无管理入口 | remote Linux/WSL 双端口、preview origin、data-dir 未跑 |
| project/session/artifact/archive | Science local data/UI | 原生保留；CSSwitch只隔离/保护 | current 0.1.25 第三方 live 未跑 |
| plans/delegation/fork | Science session/plan 与 Claude service | 原生保留 | 第三方模型请求、配额和恢复未跑 |
| files/attachments/permissions | Science permission UI/scope/persistence/enforcement；用户授权资源 | 原生保留 | read/write/request/revoke/越界拒绝未跑 |
| annotations/memory/context | Science message/project/session state | 原生保留 | 格式、定位、跨 session、删除与 compaction 未跑 |
| Skills | Science Settings、目录、Agent binding/session | source 配置/安装/attach 部分桥接 | Agent load/tool/poll/uninstall/restart 的 artifact/runtime 分层未闭合 |
| MCP local stdio | Science connector loader | 只桥接一个内部 connector | 通用 fixture 未跑 |
| MCP Remote/Directory/hosted | Science/Anthropic/账号 entitlement | `*.mcp.claude.com` source transport denial；其余不管理 | UI、OAuth、其他 remote/custom MCP 与 live 分别需 B/C |
| Plugins | Science 上游/package surface | CSSwitch不提供 runtime；仅摄取部分 Skill | Science 完整公开 lifecycle UNKNOWN |
| environments/kernels | Science | opaque preserve | Python/R机制有官方合同；Node UNKNOWN；第三方 live 未跑 |
| GPU | Science 与 Linux/GPU 主机 | 无管理入口 | sandbox 改变后的独立安全验收未跑 |
| Web Search/literature | Science/Anthropic、开放库、出版商/机构/用户凭证 | CSSwitch不管理；Anthropic transport 受限 | 开放、账号、机构与付费来源未分层运行 |
| Featured connectors | Science 与具体科研数据服务 | CSSwitch不管理 | 许可、限速、registration 与真实调用未跑 |
| cloud storage | Science、云厂商、用户 bucket/credential | CSSwitch不管理 | fake S3 与真实最小权限 bucket 均未跑 |
| Modal/BioNeMo/endpoints | Science 与外部付费 compute/inference 服务 | CSSwitch不管理 | 账号、审批、预算、取消、凭证与数据边界未跑 |
| Reviewer/Specialist | Science session/plan entitlement | CSSwitch无通用管理 | 不得从 OPERON bridge 外推 |
| SSH | Science host UI + user OpenSSH | opt-in 部分桥接 | parser/invocation/server 三 gate 全未动态闭合 |
| account/model/usage | Anthropic official entitlement；第三方 provider 服务；CSSwitch routing | 官方面不可用；第三方 profile/routing 已桥接 | 官方账号与第三方 provider 不能共享一个 owner/PASS |
| data/admin/compliance | Science/Anthropic、组织 admin、本地主机 | CSSwitch无组织管理面 | telemetry、Admin API、offboarding 与本地残留未跑 |
| updater | Science updater；CSSwitch选取/固化候选 | 部分桥接，运行时 `--no-auto-update` | source-fixed 不等于 final/installed/live |
| sandbox/network/preview/voice | Science env/config/UI/system preference；不同流量面 | CSSwitch注入 env fast-fail proxy与 loopback bypass | app/sandbox/connector/preview/voice/package/updater 未逐面 A/B |

## 6. 版本固定 package / installed-static / evidence 事实

### 6.1 获取包与历史 evidence

以下只能留在 audit/evidence：

- Claude Science `0.1.25` build `b7190511`，build date `2026-07-24T22:38:53Z`；
- standalone arm64 updater SHA-256 `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7`；
- DMG SHA-256 `cdc0642061983c80e371cbb529035ac3dd8d341a4a8dfd04c8de3085e12bd6ce`；
- 0.1.25 App-seeded CLI SHA-256 `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`；
- fixed updater path 已观察到 standalone 与 App-seeded 两种 exact embedded identity；当前 source 接受二者并要求 exact Team ID；
- 官方 changelog 在 2026-07-30 可见的最新公开条目是 `0.1.21`（2026-07-21），不能替代 0.1.25 package-static 证据；
- 旧 full-tree snapshot 的容量、entry count、APFS clone 与故障链只是历史候选证据。

### 6.2 2026-07-30 current installed-static

只读安装态盘点记录：

- `/Applications` 中 App version 为 `0.1.25`，App 与内嵌 CLI 均为 arm64；
- App bundle identifier 为 `com.anthropic.operon`，CLI identifier 为
  `com.anthropic.operon.cli`，最低 macOS 为 `13.0`；
- 内嵌 CLI SHA-256 为
  `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f`，
  与 compatibility evidence 中的 official track 一致；
- `codesign --verify --deep --strict` 在该安装态返回 invalid signature；历史记录
  中两条 manifest-verified official track 也出现同类结果。它既不证明 live
  可用，也不能单独证明包被恶意修改；
- 静态 route/string 表面可见 annotations、artifact lineage/version、
  execution log、memory、plan approval、fork、GPU、Modal、BioNeMo、
  cloud storage、SSH jobs、Skill catalog、marketplace、MCP 与 archive。

以上都是 `CURRENT-INSTALLED-STATIC`。本轮没有启动 App，所以
`target=CURRENT-INSTALLED-LIVE` 对全部用户能力均为 `NOT-RUN`；不能从静态
字符串、identifier、hash 或签名检查升级为入口可见、授权具备或执行成功。

## 7. 官方资料与 0.1.25 crosswalk

以下页面均于 2026-07-30 访问；除明确列出的 changelog/announcement 外，页面
未标文档版本或发布日期。`官方面`只表示 `EXTERNAL-OFFICIAL`，`0.1.25 静态面`
来自获取包或 current installed-static；最后一列明确记录本任务未产生 live PASS。

| 官方资料 | 官方能力面 | 0.1.25 静态面 | current live |
|---|---|---|---|
| [Overview](https://claude.com/docs/claude-science/overview)、[macOS CLI](https://claude.com/docs/claude-science/command-line-settings) | App/CLI、serve/open/url/status/stop/update/import、data-dir、no-auto-update | CLI metadata/routes 可见 | `NOT-RUN` |
| [Windows WSL](https://claude.com/docs/claude-science/run-on-windows-wsl)、[Remote Linux](https://claude.com/docs/claude-science/run-on-remote-linux-server) | whole-app WSL/Linux、web/preview 端口与远程访问 | 当前安装仅确认 macOS arm64 | `NOT-RUN` |
| [Core concepts](https://claude.com/docs/claude-science/core-concepts) | project/session/workspace/kernel、files、permissions、plans/delegation/fork、memory | project/session/permission/plan/fork/memory surface 可见 | `NOT-RUN` |
| [Artifacts](https://claude.com/docs/claude-science/artifacts)、[Annotations](https://claude.com/docs/claude-science/annotations) | artifact version/diff/preview/provenance/deletion；多格式 annotation | artifact lineage/version、execution log、annotation surface 可见 | `NOT-RUN` |
| [Manage on devices](https://claude.com/docs/claude-science/manage-on-devices) | data-dir 与固定 `~/.claude-science` 状态、设备本地残留 | metadata 不证明 offboarding/delete | `NOT-RUN` |
| [Tools and environments](https://claude.com/docs/claude-science/tools-and-environments) | Python/R/shell、starter/task env、kernel/package persistence、compute monitor、GPU | environment/kernel/GPU surface 可见；Node ownership 仍不足 | `NOT-RUN` |
| [Connectors and skills](https://claude.com/docs/claude-science/connectors-and-skills) | Settings、Skill load、Featured/Directory connectors | Skill catalog/marketplace/connector surface 可见 | `NOT-RUN` |
| [Custom connectors](https://claude.com/docs/claude-science/custom-connectors) | Remote SSE/Streamable HTTP、OAuth/header、Local command/env | MCP route/string surface 可见 | `NOT-RUN` |
| [Network requirements](https://claude.com/docs/claude-science/network-requirements)、[Corporate networks](https://claude.com/docs/claude-science/corporate-networks) | app、sandbox、hosted connector、Web Search、proxy/TLS/CA 与 voice/WebSocket 网络要求 | `*.mcp.claude.com` 目标存在；CSSwitch source 会命中 `claude.com` CONNECT denial | `NOT-RUN` |
| [Configuration reference](https://claude.com/docs/claude-science/configuration-file-reference) | env/config/UI/system proxy precedence、package/mirror/sandbox 设置 | network preference surface 可见 | `NOT-RUN` |
| [Literature access](https://claude.com/docs/claude-science/literature-access) | 开放数据库、出版商、机构代理与 paywall 边界 | literature-related surface 不能证明任一来源可用 | `NOT-RUN` |
| [Cloud storage](https://claude.com/docs/claude-science/cloud-storage) | S3/GCS/Azure/S3-compatible、凭证与导入 | cloud storage surface 可见 | `NOT-RUN` |
| [Remote compute clusters](https://claude.com/docs/claude-science/remote-compute-clusters) | `~/.ssh/config`、key/agent、ProxyJump、scheduler 与远端执行 | SSH job/host surface 可见 | `NOT-RUN` |
| [Compute providers](https://claude.com/docs/claude-science/compute-providers) | Modal job 与 BioNeMo/inference endpoint | Modal/BioNeMo surface 可见 | `NOT-RUN` |
| [The reviewer](https://claude.com/docs/claude-science/the-reviewer) | Reviewer、Specialists、输入边界与 plan availability | reviewer/specialist surface 可见 | `NOT-RUN` |
| [Enable Science](https://claude.com/docs/claude-science/enable-claude-science)、[Monitor usage](https://claude.com/docs/claude-science/monitor-usage) | 账号、官方模型、订阅、配额与 usage | 虚拟登录不能产生官方 entitlement | `NOT-RUN` |
| [Data handling](https://claude.com/docs/claude-science/how-claude-science-works-with-your-data)、[Admin controls](https://claude.com/docs/claude-science/admin-controls)、[Unavailable controls](https://claude.com/docs/claude-science/whats-not-available-yet) | 本地/服务端数据、telemetry、analytics/Admin API、组织策略、compliance/offboarding 限制 | admin/plugin/marketplace surface 不证明组织 entitlement | `NOT-RUN` |
| [Plugins overview](https://claude.com/docs/plugins/overview) | 通用 Plugin 组成与公开 availability | 0.1.25 有 plugin/marketplace/stage/apply/revoke/update 信号 | `NOT-RUN`；不可外推为 Science 完整合同 |
| [Claude Science changelog](https://claude.com/docs/claude-science/changelog)、[announcement](https://www.anthropic.com/news/claude-science-ai-workbench) | 公开发布日期与变更时间线 | 公开 changelog 最新只到 0.1.21，不能覆盖 0.1.25 | `NOT-RUN` |

## 8. 文档缺口与 `UNKNOWN`

### 8.1 调研时文档缺口及治理落点

| 调研时缺口 | 本期治理落点 |
|---|---|
| capability map 只覆盖 CSSwitch 集成面，遗漏 annotations、memory、cloud、literature、Web Search、GPU、Modal、BioNeMo、平台与 admin/compliance | [产品能力地图](../features/product-science-capability-map.md#claude-science-产品能力全景)补齐用户能力域 |
| hosted/Directory connector 只有笼统 `UNKNOWN` | 产品与 architecture 分别记录 `*.mcp.claude.com` source-level transport denial，并保留 UI/entitlement/OAuth/live `NOT-RUN / UNKNOWN` |
| permission system、用户授权和资源 owner 混写 | [能力依赖](../architecture/science-capability-dependencies.md#所有权模型)拆分 Science enforcement 与用户授权决定 |
| 官方 Claude、第三方 provider 与 CSSwitch routing owner 混写 | 能力依赖拆分 Anthropic entitlement、provider service 与 CSSwitch profile/selector/routing |
| whole-app Linux/WSL 与 SSH remote compute 混写 | 产品与 architecture 分成独立平台部署和 remote compute |
| updater 被误解为 Anthropic CONNECT blocklist 结果 | 固化 `--no-auto-update` 禁用；update host 本身不在该 blocklist |
| official、package、installed-static、historical、current live 混层 | 本 audit 增加 `CURRENT-INSTALLED-STATIC`，并将 current live 统一标为 `NOT-RUN` |
| 缺少官方资料与 0.1.25 的逐域 crosswalk | 本 audit 第 7 节登记；不把 official/package surface 升级为 live |

### 8.2 仍未闭合的 `UNKNOWN`

- 精确 v0.8.4 final artifact 与 Science 0.1.25 current installed live；
- 5 个 dormant registered IPC 的产品去留；
- 普通安装是否设置 auto-boot / Science WebView 环境入口；
- 外部 GitHub Skill 的 Agent load、tool 调用、poll、卸载和 restart artifact/runtime；
- project/files/artifacts/annotations/permission/plans/memory 的 0.1.25 第三方 live；
- Science Plugin 的完整 manifest、UI、hooks、权限、启停和更新生命周期；
- Node 环境的 Science ownership/scope；GPU 独立安全模式；
- whole-app Linux/WSL 的双端口、preview origin、data-dir 与 import/rollback；
- Reviewer/Specialist 在第三方 provider 下的模型请求与实际可用性；
- hosted MCP、Directory connector、catalog/marketplace 的第三方 entitlement；
- Web Search、literature、cloud、Modal、BioNeMo、Featured connector 的网络、账号、费用与数据边界；
- telemetry、Admin API、organization control、offboarding 与设备本地残留；
- network preference 对 Gateway、connector、preview、voice、package 与 updater 各流量面的影响；
- SSH parser、wrapper invocation 与真实 server 三道动态结果。

## 9. 后续探针队列

本节只保存 2026-07-30 调研形成时的队列来源，不是执行合同。当前唯一的 fixture、
授权、gate、判定、停止、清理与证据输出规格见
[Claude Science 0.1.25 探针规格](../operations/science-probe-spec.md)。本期只设计，
不执行。

### A｜静态 / package

1. `A-IPC-01`：为注册命令生成 compiled/registered/caller 静态清单，决定 5 个 dormant IPC 去留。
2. `A-PLUGIN-01`：固定 Science package hash，核对 Plugin schema/UI/importer 与 CSSwitch root-Skill/Plugin candidate 路径矩阵。
3. `A-SSH-01`：把 wrapper exact content/hash/owner/mode/nlink 的期望与当前 validator 事实分开。
4. `A-EVIDENCE-01`：在未来 evidence 中统一使用九层词表，禁止 source/package/installed/live/release 互相升级。

### B｜隔离 HOME + fixture/mock

1. `B-RUNTIME-01`：精确 final artifact、临时 HOME/data-dir、动态端口、local mock，闭合 start/reopen/stop/restart identity。
2. `B-CORE-01`：project/files/artifact/annotation/permission；read/write/request/revoke 与 lineage 分 gate。
3. `B-CONTEXT-01`：plans/delegation/fork/memory/reviewer 只测本地 surface 和请求形态，不外推账号服务成功。
4. `B-SKILL-01`：GitHub install→attach→Agent load→tool/poll→uninstall→restart 分 gate。
5. `B-MCP-01`：无网络 local stdio echo MCP；工具发现、权限、调用和 restart。
6. `B-MCP-02`：loopback Remote SSE/Streamable HTTP 与 network preference A/B。
7. `B-ENV-01`：Python/R/shell/Node、named env、package persistence 与 compute monitor。
8. `B-PLATFORM-01`：专用可丢弃 Linux/WSL，验证双端口、preview origin、data-dir 与 CLI import；不复用用户数据。
9. `B-DATA-01`：loopback fake S3-compatible 与无凭证 Featured registration fixture，不声称真实服务调用。
10. `B-SSH-01`：只测 Science parser acceptance。
11. `B-SSH-02`：无网络 recorder 证明 wrapper、`/usr/bin/ssh -F`、Include 和 env；不连 server。
12. `B-PLUGIN-01`：仅在 A 找到明确 Science 原生入口后使用无害组合 fixture。

### C｜另行明确授权

1. `C-PUBLIC-01`：开放文献或 Featured connector 的最小公开网络请求；需要凭证、付费或非预期上传即停止。
2. `C-ACCOUNT-01`：专用测试账号逐项验证 Web Search、catalog、Directory、hosted MCP、Plugin、Reviewer entitlement。
3. `C-ORG-01`：测试组织和专用设备验证 analytics/Admin API、telemetry、offboarding 与本地残留。
4. `C-EXTERNAL-01`：Modal、BioNeMo、真实 cloud、付费文献、GPU 各自单独授权预算、最小权限凭证、取消方式和停止条件。
5. `C-SSH-01`：真实 server，单独记录 parser、host key、auth、network 与远端命令。
6. `C-PROVIDER-01`：真实 provider/model/配额，禁止由 mock 或官方账号结果外推。

下一阶段只冻结每项探针的 fixture、授权、预期判定、证据层与停止条件；本 audit
没有运行任何探针。全局停止边界是：触及真实 HOME、Keychain、账号状态、保留端口
`8765`、真实 SSH key/agent、未授权 destination、非预期外部 egress 或凭证读取时
立即停止，不能把失败自动改写为新的网络或账号尝试。

## 10. 源码复核入口

| 边界 | 路径 / 符号 |
|---|---|
| module graph / AppState / invoke / events | `desktop/src-tauri/src/lib.rs::{AppState, run, mark_boot_failed, boot_result_error}` |
| conditional launch / Science surface | `desktop/src-tauri/src/lib.rs::decide_launch`、`runtime/sandbox_session.rs::open_science_surface` |
| frontend caller/listener | `desktop/src/main.js::{call, runOneClick}` 与 event listener |
| doctor reconcile | `commands/diagnostics.rs::run_doctor`、`runtime/sandbox_session.rs::force_third_party_reconcile`、`desktop/gateway/src/science_control.rs` |
| config/journal | `desktop/src-tauri/src/config.rs::{Config, RuntimeTransactionJournal, RuntimeBindingCommit}` |
| lock order | `desktop/src-tauri/src/lifecycle.rs::Lifecycle` |
| trace stages | `desktop/src-tauri/src/runtime/operation.rs::{OperationKind, OperationStage, OperationTrace}` |
| one-click / snapshot / receipt | `desktop/src-tauri/src/runtime/sandbox_session.rs`、`runtime/science.rs` |
| failure projection | `desktop/src-tauri/src/commands/runtime.rs::{one_click_login, one_click_login_cmd, science_failure_stage}` |
| Gateway modes | `desktop/gateway/src/main.rs`、`desktop/src-tauri/src/scratch.rs`、`runtime/proxy_lifecycle.rs` |
| Skill / Science control | `desktop/gateway/src/{skill_install.rs,science_control.rs}` |
| protected / opaque roots | `runtime/sandbox_session.rs::{SCIENCE_PROTECTED_AUTHORITY_ENTRIES, SCIENCE_OWNED_OPAQUE_ROOTS}` |
| SSH | `runtime/{settings.rs,ssh_bridge.rs,sandbox_session.rs}`、`scripts/launch-virtual-sandbox.sh`、`scripts/ssh-bridge/ssh` |
| Plugin subset parsing | `desktop/skill-package/src/archive.rs::{package_or_bundle_from_local_archive, plugin_skill_candidates}` |
