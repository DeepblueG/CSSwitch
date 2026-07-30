# Claude Science 能力依赖

本文解释
[产品与 Claude Science 能力地图](../features/product-science-capability-map.md)
中的 ownership、第三方托管边界和故障归属。能力清单、当前证据层和 `UNKNOWN`
只在能力地图维护；本文不复制第二张支持矩阵，也不记录版本、hash、route 或一次性
观察。

## 1. 架构判断顺序

每项能力按同一顺序判断：

1. **谁拥有语义和 source of truth**：Science、CSSwitch、用户、Anthropic
   账号/组织，还是具体第三方服务；
2. **第三方模式是否缺少必要运行包络**：只有隔离、身份、Gateway、routing、
   network policy、状态一致性和诊断等必要层才由 CSSwitch 托管；
3. **CSSwitch 是否只有窄桥接**：安装/attach、SSH preflight、Codex bridge
   等入口不能升级为下游领域能力 ownership；
4. **是否明确 non-target**：不属于 CSSwitch 的管理面不能因为当前
   `UNKNOWN` 就自动变成未来实现任务；
5. **证据到哪一层**：official/source/test/package/fixture 不得外推为
   artifact、installed-live 或真实外部服务。

“Science 官方拥有”不等于“第三方模式 current live 已验证”；“CSSwitch
托管运行包络”也不等于 CSSwitch 拥有其中的 project、artifact、permission、
Skill、kernel 或 Reviewer 语义。

## 2. 第三方模式必须托管的最小闭环

CSSwitch 只需要对下面四类责任形成闭环。

### 2.1 Runtime 包络

- 隔离 HOME 与持久 data-dir；
- executable/runtime identity 选择与固定；
- 受管启动、打开、复用、停止和恢复；
- 只保护 CSSwitch-owned 状态，不递归接管
  `conda/runtime/seed-assets/r-libs/sbx-bind-src` 等 Science-owned opaque
  roots。

Science 仍然拥有 project、session、artifact、environment 和本地数据库语义。
持久 data-dir 不证明 executable 版本，也不证明某项原生能力 current live。

### 2.2 身份与 Gateway

- 本地虚拟登录只为隔离 Science 建立第三方模式入口；
- provider/profile/model selector、协议适配和 routing 由 CSSwitch 管理；
- 第三方 provider 拥有模型、认证、配额、计费和实际服务质量；
- 虚拟登录不产生 Anthropic OAuth、官方 catalog、usage 或 entitlement。

因此“模型请求经过 CSSwitch”不能外推为 Web Search、hosted connector、
Reviewer 或其他 Anthropic 服务已经可用。

### 2.3 Network policy

第三方模式只托管完成 model/Gateway 路径所必需的受限 network policy。Science
网络必须按 app、model、connector、sandbox、preview/voice、package、
remote compute 和 updater 分面判断。

raw TCP `CONNECT` 只证明 tunnel 可能建立，不证明 MCP client、OAuth、tool
discovery 或 tool call；对某一 Anthropic host 的拒绝也不能推广为所有 custom
connector 或所有 Science 网络都不可用。

### 2.4 状态一致性与诊断

CSSwitch 负责自己受管对象的状态、补偿和诊断：runtime、Gateway、route、
provider/profile 以及窄 bridge 的配置状态。诊断只能报告它能观察和拥有的层，
不能替 Science 判断 project、artifact、permission、kernel 或外部服务语义。

## 3. 原生保留而非 CSSwitch 托管

以下能力继续由 Science 管理：

- project、session、conversation、archive/import；
- plans、delegation、fork；
- files、attachments、permission UI/scope/persistence/enforcement；
- artifact、version、preview、provenance；
- annotations、memory、search、context compaction；
- Python/R/Conda、task environment、kernel 和 package lifecycle；
- Skill catalog/discovery、session load、trigger、domain execution；
- generic MCP client/loader；
- Reviewer、Specialist；
- Science updater 与最低支持版本。

CSSwitch 可以隔离、启动、保全或展示这些状态，但不得新增一套平行语义层。
例如：

- Skill discovery/attach 不等于 session load、trigger 或 domain execution；
- persistent data-dir 不等于 project/archive 行为已验证；
- package route/string 不等于 artifact、permission 或 Reviewer live；
- Python/R kernel 可用不等于 GPU 模式可用。

## 4. 窄桥接

### 4.1 外部 Skill

CSSwitch 只拥有安全来源、安装、投影和 CSSwitch-owned package 的
attach/detach 前置链路。Science Agent/session 拥有 load、trigger，Skill
代码和其工具环境拥有 domain execution。

故障必须按阶段切开：

1. discovery；
2. install；
3. attach/detach；
4. session load；
5. trigger/tool execution；
6. restart persistence。

前三层可能属于 CSSwitch bridge；后三层不能仅凭前置成功归因给 CSSwitch。

### 4.2 SSH / HPC / SLURM

SSH 链路有五个 owner：

