# CSSwitch ↔ Claude Science 关联机制与合理设计

状态：日期化研究结论（source 层）

适用范围：源码基线 `origin/main@37d5cfb`（v0.8.4）；对照私有能力/托管地图
`docs/features/product-science-capability-map.md` 与
`docs/architecture/science-capability-dependencies.md`。

最后复核：2026-07-30

失效条件：一键事务、runtime 选择、虚拟登录、Gateway 契约或隔离 HOME/data-dir
布局发生结构性变化时重读源码并修订本文；本文不是 live 认证。

本文回答三个问题：两者实际如何关联（含沙箱）、沙箱角色与限度、合理设计应
如何。不新增能力矩阵，不执行探针，不改生产代码。

## 1. 一句话

CSSwitch 是 **编排器 + 模型旁路适配器**；Science 是 **被隔离启动的外部产品本体**。
两者的硬耦合点很少，但被包在一条很长的 **一键事务** 里，使边界在实现上显得缠绕。

## 2. 现状关联图（source 事实）

```text
用户 / Desktop UI
  -> Tauri commands/runtime（一键开始等）
     -> Lifecycle 串行化
     -> resolve profile / launch plan
     -> 选择 Science executable（science.rs identity）
     -> 可选：停止旧 managed Science + protected authority snapshot
     -> oauth_forge：隔离 HOME 内虚拟登录（非真实 Claude OAuth）
     -> 可选：SSH stub / Skill install 前置
     -> 启动正式 Gateway（loopback + path secret）
     -> scripts/launch-virtual-sandbox.sh
          HOME=SANDBOX_HOME（~/.csswitch/sandbox/home）
          data-dir=$HOME/.claude-science
          SCIENCE_BIN=已选定 executable
          ANTHROPIC_BASE_URL=http://127.0.0.1:<proxy>/<secret>
          claude-science serve --host 127.0.0.1 --port <sandbox>
            --sandbox-port <preview> --no-auto-update --detached
     -> health + listener/data-dir/managed-launch 身份对齐
     -> DB reverify / open surface

推理路径（第三方模式核心）：
  Science UI/Agent
    -> ANTHROPIC_BASE_URL（含 path secret）
    -> CSSwitch Gateway（协议/模型路由）
    -> 第三方 provider

非模型外部资源（检索/hosted/云/通用 MCP…）：
  语义归 Science / 账号 / 外部服务；
  当前 socket 可能继承 CSSwitch 注入的 HTTPS_PROXY 并经 Gateway raw CONNECT
  出站，但 CSSwitch 不因此拥有这些产品语义
```

### 2.1 控制平面（谁启动谁）

| 触点 | 位置（代表） | 事实 |
|---|---|---|
| 一键事务 | `runtime/sandbox_session.rs` `one_click_login_with_options` | 编排 profile、停旧实例、authority、登录、Gateway、起沙箱、探活、receipt |
| Runtime 选择 | `runtime/science.rs` | explicit / official_updated snapshot / App / cached_once |
| 虚拟登录 | `oauth_forge.rs` | 写入隔离 `…/sandbox/home/.claude-science`，禁止落到真实 HOME |
| 启动脚本 | `scripts/launch-virtual-sandbox.sh` | 铁律：非 8765、非真实 data-dir、注入 PROXY_URL、serve |
| Managed receipt | `science.rs` managed launch record | 复用/停止/恢复的身份钉 |

### 2.2 数据平面（什么被隔离）

| 路径 | 含义 |
|---|---|
| `~/.csswitch/sandbox/home` | 隔离 **HOME**（脚本与 keychain 作用域） |
| `…/home/.claude-science` | 隔离 **data-dir**（= Science 在该 HOME 下的默认态） |
| 真实 `~/.claude-science` | **禁止**读写作为数据源；仅可能作为 updater binary 候选探测 |
| protected snapshot | OAuth/orgs/config/mcp/ssh bridge 等 CSSwitch 可能改写的投影 |
| opaque roots | `conda/runtime/seed-assets/r-libs/sbx-bind-src`：只绑 inode，不递归管理 |
| process environment | 当前 Rust/脚本启动链会继承 parent environment；这不是已闭合的隔离合同 |

### 2.3 请求平面（模型如何拐弯）

