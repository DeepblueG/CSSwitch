# CSSwitch 0.8.4 升级与回滚 / Upgrade and rollback

本说明适用于 macOS Apple Silicon 的 CSSwitch 0.8.4。0.8.4 继续使用 v4 配置
schema，复用 Science 持久化 data-dir 与外部 Skill bridge；从 v1/v2/v3 首次升级
时执行原子迁移并保留不可覆盖备份。现有 API provider、active profile、Codex
配置、用户 MCP 配置和未知字段保持不变。

This guide applies to CSSwitch 0.8.4 for macOS Apple Silicon. Version 0.8.4
keeps schema v4 and reuses Science's persistent data directory and external
Skill bridge. A first upgrade from v1/v2/v3 performs the atomic migration and
preserves non-overwriting backups. Existing API providers, the active profile,
Codex configuration, user MCP entries, and unknown fields remain intact.

## 升级前 / Before upgrading

1. 在 CSSwitch 中停止当前第三方链路，然后退出 CSSwitch。
2. 备份整个 `~/.csswitch/`，包括配置、日志和 Skill Manager store/inventory。
3. 不要删除 `~/.csswitch/sandbox/`。覆盖安装 app 不应删除该目录，但手工删除会影响隔离 Science 状态与历史数据。
4. 确认下载文件名和目标版本是 `CSSwitch_0.8.4_aarch64.dmg` / `0.8.4`，并按
   [v0.8.4 release evidence](../evidence/releases/v0.8.4.md)核对 SHA-256。

1. Stop the active third-party path in CSSwitch, then quit CSSwitch.
2. Back up all of `~/.csswitch/`, including configuration, logs, and Skill Manager store/inventory.
3. Do not delete `~/.csswitch/sandbox/`. Replacing the app should not remove it, but manual deletion can remove isolated Science state and history.
4. Confirm that the download and target version are
   `CSSwitch_0.8.4_aarch64.dmg` / `0.8.4`, then verify its SHA-256 against the
   [v0.8.4 release evidence](../evidence/releases/v0.8.4.md).

## 覆盖安装 / In-place install

1. 打开 DMG，把 CSSwitch 拖入「应用程序」并选择替换旧版。
2. 首次打开如果被 macOS 阻止，在 Finder 中右键 CSSwitch，选择「打开」。0.8.4
   的当前公开附件是经过完整性验证的 ad-hoc seal，没有 Developer ID、公证、
   stapled ticket 或 Gatekeeper acceptance；ad-hoc 不等于这些分发证据。
3. 打开 CSSwitch，确认已有 profile 仍存在，再执行一次「设为当前」。
4. 先用最小请求验证常用 provider，再恢复日常工作。

1. Open the DMG, drag CSSwitch into Applications, and replace the older copy.
2. If macOS blocks the first launch, right-click CSSwitch in Finder and choose
   “Open.” The current 0.8.4 public artifact has a verified ad-hoc seal, but no
   Developer ID, notarization, stapled ticket, or Gatekeeper acceptance.
3. Open CSSwitch, confirm that existing profiles remain, then run “Set active” once.
4. Send one minimal request through your usual provider before resuming normal work.

## 回滚 / Rollback

0.8.4、0.8.3、0.8.2、0.8.1 与 0.8.0 都使用 v4 配置 schema；停止全部
CSSwitch / Science 进程后，在这些版本间回滚不需要 schema 降级，但仍须先备份并
核对目标版本的 release-specific runtime/bugfix 边界。0.7.0 只能读取到 v3，因此
回滚到 0.7.0 或更早版本前，应先在 0.8.4「高级设置」中使用“导出并降级到 v2”，
或在所有进程退出后恢复兼容的版本备份。随后再用旧版 `.dmg` 覆盖
`/Applications/CSSwitch.app`。不要同时运行两个版本，也不要把新 sidecar 单独复制
进旧版 app。回滚不会删除 Science data-dir、已安装 Skill、bundle manifest、隔离
回收内容或旧 Skill Manager 数据。