1. CSSwitch opt-in、preflight、sidecar/stub transaction；
2. Science Host alias parser 与 host record；
3. wrapper 到系统 OpenSSH 的 invocation；
4. 用户 config、key、agent、known_hosts；
5. DNS、network、server、scheduler 与远端命令。

parser acceptance、OpenSSH invocation、real server connectivity 必须分别判断。
CSSwitch 不读取 key，不拥有 server/scheduler，也不能把系统 OpenSSH fixture
写成真实 Science SSH live。

whole-app remote Linux/WSL 是把 App/CLI、web UI、preview、data-dir 与端口部署
到另一平台，不是本机 Science 的 SSH remote compute，两者不能合并。

### 4.3 Codex

Codex bridge 只拥有 browser-only OAuth、模型目录和 Responses 适配的明确入口，
默认关闭且不读取原生 `~/.codex`。Codex 账号、provider 服务和 Science 行为仍由
各自 owner 决定。

## 5. 明确 non-target

CSSwitch 不托管：

- 真实 Claude OAuth、官方模型 catalog、subscription、usage、entitlement；
- 官方 Skill catalog/marketplace/private sources parity；
- 通用 local/remote MCP 管理面；
- Plugin registry/runtime、hooks、权限、启停与更新；
- Featured/Directory/Anthropic-hosted connectors；
- Web Search、机构/付费文献凭证和 paywall；
- S3/GCS/Azure 凭证、bucket policy 和云数据；
- GPU 主机/driver、Modal 账号/预算、BioNeMo 或其他 endpoint 计费；
- Reviewer/Specialist 的替代实现；
- analytics/Admin API、offboarding、compliance 和组织策略；
- 用户 SSH key、远端 server、scheduler 或主机安全状态。

这些项目的 `UNKNOWN` 用来限制结论，不构成 CSSwitch 的默认 backlog。

## 6. 故障归属

| 症状 | 先定位到 | 不应直接归因 |
|---|---|---|
| Science 起不来、复用/停止/恢复异常 | CSSwitch runtime 包络、identity、隔离 HOME/data-dir | project 或模型能力 |
| 第三方模型请求失败 | profile/selector、Gateway、协议、route、provider | Science 所有网络面 |
| project/session/artifact 状态异常 | Science 本地状态；再检查 CSSwitch 是否破坏隔离或恢复边界 | provider routing |
| 权限卡、grant/revoke 或越界访问异常 | Science permission engine、用户授权与资源权限 | CSSwitch 路径投影 |
| Skill 看不见或无法 attach | CSSwitch discovery/install/attach 前置链路 | session load/domain execution |
| Skill 已 attach 但不 load/trigger | Science Agent/session、Skill runtime 和工具环境 | 安装成功本身 |
| MCP 无法调用 | 先区分 Skill-internal stdio、generic stdio、custom remote、hosted | raw CONNECT 或“有 MCP”这个总标签 |
| SSH 失败 | parser、OpenSSH invocation、用户配置、network/server/scheduler 逐层定位 | CSSwitch 单一 owner |
| Web Search/hosted connector/Reviewer 不可用 | Anthropic service、账号/org entitlement、Science client 与 network | 第三方模型 Gateway |
| Python/R/Conda/GPU 异常 | Science environment/kernel；GPU 另查主机与安全模式 | CSSwitch 将 opaque roots 当缓存管理 |
| 更新后行为变化 | 先确定实际 runtime identity，再对照能力 owner 和依赖面 | seed App 版本或静态字符串单独定性 |

## 7. Science 更新影响的最小判断

Science 更新后只回答下面几个问题，不新增一套 gate 或执行流水线：

1. runtime identity、CLI 参数、data-dir 或启动方式是否改变；
2. CSSwitch 必须托管的身份/Gateway/network/state 边界是否改变；
3. 原生能力的 owner 是否改变，还是只增加了 UI/package-static 表面；
4. Skill、MCP、environment、SSH 等窄桥接的接口边界是否改变；
5. 变化当前只在 official/source/package 哪一层，哪些 live 结果仍是
   `UNKNOWN`。

日期化变化留在 audit/evidence；只有稳定 ownership 或托管决策改变时，才更新
能力地图和本文。这样可以快速判断影响，又不会把一次版本调查变成长生命周期
基础设施。

## 8. 文档边界

- 完整能力清单、逐能力 ownership/托管/non-target 结论、当前证据层与
  `UNKNOWN`：只在能力地图的单一决策表维护；
- 本文只维护这些逐项结论的稳定解释规则：跨 owner 依赖、最小托管责任、窄桥接
  与 non-target 类别边界、故障归因；不复制第二套逐能力状态表；
- exact version/hash/package/route/一次观察：留在日期化 audit/evidence；
- 不在这里新增 handoff、probe 执行、GATE-SOURCE、schema、lint 或 lifecycle
  合同；
- source/test/package/fixture 不能写成 artifact、installed-live 或真实服务。