- Gateway 先起，生成/复用 **持久 path secret**（换 secret 会让已烘进 Science 的 BASE_URL 失效，故持久化）。
- `CSSWITCH_PROXY_URL` / `ANTHROPIC_BASE_URL` = `http://127.0.0.1:<proxy_port>/<secret>`。
- Science 以为自己在打 Anthropic-compatible 端点；Gateway 按 profile 转到 DeepSeek/Qwen/relay/custom/Codex 等。
- **虚拟登录 ≠ 官方 Claude 能力**；不产生官方 catalog/usage/entitlement。
- 当前启动还会注入 `HTTPS_PROXY` / `NO_PROXY`。HTTPS socket 可通过 Gateway 的
  raw `CONNECT` 转发，但这只是 transport，不把 connector、文献、云或 hosted
  服务变成 CSSwitch 托管能力。

### 2.4 「沙箱」一词在现状里叠了三层

| 口语「沙箱」 | 实现含义 | CSSwitch 角色 |
|---|---|---|
| **隔离环境** | 独立 HOME + data-dir + 非 8765 端口 + 隔离 keychain | **必须托管** |
| **sandbox_port** | Science UI/daemon 的 loopback 端口（配置项） | 端口与 identity 管理 |
| **Science `--sandbox-port`** | 预览等 Science 自有 sandbox 端口（UI port+1） | 只分配/冲突检查，不拥有语义 |

问题根源之一：**把「隔离」听成「断网/功能裁剪」**，或把 **Science 自带 sandbox 计算** 与 **CSSwitch 隔离 HOME** 混谈。

### 2.5 可选窄桥（挂在事务两侧，不是主干）

- 外部 Skill：安装/投影/attach 前置 → Science 负责 load/执行
- SSH：preflight + stub Include → OpenSSH/用户 key/远端
- Codex：独立 network route + auth proof，默认收紧

### 2.6 失败投影（当前不合理点）

- 事务内部有 `OperationStage` 等结构化阶段。
- UI 失败 stage 仍经 `commands/runtime.rs::science_failure_stage` **对中文/英文错误串 contains** 归类。
- 同一失败在 journal / operation log / frontend stage 粒度不一致 → 难定位、文案改动会改分类。

## 3. 沙箱/隔离：角色与限度

### 3.1 合理目的（应坚持）

1. **账号与数据边界**：第三方模式不碰真实 Claude Science 登录态与用户 HOME 数据。
2. **进程身份边界**：知道「正在管的是哪一个 binary + data-dir + port + receipt」。
3. **模型旁路边界**：把 Anthropic-shaped 请求接到用户配置的第三方通路。
4. **可补偿边界**：只对 CSSwitch 改写过的 protected 投影做 snapshot/rollback；opaque 环境不装成可重建缓存。

### 3.2 隔离不保证（常被误判）

- 不保证断网；Science 仍可出站做检索/connector/云等（owner 在 Science/账号/外部服务）。
- 不保证所有非模型流量绕开 CSSwitch；当前 HTTPS transport 可能经 raw
  `CONNECT`，故必须把 socket 路径与能力 ownership 分开。
- 不保证官方模型、Web Search、hosted MCP、Reviewer entitlement。
- 不保证 SSH/远端计算成功（多层 owner）。
- 不保证 project/artifact/permission/kernel 的产品语义正确（Science 本体）。
- 端口 HTTP health ≠ runtime identity。
- path secret 隧道 / CONNECT ≠ MCP 产品能力。

### 3.3 与外部资源的真实关系

隔离解决的是 **「在谁的 HOME/身份下跑」**；
外部资源解决的是 **「Science 进程连谁」**。

二者正交。合理产品叙事应是：

> 沙箱让你安全地跑「自己的 Science 实例 + 自己的模型通路」；
> 不负责把 Science 变成离线 IDE，也不负责代理全部官方云能力。

## 4. 合理设计（应然）

### 4.1 目标耦合模型：窄契约 adapter

CSSwitch **应只持有** 下列稳定契约：

1. **RuntimeSelection**：选哪个 executable、指纹、来源枚举
2. **IsolationLayout**：sandbox HOME、data-dir、ports、禁止真实 8765/真实 data-dir
3. **VirtualAuthProjection**：隔离内虚拟登录形态（非 Anthropic 账号）
4. **LaunchEnvironment**：两级 allowlist；Runtime 只收运行必需变量，provider
   secret 只进入 Gateway，bridge 变量逐项 opt-in
