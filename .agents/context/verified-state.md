# 已验证状态快照

状态：当前；只汇总已绑定的 v0.8.4 分层事实

最后复核：2026-07-30（Asia/Shanghai）

失效条件：release source、最终 DMG、安装 app、公开附件或维护基线任一变化时，
对应层立即失效；未受影响层仍按其 exact identity 判断。

| 层 | 当前可声明的 v0.8.4 事实 |
|---|---|
| Source / unit | exact release source 的 trusted `GATE-SOURCE` completion seal `PASS`；run `712d9f75cb2d98679dfd64aed5cb1fea` |
| Final artifact | DMG SHA-256 `23471daf…f2b2`（64 hex，与 GitHub digest 一致）；Gateway `4448c15e…57a`（64 hex）。历史 Desktop 串仅 63 hex，**已废止**，本层不再声明 Desktop 二进制 hash |
| Installed | `/Applications/CSSwitch.app` 版本 0.8.4；收尾时只检测到一个 CSSwitch app。因 Desktop hash 废止，**不再**声明“安装 hash 与最终 artifact 记录一致” |
| Signing | strict seal 校验通过；仅 ad-hoc，不是 Developer ID / notarization / Gatekeeper |
| Public | peeled tag 与 release source 一致；公开重下载 hash、镜像校验与根目录白名单通过 |
| Current remote refresh | 2026-07-30 tag/main/Release 元数据仍与上述公开 identity 一致 |

以下仍不是当前 PASS：全部真实 provider/model、真实 SSH server、官方账号 entitlement、
Science 全领域行为、Intel/Windows/WSL、Developer ID/notarization/Gatekeeper。
`B-RUNTIME-01` 因缺少允许的 Science executable 保持
`INCONCLUSIVE(reason=artifact-or-binary-identity)`；它与历史 release evidence
属于不同 probe/环境，不能互相覆盖。

完整证据与不能外推的边界见
[v0.8.4 release evidence](../../docs/evidence/releases/v0.8.4.md)；日期化调查从
[调查索引](../../docs/evidence/investigations/README.md)进入。
