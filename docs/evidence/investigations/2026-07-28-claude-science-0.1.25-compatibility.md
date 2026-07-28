# Claude Science 0.1.20 → 0.1.25 compatibility and updater repair

Date: 2026-07-28 (Asia/Shanghai)

This record separates the standalone updater executable from the executable
seeded inside the DMG App. They have different hashes and embedded identifiers
and are not interchangeable evidence. All dynamic checks in this source phase
used temporary HOME/data directories or the two read-only DMG mounts; no real
Science account, organization, config, Keychain item, or existing Science data
was read.

## Official release identity

| Fact | 0.1.20 | 0.1.25 |
| --- | --- | --- |
| build / `sha8` | `17bca090` | `b7190511` |
| build date | `2026-07-17T01:22:41Z` | `2026-07-24T22:38:53Z` |
| manifest | `operon-releases/17bca090/manifest.json` | `operon-releases/b7190511/manifest.json` |
| arm64 updater SHA-256 | `b806b02f36b46606ce4703c2e2758ae17f0336a41feeeea14f824f93ee1e25f9` | `b0de4c8764c58005738cbcf0d0c111935a2caedb11a05483462be32f5545adb7` |
| arm64 DMG SHA-256 | `eb860b3956a55cfe6b815cc4ca66b88ccf9c8ce2633ddc77c038ab50b7926fba` | `cdc0642061983c80e371cbb529035ac3dd8d341a4a8dfd04c8de3085e12bd6ce` |

The downloaded updater and DMG hashes matched those manifests. Both DMGs also
passed `hdiutil verify`.

## Updater and DMG seed are distinct

| Track | 0.1.20 | 0.1.25 |
| --- | --- | --- |
| standalone size | `118027968` | `118980096` |
| standalone identifier | `com.anthropic.operon` | `com.anthropic.operon` |
| standalone Team ID | `Q6L2SF6YDW` | `Q6L2SF6YDW` |
| DMG-seed CLI SHA-256 | `487784354a6a9f7b40b9ba59515ebe434c20ae1c0f31b727ee514cb1812a894a` | `63b0f57aa3b9588ba9e61433d27c78df788f8fe2c1b51842db107d6697e9c03f` |
| DMG-seed size | `118027984` | `118980112` |
| DMG-seed identifier | `com.anthropic.operon.cli` | `com.anthropic.operon.cli` |
| DMG-seed Team ID | `Q6L2SF6YDW` | `Q6L2SF6YDW` |

Both tracks are arm64 and report their expected public version. The standalone
and seed differ by sixteen bytes and by embedded identifier. Strict
`codesign --verify` failed for both versions and both tracks with `invalid
signature (code or signature have been modified)`. The embedded identifier and
Team ID are therefore only local format/identity guards, not cryptographic
proof of official provenance.

The DMG `Info.plist` changed only the short/build version from `0.1.20` to
`0.1.25`; bundle ID remains `com.anthropic.operon`, minimum macOS remains 13.0,
and the icon hash is unchanged.

## Static compatibility delta

Top-level and per-subcommand help for `serve`, `open`, `url`, `status`, `logs`,
`stop`, `update`, and `import` had no 0.1.20 → 0.1.25 delta. CSSwitch-required
flags remain present: `--data-dir`, `--no-auto-update`, `--no-browser`, and
`--detached`.

Normalized embedded route inventory grew from 324 to 330, with six additions
and no removals:

| Added route | Observed method/schema |
| --- | --- |
| `/api/network/status` | `GET`; optional diagnostic `step` and `cause` query |
| `/api/preferences/auto-switch-on-flag` | `GET`; `PUT {enabled:boolean}` |
| `/api/preferences/conda-mirror/credential` | `PUT` credential; `DELETE` credential |
| `/api/preferences/network-proxy` | `GET`; `PUT {proxy:string|null}` with restart status |
| `/api/projects/:pid/archive` | `POST` |
| `/api/projects/:pid/unarchive` | `POST` |

The 0.1.25 package says its saved network-proxy preference is applied at daemon
startup and exposes redacted status. That is package fact only. Whether the
default/no-setting path preserves CSSwitch's local Gateway routing must still be
proven from installed process connections and target traffic.

CSSwitch dependency markers remain present in both versions:

- `ANTHROPIC_BASE_URL`, `/v1/models`, and `/v1/messages`;
- `POST /api/auth/nonce` and `/api/oauth/operon/client_data`;
- `GET /daemon/update-status`;
- `POST /daemon/check-update` and `POST /daemon/apply-update`, including the
  `x-operon: 1` and same-port origin checks.

These observations establish package compatibility signals, not live request
success.

## Reproduced product defect and repair