5. **GatewayEndpoint**：loopback URL + path secret + provider launch plan
6. **ManagedIdentity**：launch receipt / listener / data-dir 对齐规则
7. **ProtectedProjection**：可快照/补偿的显式名单 + opaque 不递归
8. **OptionalBridges**：Skill / SSH / Codex 的显式、默窄、可关闭入口

Science **应继续拥有**：UI、project/session、权限、artifact、环境/kernel、Agent、Skill 执行、通用 MCP/Plugin、官方服务客户端。

### 4.2 反模式（应逐步消灭）

| 反模式 | 现状痕迹 | 目标 |
|---|---|---|
| 超长一键上帝函数 | `one_click_login_with_options` 嵌在 ~6k 行生产主体文件 | 阶段模块 + 统一 rollback API |
| 字符串反推 UI stage | `science_failure_stage` | typed failure → 现有 DTO |
| 多套 stage 词汇 | OperationStage / journal.string / frontend stage | 单一内部投影 |
| 把 opaque 环境当 CSSwitch 状态 | 历史 full-tree 教训；现已 protected 化 | 保持并写进设计原则 |
| 用 Gateway 成败解释一切网络 | 用户认知 | 分面：model vs 其它出站 |
| 隔离 HOME 却继承 ambient env | Rust/脚本 parent env | Runtime/Gateway 两级 allowlist + sentinel 泄漏测试 |
| 能力表当永恒法 | — | 当前基线合同，版本触发修订 |

### 4.3 与「会变」的关系

- **应变的**：Science CLI 旗标、identity 枚举、data-dir 布局、受保护条目列表、桥接口。
- **应力求稳定的**：隔离目的、禁止真实 HOME、模型旁路契约、opaque 不递归、窄桥不升级为领域 ownership。
- 能力/托管表与架构正文是 **当前稳定权威**；本文只是绑定
  `origin/main@37d5cfb` 的日期化 source 解释。Science/CSSwitch 结构性变更后先
  更新调查，再把稳定结论晋升到权威正文。

## 5. 差距与优先序（衔接下一步工程）

对照公开重构优先级报告与上文：

| 优先级 | 动作 | 为何服务最终目标 |
|---|---|---|
| P0 前置 | 收紧 ambient environment：Runtime/Gateway 两级 allowlist + sentinel 测试 | 隔离与凭证边界；在机械搬代码前冻结 |
| P0 | Typed failure projection（去掉字符串 stage） | 排障、可维护 |
| P0 | 拆 `sandbox_session` 一键事务（保持锁序与补偿） | 去巨石、可扩展、对齐窄契约模块 |
| P0 | 拆 Gateway `server.rs` HTTP / dispatch / Skill | 模型通路可扩展 |
| 已完成 | 把 ownership/stage/transport/env 原则收进能力地图、依赖与 runtime 正文 | AI/人默认读懂关联 |
| 不做 | 重开并行文档治理、探针百科、第二张能力矩阵 | 避免权威漂移 |

能力/托管地图（已完成）继续约束 **产品边界**；
本文约束 **连接方式与设计方向**；
代码 P0 落实 **可维护的实现形状**。

## 6. 证据入口（便于复核）

- `desktop/src-tauri/src/runtime/sandbox_session.rs` — 一键事务与 launch env
- `desktop/src-tauri/src/runtime/science.rs` — HOME/data-dir、identity、managed launch
- `desktop/src-tauri/src/oauth_forge.rs` — 虚拟登录与真实路径护栏
- `scripts/launch-virtual-sandbox.sh` — serve 参数与 `ANTHROPIC_BASE_URL`
- `desktop/src-tauri/src/commands/runtime.rs` — `science_failure_stage`
- 当前合同：`docs/features/product-science-capability-map.md`
- 当前合同：`docs/architecture/science-capability-dependencies.md`、
  `science-runtime.md`、`gateway-provider-routing.md`
- 日期化证据：`docs/evidence/investigations/2026-07-30-csswitch-refactor-priorities.md`

## 7. 非结论

- 未跑 live Science / 真实 provider / 已安装 App 全链路。
- 未宣称 final artifact 或公证。
- 未修改生产代码。