Versions 0.8.4 through 0.8.0 use schema v4, so rolling back among them after
stopping every CSSwitch and Science process does not require a schema downgrade;
still back up first and review the target release's runtime and bug-fix boundary.
Version 0.7.0 can read only through v3. Before rolling back to 0.7.0 or earlier,
use “Export and downgrade to v2” under 0.8.4 Advanced Settings, or restore a
compatible backup after every CSSwitch process has stopped. Then replace
`/Applications/CSSwitch.app` with the older DMG. Do not run two versions at once
or copy a newer sidecar into an older app. Rollback does not delete the Science
data directory, installed Skills, bundle manifests, quarantined content, or
legacy Skill Manager data.

回滚只替换应用程序，不自动回退或删除 `~/.csswitch` 数据。若旧版无法读取升级后的配置，请退出旧版，把备份的 `config.json` 恢复到原位并保持文件权限为 `0600`。不要在 CSSwitch 或 Science 运行时修改配置文件。

Rollback replaces only the app; it does not automatically revert or delete `~/.csswitch` data. If the older app cannot read the post-upgrade config, quit it, restore the backed-up `config.json`, and keep permissions at `0600`. Do not edit the config while CSSwitch or Science is running.

### Codex 配置降级 / Downgrading Codex configuration

0.7.0 及更早版本无法表达 Codex profile、Codex network route 或 v4 多模型目录。
0.8.4 在「高级设置」提供“导出并降级到 v2”：它先预览全部 Codex profile 和无法
由 v2 表达的模型目录元数据，要求用户在四秒内再次点击确认，再选择导出文件；后端
为**每一个**当前 Codex profile 应用 `export_then_remove`，停止全部受管链路，先
原子导出再提交 v2，在同一配置锁内设置进程终态 latch，随后直接退出。latch 后所有
配置读取、写入与状态轮询均失败关闭，不能触发常规 v2 → v4 自动迁移；终态退出也
不再走会重新读取配置的通用 stop 路径。如果当前生效项是 Codex，降级后的
`active_id` 为空。导出只含 profile 和模型目录元数据，不含 token、账号 ID、
credential payload、模型缓存或代理 URL。降级不会读取、注销或删除 CSSwitch 私有
OAuth 文件；如要删除该凭据，应在降级前另行点击“退出登录”。API-key profiles、
端口和 v2 可表达设置保持不变；Codex network、额外模型和 role bindings 按确认
结果降级。取消文件选择、profile 列表在确认后变化或停止/导出失败时不提交 v2；
若导出成功而随后 config 提交失败，安全结果是“原 v4 + 已完成导出”，可重新执行。

Version 0.7.0 and earlier cannot represent Codex profiles, the Codex network
route, or v4 multi-model catalogs. Version 0.8.4 exposes “Export and downgrade
to v2” under Advanced Settings: it previews every Codex profile and catalog
metadata that v2 cannot represent, requires a second click within four seconds,
then asks for an export destination. The backend applies `export_then_remove`
to **every** current Codex profile, stops all managed paths, atomically exports
before committing v2, sets a process-terminal latch under the same config lock,
then exits directly. After the latch, every config read/write and status poll
fails closed and cannot trigger the normal v2-to-v4 migration; terminal exit
also bypasses the generic stop path that may reload config. If Codex is active,
the downgraded `active_id` is empty. Exports contain profile and model-catalog
metadata only—never tokens, account IDs, credential payloads, model caches, or
proxy URLs. Downgrade neither reads nor logs out nor deletes the CSSwitch
private OAuth files; use the separate Logout action first if removal is
intended. API-key profiles, ports, and all v2-expressible settings remain
intact; Codex network fields, additional models, and role bindings are
downgraded only through the confirmed plan. Cancelling the picker, a changed
profile set, or stop/export failure does not commit v2; if export succeeds but
the later config commit fails, the safe result is the original v4 config plus
a completed export, and the operation can be retried.

## 证据边界 / Evidence boundary

本说明描述安全操作步骤，不证明某个具体下载附件已经通过 hash、签名、公证、Gatekeeper、真实账号或 live provider 验证。每个发布附件都应在对应 release evidence 中单独记录。

This guide describes safe operational steps. It does not prove that a particular download passed hash, signing, notarization, Gatekeeper, real-account, or live-provider verification. Each release artifact needs its own release evidence.