Before repair, the official 0.1.25 standalone updater failed the repository's
existing real-updater oracle:

```text
left: None
right: Some(.../.claude-science/bin/claude-science)
```

The cause was `PRODUCT_DEFECT`: CSSwitch required the DMG-seed identifier
`com.anthropic.operon.cli` at the standalone updater path, whose actual
identifier is `com.anthropic.operon`.

The repair gives the updater track its actual exact identifier while retaining
the fixed path, current-user ownership, non-group/world-writable directories
and file, bounded Mach-O size, exact Team ID, same-open copy, SHA-256
content-addressed read-only snapshot, source stability recheck, and snapshot
reverification. A parser regression rejects the DMG-seed identifier, wrong Team
ID, and substring/prefix spoofing at the updater boundary.

After repair:

- official 0.1.20 updater → `official_updated` snapshot: PASS;
- official 0.1.25 updater → `official_updated` snapshot: PASS;
- official 0.1.20 DMG seed → `installed_app`: PASS;
- official 0.1.25 DMG seed → `installed_app`: PASS;
- installed-App selection preserved the temporary data-dir marker;
- local priority, fingerprint, historical replacement, symlink, and unsafe
  candidate units passed.

Inside the restricted command sandbox, the focused Science group produced 26
PASS, one permission failure, and two intentionally ignored real-artifact
tests. The exact failed case passed on the host using only temporary state and
loopback; the full host focused group then passed 27/27 with the two
real-artifact tests still intentionally ignored. The sandbox-only result is
classified `ENVIRONMENT_BLOCK`, not a product failure.

Isolated `status` for both updater versions and the 0.1.25 seed returned
`{"running":false}`; isolated `stop` reported no daemon/lockfile. `update
--check` was not run because this binary invokes absolute `/usr/bin/security`
for system certificate/credential helpers and the source-phase safety contract
forbids reading real Keychain state. It is not used as an updater oracle.

## Evidence boundary

This document proves official package identity, static 0.1.20 → 0.1.25
differences, the old source failure, and repaired source-level selection against
the four official executables. It does not yet prove:

- a fixed production CSSwitch artifact or installed CSSwitch runtime;
- installed Science start/reopen/stop/restart, Gateway routing, or model/API
  behavior;
- any real provider, account, quota, or paid request;
- Developer ID signing, notarization, Gatekeeper acceptance, tag, public
  release, or published attachment.

`BUG-083-SCIENCE-UPDATER` is therefore only
`source-fixed-product-pending`. Any post-commit source fix invalidates later
artifact, installed, and live evidence and requires a full rerun.

## Installed restart defect found after the first candidate

The first installed `0.8.3` candidate started official Science `0.1.25`
successfully in an isolated HOME and stopped it through CSSwitch's own
**Stop All** action. A subsequent **One-click Start** failed during the
transaction snapshot prepare stage:

```text
隔离 authority 单文件超过安全上限 67108864 bytes（阶段：prepare）
```

The exact isolated file was
`.claude-science/conda/pkgs/cache/mambafm8uj7td3z6`, an `85,323,776`-byte
Science-created Conda cache file. The failure was classified
`PRODUCT_DEFECT`: the 64 MiB per-file authority snapshot budget was below a
normal Science `0.1.25` managed-state file.

The repair raises only the per-file authority snapshot limit to 128 MiB. The
16,384-entry limit, 512 MiB total limit, authority-root symlink rejection,
regular-file checks, independent-inode copy, streaming I/O, mode preservation, and
device/inode/size/mtime stability checks remain unchanged. The cache is not
excluded because late-failure rollback promises the exact prior authority
object set and bytes. The existing exact-restore regression now additionally
proves that the observed `85,323,776`-byte size passes and 128 MiB plus one
byte remains fail-closed.

The next installed retry crossed that limit and exposed a second normal
Science runtime object:
`.claude-science/runtime/0.1.25-release/agents/operon/.claude/skills/alphafold2`
is a relative symlink to `../../../../skills/alphafold2`. The old snapshot
walker rejected every symlink, including this package-owned runtime link.
The repair keeps authority *root* symlinks fail-closed and never follows an
internal link. It snapshots an internal symlink as an object using `lstat` and
`read_link`, charges its target bytes to the existing budgets, recreates the
link in the private backup, and rechecks device, inode, size, timestamps, and
target stability. The exact-restore regression now covers a mutated relative
link and separately proves that both a symlinked live authority root and a
private backup root replaced by a symlink are still rejected at the walker's
root `lstat`.

This post-candidate source repair invalidates the earlier source-gate,
artifact, installed, and local-mock evidence. A new clean commit and complete
rerun are required before the installed restart defect can move beyond
`source-fixed-product-pending`.
