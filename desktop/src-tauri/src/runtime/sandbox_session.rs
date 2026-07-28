use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{Manager, Runtime};

use crate::runtime::operation::{
    self, OperationKind, OperationStage, OperationTrace, POLL_INTERVAL_MS,
};
use crate::runtime::proxy::ProxyAction;
use crate::runtime::proxy_lifecycle::{
    current_skill_install_bridge_key, ensure_proxy, skill_install_bridge_dir, start_proxy_for,
};
use crate::runtime::science::{
    probe_known_runtime, probe_sandbox_runtime_cached, runtime_identity_is_current,
    sandbox_data_dir, sandbox_home, sandbox_listener_matches_runtime, sandbox_url,
    select_science_runtime_cached, stop_sandbox, stop_sandbox_with_launch_token,
    SandboxScienceState, ScienceManagedLaunchToken, ScienceRuntimeIdentity, ScienceRuntimeSource,
};
use crate::runtime::skill_install_bridge::{
    configure_third_party_after_science_start, inspect_while_science_running,
    invalidate_route_configuration, mark_route_configuration_current,
    register_before_science_start, route_configuration_is_current, RegistrationStatus,
};
use crate::runtime::system::{asset_root, log_path, open_in_browser, open_log, redact, tail_file};
use crate::{
    config, lifecycle, lock, oauth_forge, proc, AppState, HistoryRecoveryChoice,
    HistoryRecoverySession, SharedAppState,
};

fn stop_sandbox_state<R: Runtime>(
    app: &tauri::AppHandle<R>,
    st: &mut AppState,
) -> Result<(), String> {
    let runtime = st.science_runtime.clone();
    let result = stop_sandbox(app, &mut st.sandbox, &mut st.sandbox_url, runtime.as_ref());
    if result.is_ok() {
        st.science_confirmed_stopped = runtime;
        st.science_runtime = None;
    }
    result
}

fn open_science_surface<R: Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
) -> Result<&'static str, String> {
    if std::env::var("CSSWITCH_SCIENCE_WEBVIEW_SPIKE")
        .ok()
        .as_deref()
        == Some("1")
    {
        if let Some(win) = app.get_webview_window("science") {
            let _ = win.close();
        }
        let parsed = url
            .parse()
            .map_err(|e| format!("Science URL 解析失败：{e}"))?;
        match tauri::WebviewWindowBuilder::new(app, "science", tauri::WebviewUrl::External(parsed))
            .title("Claude Science")
            .inner_size(1100.0, 800.0)
            .build()
        {
            Ok(win) => {
                let _ = win.set_focus();
                return Ok("webview");
            }
            Err(_) => {
                // Spike-only path: construction failure falls through to the existing browser surface.
            }
        }
    }
    open_in_browser(url)?;
    Ok("browser")
}

fn installer_status_json(status: &RegistrationStatus) -> Value {
    match status {
        RegistrationStatus::Warning(message) => {
            json!({"status": status.code(), "message": message})
        }
        _ => json!({"status": status.code()}),
    }
}

fn append_installer_note(mut message: String, status: &RegistrationStatus) -> String {
    if let Some(note) = status.user_note() {
        message.push_str(&format!(" {note}"));
    }
    message
}

struct AuthorityTreeSnapshot {
    source: PathBuf,
    backup: PathBuf,
    existed: bool,
}

const MAX_AUTHORITY_SNAPSHOT_ENTRIES: usize = 16_384;
const MAX_AUTHORITY_SNAPSHOT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_AUTHORITY_SNAPSHOT_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(test)]
#[derive(Default)]
struct SandboxSessionTestSeams {
    cleanup_fault: Option<(PathBuf, String, PathBuf)>,
    cleanup_calls: usize,
    capture_fail_source: Option<PathBuf>,
    directory_barrier: Option<(PathBuf, PathBuf)>,
    one_click_capture: Option<(PathBuf, PathBuf, bool, u32, PathBuf)>,
    catalog_failure_port: Option<u16>,
    prior_restart_post_spawn_failure_port: Option<u16>,
    prior_restart_post_spawn_identity: Option<(u32, String)>,
    rollback_diagnostic_canary: Option<String>,
    rollback_diagnostic_snapshot: Option<PathBuf>,
}

#[cfg(test)]
static SANDBOX_SESSION_TEST_SEAMS: std::sync::LazyLock<
    std::sync::Mutex<SandboxSessionTestSeams>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(SandboxSessionTestSeams::default()));

#[cfg(test)]
pub(crate) struct SandboxSessionTestSeamGuard;

#[cfg(test)]
impl Drop for SandboxSessionTestSeamGuard {
    fn drop(&mut self) {
        *SANDBOX_SESSION_TEST_SEAMS
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = SandboxSessionTestSeams::default();
    }
}

#[cfg(test)]
pub(crate) fn test_arm_authority_snapshot_cleanup_fault(
    scope: PathBuf,
    mode: &str,
    log: PathBuf,
) -> SandboxSessionTestSeamGuard {
    let mut seams = SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    seams.cleanup_fault = Some((scope, mode.to_string(), log));
    seams.cleanup_calls = 0;
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_arm_authority_snapshot_capture_failure(
    source: PathBuf,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .capture_fail_source = Some(source);
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_arm_authority_snapshot_directory_barrier(
    source: PathBuf,
    barrier: PathBuf,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .directory_barrier = Some((source, barrier));
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_arm_one_click_snapshot_capture(
    config_dir: PathBuf,
    observation: PathBuf,
    fail: bool,
    expected_prior_pid: u32,
    expected_receipt: PathBuf,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .one_click_capture = Some((
        config_dir,
        observation,
        fail,
        expected_prior_pid,
        expected_receipt,
    ));
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_arm_healthy_reopen_catalog_failure(
    port: u16,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .catalog_failure_port = Some(port);
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_arm_prior_restart_post_spawn_failure(
    port: u16,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .prior_restart_post_spawn_failure_port = Some(port);
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_prior_restart_post_spawn_identity() -> Option<(u32, String)> {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .prior_restart_post_spawn_identity
        .clone()
}

#[cfg(test)]
pub(crate) fn test_arm_rollback_diagnostic_canary(
    canary: &str,
) -> SandboxSessionTestSeamGuard {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .rollback_diagnostic_canary = Some(canary.to_string());
    SandboxSessionTestSeamGuard
}

#[cfg(test)]
pub(crate) fn test_rollback_diagnostic_snapshot() -> Option<PathBuf> {
    SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .rollback_diagnostic_snapshot
        .clone()
}

fn remove_authority_snapshot_root(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    {
        let observation = {
            let mut seams = SANDBOX_SESSION_TEST_SEAMS
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let configured = seams
                .cleanup_fault
                .as_ref()
                .filter(|(scope, _, _)| path.starts_with(scope))
                .cloned();
            configured.map(|(_, mode, log_path)| {
                let attempt = seams.cleanup_calls;
                seams.cleanup_calls += 1;
                let injected = mode == "persistent" || (mode == "once" && attempt == 0);
                (attempt, injected, log_path)
            })
        };
        if let Some((attempt, injected, log_path)) = observation {
            if let Ok(mut log) = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(log_path)
            {
                use std::io::Write;
                let _ = writeln!(
                    log,
                    "{}\t{}\t{}",
                    attempt + 1,
                    if injected { "injected" } else { "real" },
                    path.display()
                );
            }
            if injected {
                return Err(std::io::Error::other(
                    "test-only authority snapshot cleanup failure",
                ));
            }
        }
    }
    std::fs::remove_dir_all(path)
}

fn remove_authority_snapshot_root_with_retry(path: &Path) -> Result<(), String> {
    match remove_authority_snapshot_root(path) {
        Ok(()) => Ok(()),
        Err(first) => remove_authority_snapshot_root(path).map_err(|second| {
            format!(
                "首次清理失败：{first}；重试清理失败：{second}"
            )
        }),
    }
}

const PENDING_CLEANUP_MARKER_FILE: &str = ".csswitch-one-click-rollback.marker";
const MAX_PENDING_CLEANUP_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingCleanupManifest {
    schema_version: u32,
    entries: Vec<PendingCleanupEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingCleanupEntry {
    managed_id: String,
    path: PathBuf,
    device: u64,
    inode: u64,
    marker: String,
}

#[derive(Clone)]
struct AuthorityCleanupContext {
    config_dir: PathBuf,
    expected_snapshot_parent: PathBuf,
    managed_id: String,
    root: PathBuf,
    state: SharedAppState,
}

struct RegisteredAuthorityCleanup {
    manifest_raw: Vec<u8>,
    entry: PendingCleanupEntry,
}

#[derive(Clone)]
struct PendingCleanupClearRetry {
    config_dir: PathBuf,
    manifest_raw: Vec<u8>,
    entry: PendingCleanupEntry,
}

static PENDING_CLEANUP_CLEAR_RETRY: std::sync::LazyLock<
    std::sync::Mutex<Option<PendingCleanupClearRetry>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

enum PendingCleanupTargetState {
    Missing,
    Present(PendingCleanupEntry),
    Unsafe,
}

fn cleanup_required_error(primary: &str, path: &Path, code: &str) -> String {
    format!(
        "{primary}；status=degraded；recovery_status=cleanup_required；recovery_path={}；cleanup_code={code}",
        path.display()
    )
}

fn pending_cleanup_name_is_valid(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(".one-click-rollback-") else {
        return false;
    };
    suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn pending_cleanup_manifest_bytes(
    entries: Vec<PendingCleanupEntry>,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&PendingCleanupManifest {
        schema_version: 1,
        entries,
    })
    .map_err(|_| "cleanup_manifest_encode_failed：无法编码待清理事务清单。".into())
}

fn parse_pending_cleanup_manifest(bytes: &[u8]) -> Result<PendingCleanupManifest, String> {
    if bytes.is_empty() || bytes.len() > MAX_PENDING_CLEANUP_MANIFEST_BYTES {
        return Err(
            "cleanup_manifest_invalid：待清理事务清单大小非法，已在运行前拒绝。".into(),
        );
    }
    let manifest: PendingCleanupManifest = serde_json::from_slice(bytes)
        .map_err(|_| "cleanup_manifest_invalid：待清理事务清单格式非法，已在运行前拒绝。")?;
    if manifest.schema_version != 1 || manifest.entries.len() > 1 {
        return Err(
            "cleanup_manifest_invalid：待清理事务清单版本或条目数量非法，已在运行前拒绝。"
                .into(),
        );
    }
    Ok(manifest)
}

fn read_marker(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "cleanup_identity_invalid：事务快照 marker 不可用。".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() > 256
    {
        return Err("cleanup_identity_invalid：事务快照 marker 身份不安全。".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "cleanup_identity_invalid：无法安全打开事务快照 marker。")?;
    let opened = file
        .metadata()
        .map_err(|_| "cleanup_identity_invalid：无法复核事务快照 marker。")?;
    if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
        return Err("cleanup_identity_changed：事务快照 marker 在读取前发生变化。".into());
    }
    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, 257)
        .read_to_end(&mut bytes)
        .map_err(|_| "cleanup_identity_invalid：无法读取事务快照 marker。")?;
    if bytes.len() > 256 {
        return Err("cleanup_identity_invalid：事务快照 marker 过大。".into());
    }
    String::from_utf8(bytes)
        .map_err(|_| "cleanup_identity_invalid：事务快照 marker 不是 UTF-8。".into())
}

fn inspect_pending_cleanup_target(entry: &PendingCleanupEntry) -> PendingCleanupTargetState {
    let metadata = match std::fs::symlink_metadata(&entry.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return PendingCleanupTargetState::Missing
        }
        Err(_) => return PendingCleanupTargetState::Unsafe,
    };
    let marker = match read_marker(&entry.path.join(PENDING_CLEANUP_MARKER_FILE)) {
        Ok(marker) => marker,
        Err(_) => return PendingCleanupTargetState::Unsafe,
    };
    let marker = marker.strip_suffix('\n').unwrap_or(&marker).to_string();
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return PendingCleanupTargetState::Unsafe;
    }
    PendingCleanupTargetState::Present(PendingCleanupEntry {
        managed_id: entry.managed_id.clone(),
        path: entry.path.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        marker,
    })
}

fn validate_pending_cleanup_entry(
    entry: &PendingCleanupEntry,
    expected_parent: &Path,
) -> Result<PendingCleanupTargetState, String> {
    if !pending_cleanup_name_is_valid(&entry.managed_id)
        || entry.marker != entry.managed_id
        || entry.path.parent() != Some(expected_parent)
        || entry.path.file_name().and_then(|name| name.to_str())
            != Some(entry.managed_id.as_str())
    {
        return Err(
            "cleanup_manifest_invalid：待清理事务清单路径或 managed_id 非法，已在运行前拒绝。"
                .into(),
        );
    }
    match inspect_pending_cleanup_target(entry) {
        PendingCleanupTargetState::Missing => Ok(PendingCleanupTargetState::Missing),
        PendingCleanupTargetState::Present(current)
            if current.device == entry.device
                && current.inode == entry.inode
                && current.marker == entry.marker =>
        {
            Ok(PendingCleanupTargetState::Present(current))
        }
        _ => Err(
            "cleanup_manifest_identity_mismatch：待清理事务快照身份不一致，已在运行前拒绝。"
                .into(),
        ),
    }
}

#[cfg(test)]
fn test_pending_cleanup_identity(
    entry: &PendingCleanupEntry,
) -> config::PendingCleanupIdentity {
    config::PendingCleanupIdentity {
        managed_id: entry.managed_id.clone(),
        path: entry.path.clone(),
        device: entry.device,
        inode: entry.inode,
        marker: entry.marker.clone(),
    }
}

impl AuthorityCleanupContext {
    fn new(
        config_dir: &Path,
        sandbox_home: &Path,
        state: &SharedAppState,
    ) -> Result<Self, String> {
        let expected_snapshot_parent = sandbox_home
            .parent()
            .ok_or("cleanup_register_failed：沙箱 HOME 无父目录。")?
            .to_path_buf();
        let managed_id = format!(".one-click-rollback-{}", config::new_id());
        let root = expected_snapshot_parent.join(&managed_id);
        Ok(Self {
            config_dir: config_dir.to_path_buf(),
            expected_snapshot_parent,
            managed_id,
            root,
            state: state.clone(),
        })
    }

    fn register_error(&self, detail: &str) -> String {
        format!(
            "cleanup_register_failed：{detail}；recovery_path={}",
            self.root.display()
        )
    }
}

fn register_authority_cleanup(
    context: &AuthorityCleanupContext,
) -> Result<RegisteredAuthorityCleanup, String> {
    if context.root.parent() != Some(context.expected_snapshot_parent.as_path())
        || context.root.file_name().and_then(|name| name.to_str())
            != Some(context.managed_id.as_str())
        || !pending_cleanup_name_is_valid(&context.managed_id)
    {
        return Err(context.register_error("事务快照路径不在受管根内。"));
    }
    let before = std::fs::symlink_metadata(&context.root)
        .map_err(|_| context.register_error("事务快照不可用。"))?;
    if before.file_type().is_symlink()
        || !before.is_dir()
        || before.uid() != unsafe { libc::geteuid() }
        || before.permissions().mode() & 0o777 != 0o700
    {
        return Err(context.register_error("事务快照身份不安全。"));
    }
    let marker_path = context.root.join(PENDING_CLEANUP_MARKER_FILE);
    match marker_path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut marker = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&marker_path)
                .map_err(|_| context.register_error("无法创建事务快照 marker。"))?;
            std::io::Write::write_all(
                &mut marker,
                format!("{}\n", context.managed_id).as_bytes(),
            )
                .and_then(|_| marker.sync_all())
                .map_err(|_| context.register_error("无法持久化事务快照 marker。"))?;
            AuthorityTreeSnapshot::sync_directory(&context.root)
                .map_err(|_| context.register_error("无法持久化事务快照目录。"))?;
        }
        Ok(_) => {
            let marker = read_marker(&marker_path)
                .map_err(|_| context.register_error("事务快照 marker 身份不安全。"))?;
            if marker != format!("{}\n", context.managed_id) {
                return Err(context.register_error("事务快照 marker 不匹配。"));
            }
        }
        Err(_) => return Err(context.register_error("无法检查事务快照 marker。")),
    }
    let after = std::fs::symlink_metadata(&context.root)
        .map_err(|_| context.register_error("无法复核事务快照。"))?;
    if after.dev() != before.dev()
        || after.ino() != before.ino()
        || !after.is_dir()
        || after.file_type().is_symlink()
        || after.uid() != unsafe { libc::geteuid() }
        || after.permissions().mode() & 0o777 != 0o700
    {
        return Err(context.register_error("事务快照在注册期间发生变化。"));
    }
    let entry = PendingCleanupEntry {
        managed_id: context.managed_id.clone(),
        path: context.root.clone(),
        device: after.dev(),
        inode: after.ino(),
        marker: context.managed_id.clone(),
    };
    if !matches!(
        validate_pending_cleanup_entry(&entry, &context.expected_snapshot_parent),
        Ok(PendingCleanupTargetState::Present(ref current)) if current == &entry
    ) {
        return Err(context.register_error("事务快照持久化后身份复核失败。"));
    }
    #[cfg(test)]
    config::test_pending_cleanup_register_publish_attempt(test_pending_cleanup_identity(&entry))
        .map_err(|_| context.register_error("待清理事务清单 REGISTER 发布失败。"))?;
    let previous = config::read_pending_authority_cleanup_manifest(&context.config_dir)
        .map_err(|_| context.register_error("无法读取待清理事务清单。"))?;
    if let Some(bytes) = previous.as_deref() {
        let existing = parse_pending_cleanup_manifest(bytes)
            .map_err(|_| context.register_error("现有待清理事务清单非法。"))?;
        if !existing.entries.is_empty() && existing.entries != [entry.clone()] {
            return Err(context.register_error("已有不同的待清理事务快照。"));
        }
    }
    let manifest_raw = pending_cleanup_manifest_bytes(vec![entry.clone()])
        .map_err(|_| context.register_error("无法编码待清理事务清单。"))?;
    let publish = match previous.as_deref() {
        Some(expected) => config::write_pending_authority_cleanup_manifest(
            &context.config_dir,
            &manifest_raw,
            Some(expected),
        ),
        None => config::write_pending_authority_cleanup_manifest_if_absent(
            &context.config_dir,
            &manifest_raw,
        ),
    };
    publish.map_err(|_| context.register_error("无法原子提交待清理事务清单。"))?;
    let mut current = lock(&context.state);
    if !current
        .pending_authority_cleanup
        .iter()
        .any(|pending| pending == &context.root)
    {
        current
            .pending_authority_cleanup
            .push(context.root.clone());
    }
    Ok(RegisteredAuthorityCleanup {
        manifest_raw,
        entry,
    })
}

fn publish_pending_cleanup_clear(
    state: &SharedAppState,
    config_dir: &Path,
    manifest_raw: &[u8],
    entry: &PendingCleanupEntry,
    observe_recovery: bool,
) -> Result<(), String> {
    #[cfg(not(test))]
    let _ = observe_recovery;
    let empty = pending_cleanup_manifest_bytes(Vec::new())?;
    config::write_pending_authority_cleanup_manifest(
        config_dir,
        &empty,
        Some(manifest_raw),
    )
    .map_err(|_| {
        cleanup_required_error(
            "待清理事务快照已移除，但清单 CLEAR 未提交",
            &entry.path,
            "cleanup_clear_failed",
        )
    })?;
    #[cfg(test)]
    if observe_recovery {
        config::test_observe_pending_cleanup_clear_published();
    }
    lock(state)
        .pending_authority_cleanup
        .retain(|pending| pending != &entry.path);
    *PENDING_CLEANUP_CLEAR_RETRY
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    Ok(())
}

fn retry_completed_pending_cleanup_clear(state: &SharedAppState) -> Result<bool, String> {
    let retry = PENDING_CLEANUP_CLEAR_RETRY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let Some(retry) = retry else {
        return Ok(false);
    };
    let current = config::read_pending_authority_cleanup_manifest(&retry.config_dir)
        .map_err(|_| "cleanup_manifest_read_failed：无法读取待清理事务清单。")?;
    if current.as_deref() != Some(retry.manifest_raw.as_slice())
        || !matches!(
            inspect_pending_cleanup_target(&retry.entry),
            PendingCleanupTargetState::Missing
        )
    {
        *PENDING_CLEANUP_CLEAR_RETRY
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
        return Ok(false);
    }
    publish_pending_cleanup_clear(
        state,
        &retry.config_dir,
        &retry.manifest_raw,
        &retry.entry,
        true,
    )?;
    Ok(true)
}

fn finalize_registered_authority_cleanup(
    context: &AuthorityCleanupContext,
    ticket: &RegisteredAuthorityCleanup,
) -> Result<(), String> {
    let manifest_raw = config::read_pending_authority_cleanup_manifest(&context.config_dir)
        .map_err(|_| "cleanup_manifest_read_failed：无法安全读取刚注册的待清理事务清单。")?
        .ok_or("cleanup_manifest_missing：刚注册的待清理事务清单不存在。")?;
    if manifest_raw != ticket.manifest_raw {
        return Err(
            "cleanup_manifest_causal_mismatch：刚注册的待清理事务清单字节票据不匹配。"
                .into(),
        );
    }
    let manifest = parse_pending_cleanup_manifest(&manifest_raw)?;
    if manifest.entries.len() != 1 || manifest.entries.first() != Some(&ticket.entry) {
        return Err(
            "cleanup_manifest_causal_mismatch：刚注册的待清理事务清单因果票据不匹配。"
                .into(),
        );
    }
    match validate_pending_cleanup_entry(
        &ticket.entry,
        &context.expected_snapshot_parent,
    )? {
        PendingCleanupTargetState::Present(actual) if actual == ticket.entry => {}
        _ => {
            return Err(
                "cleanup_identity_changed：刚注册的事务快照在删除前发生变化，已停止清理。"
                    .into(),
            )
        }
    }
    if remove_authority_snapshot_root_with_retry(&ticket.entry.path).is_err() {
        return Err(cleanup_required_error(
            "one-click 事务快照仍无法清理",
            &ticket.entry.path,
            "cleanup_remove_failed",
        ));
    }
    if !matches!(
        inspect_pending_cleanup_target(&ticket.entry),
        PendingCleanupTargetState::Missing
    ) {
        return Err(
            "cleanup_identity_changed：刚注册的事务快照删除后仍存在，已停止清理。".into(),
        );
    }
    publish_pending_cleanup_clear(
        &context.state,
        &context.config_dir,
        &manifest_raw,
        &ticket.entry,
        false,
    )?;
    Ok(())
}

fn retry_pending_authority_cleanup(state: &SharedAppState) -> Result<(), String> {
    if retry_completed_pending_cleanup_clear(state)? {
        return Ok(());
    }
    let config_dir = config::default_dir();
    let Some(manifest_raw) = config::read_pending_authority_cleanup_manifest(&config_dir)
        .map_err(|_| "cleanup_manifest_read_failed：无法安全读取待清理事务清单。")?
    else {
        return Ok(());
    };
    let manifest = parse_pending_cleanup_manifest(&manifest_raw)?;
    if manifest.entries.is_empty() {
        lock(state).pending_authority_cleanup.clear();
        return Ok(());
    }
    let sandbox_home_path = sandbox_home();
    let expected_parent = sandbox_home_path
        .parent()
        .ok_or("cleanup_manifest_invalid：沙箱 HOME 无父目录。")?;
    let entry = manifest
        .entries
        .into_iter()
        .next()
        .ok_or("cleanup_manifest_invalid：待清理事务清单缺少条目。")?;
    let initial = validate_pending_cleanup_entry(&entry, expected_parent)?;
    #[cfg(test)]
    config::test_observe_pending_cleanup_manifest_validated(test_pending_cleanup_identity(&entry));
    {
        let mut current = lock(state);
        if !current
            .pending_authority_cleanup
            .iter()
            .any(|pending| pending == &entry.path)
        {
            current.pending_authority_cleanup.push(entry.path.clone());
        }
    }
    #[cfg(test)]
    config::test_observe_pending_cleanup_initial_ticket(match &initial {
        PendingCleanupTargetState::Present(_) => config::PendingCleanupInitialTicket::Present(
            test_pending_cleanup_identity(&entry),
        ),
        PendingCleanupTargetState::Missing => config::PendingCleanupInitialTicket::Missing(
            test_pending_cleanup_identity(&entry),
        ),
        PendingCleanupTargetState::Unsafe => unreachable!(),
    });
    #[cfg(test)]
    config::test_pending_cleanup_race_hook()
        .map_err(|_| "cleanup_race_hook_failed：待清理事务快照复核失败。")?;
    let current = inspect_pending_cleanup_target(&entry);
    let completed = match (&initial, &current) {
        (
            PendingCleanupTargetState::Present(_),
            PendingCleanupTargetState::Present(actual),
        ) if actual == &entry => {
            #[cfg(test)]
            config::test_observe_pending_cleanup_delete_attempt();
            if remove_authority_snapshot_root_with_retry(&entry.path).is_err() {
                #[cfg(test)]
                config::test_observe_pending_cleanup_completion(
                    config::PendingCleanupRemovalOutcome::Error,
                    config::PendingCleanupFinalState::Present(
                        test_pending_cleanup_identity(&entry),
                    ),
                );
                return Err(cleanup_required_error(
                    "待清理 one-click 事务快照仍无法清理",
                    &entry.path,
                    "cleanup_remove_failed",
                ));
            }
            matches!(
                inspect_pending_cleanup_target(&entry),
                PendingCleanupTargetState::Missing
            )
        }
        (PendingCleanupTargetState::Missing, PendingCleanupTargetState::Missing) => true,
        _ => false,
    };
    if !completed {
        #[cfg(test)]
        config::test_observe_pending_cleanup_completion(
            config::PendingCleanupRemovalOutcome::Error,
            match current {
                PendingCleanupTargetState::Missing => config::PendingCleanupFinalState::NotFound,
                PendingCleanupTargetState::Present(actual) => {
                    config::PendingCleanupFinalState::Present(test_pending_cleanup_identity(&actual))
                }
                PendingCleanupTargetState::Unsafe => config::PendingCleanupFinalState::Error,
            },
        );
        return Err(
            "cleanup_identity_changed：待清理事务快照在删除前后发生变化，已停止运行。".into(),
        );
    }
    #[cfg(test)]
    config::test_observe_pending_cleanup_completion(
        match initial {
            PendingCleanupTargetState::Present(_) => config::PendingCleanupRemovalOutcome::Removed,
            PendingCleanupTargetState::Missing => {
                config::PendingCleanupRemovalOutcome::AlreadyAbsent
            }
            PendingCleanupTargetState::Unsafe => unreachable!(),
        },
        config::PendingCleanupFinalState::NotFound,
    );
    *PENDING_CLEANUP_CLEAR_RETRY
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(PendingCleanupClearRetry {
        config_dir: config_dir.clone(),
        manifest_raw: manifest_raw.clone(),
        entry: entry.clone(),
    });
    publish_pending_cleanup_clear(state, &config_dir, &manifest_raw, &entry, true)?;
    Ok(())
}

#[derive(Default)]
struct AuthorityCopyBudget {
    entries: usize,
    bytes: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct AuthorityDirectoryEntryIdentity {
    name: std::ffi::OsString,
    kind: u8,
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl AuthorityTreeSnapshot {
    fn directory_manifest(
        entries: &[std::fs::DirEntry],
    ) -> Result<Vec<AuthorityDirectoryEntryIdentity>, String> {
        entries
            .iter()
            .map(|entry| {
                let metadata = std::fs::symlink_metadata(entry.path()).map_err(|error| {
                    format!("无法复核隔离 authority 目录成员：{error}")
                })?;
                let kind = if metadata.is_file() {
                    1
                } else if metadata.is_dir() {
                    2
                } else if metadata.file_type().is_symlink() {
                    3
                } else {
                    4
                };
                Ok(AuthorityDirectoryEntryIdentity {
                    name: entry.file_name(),
                    kind,
                    device: metadata.dev(),
                    inode: metadata.ino(),
                    size: metadata.len(),
                    mode: metadata.permissions().mode() & 0o777,
                    modified_seconds: metadata.mtime(),
                    modified_nanoseconds: metadata.mtime_nsec(),
                })
            })
            .collect()
    }

    fn capture(source: PathBuf, backup: PathBuf) -> Result<Self, String> {
        #[cfg(test)]
        if SANDBOX_SESSION_TEST_SEAMS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .capture_fail_source
            .as_ref()
            == Some(&source)
        {
            return Err(format!(
                "test-only authority snapshot capture failure for {}",
                source.display()
            ));
        }
        let existed = match std::fs::symlink_metadata(&source) {
            Ok(_) => {
                let mut budget = AuthorityCopyBudget::default();
                Self::copy_tree(&source, &backup, &mut budget)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(format!(
                    "无法快照隔离 authority {}：{error}",
                    source.display()
                ))
            }
        };
        Ok(Self {
            source,
            backup,
            existed,
        })
    }

    fn charge_entry(
        budget: &mut AuthorityCopyBudget,
        file_bytes: u64,
    ) -> Result<(), String> {
        budget.entries = budget
            .entries
            .checked_add(1)
            .ok_or("隔离 authority 快照条目计数溢出")?;
        if budget.entries > MAX_AUTHORITY_SNAPSHOT_ENTRIES {
            return Err(format!(
                "隔离 authority 快照超过安全条目上限 {MAX_AUTHORITY_SNAPSHOT_ENTRIES}"
            ));
        }
        if file_bytes > MAX_AUTHORITY_SNAPSHOT_FILE_BYTES {
            return Err(format!(
                "隔离 authority 单文件超过安全上限 {MAX_AUTHORITY_SNAPSHOT_FILE_BYTES} bytes"
            ));
        }
        budget.bytes = budget
            .bytes
            .checked_add(file_bytes)
            .ok_or("隔离 authority 快照大小溢出")?;
        if budget.bytes > MAX_AUTHORITY_SNAPSHOT_TOTAL_BYTES {
            return Err(format!(
                "隔离 authority 快照超过安全总大小上限 {MAX_AUTHORITY_SNAPSHOT_TOTAL_BYTES} bytes"
            ));
        }
        Ok(())
    }

    fn sync_directory(path: &Path) -> Result<(), String> {
        std::fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("无法持久化隔离 authority 目录：{error}"))
    }

    fn copy_tree(
        source: &Path,
        backup: &Path,
        budget: &mut AuthorityCopyBudget,
    ) -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(source)
            .map_err(|error| format!("无法检查隔离 authority {}：{error}", source.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "隔离 authority {} 包含符号链接，拒绝建立事务快照",
                source.display()
            ));
        }
        if metadata.is_file() {
            Self::charge_entry(budget, metadata.len())?;
            let parent = backup.parent().ok_or("隔离 authority 备份路径没有父目录")?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建隔离 authority 备份目录：{error}"))?;
            let mut input = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(source)
                .map_err(|error| format!("无法打开隔离 authority 快照源：{error}"))?;
            let opened = input
                .metadata()
                .map_err(|error| format!("无法复核隔离 authority 快照源：{error}"))?;
            if !opened.is_file()
                || opened.dev() != metadata.dev()
                || opened.ino() != metadata.ino()
                || opened.len() != metadata.len()
            {
                return Err("隔离 authority 快照源在读取前发生变化".into());
            }
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(backup)
                .map_err(|error| format!("无法创建隔离 authority 独立快照：{error}"))?;
            let copied = std::io::copy(&mut input, &mut output)
                .map_err(|error| format!("无法复制隔离 authority 快照：{error}"))?;
            if copied != metadata.len() {
                return Err("隔离 authority 快照源在读取期间发生变化".into());
            }
            output
                .sync_all()
                .map_err(|error| format!("无法持久化隔离 authority 快照：{error}"))?;
            std::fs::set_permissions(
                backup,
                std::fs::Permissions::from_mode(metadata.permissions().mode() & 0o777),
            )
            .map_err(|error| format!("无法保留隔离 authority 文件权限：{error}"))?;
            let final_metadata = input
                .metadata()
                .map_err(|error| format!("无法复核隔离 authority 快照源：{error}"))?;
            if final_metadata.dev() != metadata.dev()
                || final_metadata.ino() != metadata.ino()
                || final_metadata.len() != metadata.len()
                || final_metadata.mtime() != metadata.mtime()
                || final_metadata.mtime_nsec() != metadata.mtime_nsec()
            {
                return Err("隔离 authority 快照源在读取期间发生变化".into());
            }
            return Ok(());
        }
        if !metadata.is_dir() {
            return Err(format!(
                "隔离 authority {} 不是普通文件或目录",
                source.display()
            ));
        }
        Self::charge_entry(budget, 0)?;
        std::fs::create_dir(backup)
            .map_err(|error| format!("无法创建隔离 authority 快照目录：{error}"))?;
        let mut children = std::fs::read_dir(source)
            .map_err(|error| format!("无法枚举隔离 authority：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("无法枚举隔离 authority：{error}"))?;
        children.sort_by_key(|entry| entry.file_name());
        let initial_manifest = Self::directory_manifest(&children)?;
        #[cfg(test)]
        if let Some(barrier) = SANDBOX_SESSION_TEST_SEAMS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .directory_barrier
            .as_ref()
            .filter(|(target, _)| target == source)
            .map(|(_, barrier)| barrier.clone())
        {
            std::fs::create_dir_all(&barrier)
                .map_err(|error| format!("test-only snapshot barrier create failed: {error}"))?;
            std::fs::write(barrier.join("ready"), b"ready\n")
                .map_err(|error| format!("test-only snapshot barrier arm failed: {error}"))?;
            let mut released = false;
            for _ in 0..200 {
                if barrier.join("release").is_file() {
                    released = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !released {
                return Err("test-only authority snapshot barrier timed out".into());
            }
        }
        for child in children {
            Self::copy_tree(&child.path(), &backup.join(child.file_name()), budget)?;
        }
        let mut final_children = std::fs::read_dir(source)
            .map_err(|error| format!("无法复核隔离 authority 目录：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("无法复核隔离 authority 目录：{error}"))?;
        final_children.sort_by_key(|entry| entry.file_name());
        let final_manifest = Self::directory_manifest(&final_children)?;
        let final_metadata = std::fs::symlink_metadata(source)
            .map_err(|error| format!("无法复核隔离 authority 目录：{error}"))?;
        if initial_manifest != final_manifest
            || !final_metadata.is_dir()
            || final_metadata.dev() != metadata.dev()
            || final_metadata.ino() != metadata.ino()
            || final_metadata.uid() != metadata.uid()
            || final_metadata.permissions().mode() & 0o777
                != metadata.permissions().mode() & 0o777
            || final_metadata.mtime() != metadata.mtime()
            || final_metadata.mtime_nsec() != metadata.mtime_nsec()
        {
            return Err("隔离 authority 目录在快照期间发生变化".into());
        }
        std::fs::set_permissions(
            backup,
            std::fs::Permissions::from_mode(metadata.permissions().mode() & 0o777),
        )
        .map_err(|error| format!("无法保留隔离 authority 目录权限：{error}"))?;
        Self::sync_directory(backup)?;
        Ok(())
    }

    fn remove_current(path: &Path) -> Result<(), String> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("无法检查待恢复 authority：{error}")),
        };
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "待恢复 authority {} 变成符号链接，拒绝跟随",
                path.display()
            ));
        }
        if metadata.is_dir() {
            std::fs::remove_dir_all(path)
                .map_err(|error| format!("无法移除待恢复 authority 目录：{error}"))
        } else if metadata.is_file() {
            std::fs::remove_file(path)
                .map_err(|error| format!("无法移除待恢复 authority 文件：{error}"))
        } else {
            Err(format!(
                "待恢复 authority {} 变成特殊文件，拒绝修改",
                path.display()
            ))
        }
    }

    fn restore(&mut self) -> Result<(), String> {
        Self::remove_current(&self.source)?;
        if self.existed {
            let parent = self.source.parent().ok_or("隔离 authority 没有父目录")?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建隔离 authority 恢复目录：{error}"))?;
            let mut budget = AuthorityCopyBudget::default();
            Self::copy_tree(&self.backup, &self.source, &mut budget)
                .map_err(|error| format!("无法恢复隔离 authority：{error}"))?;
            Self::sync_directory(parent)?;
        }
        Ok(())
    }
}

struct AppAuthoritySnapshot {
    proxy_present: bool,
    proxy_port: u16,
    secret: String,
    provider: String,
    gateway_kind: String,
    shim_mode: String,
    launch_id: String,
    key_fp: u64,
    gateway_launch_context: Option<crate::GatewayLaunchContext>,
    sandbox_present: bool,
    sandbox_port: u16,
    sandbox_url: Option<String>,
    science_runtime: Option<ScienceRuntimeIdentity>,
    science_confirmed_stopped: Option<ScienceRuntimeIdentity>,
    history_recovery: Option<HistoryRecoverySession>,
    pending_authority_cleanup: Vec<PathBuf>,
}

impl AppAuthoritySnapshot {
    fn capture(state: &SharedAppState) -> Self {
        let state = lock(state);
        Self {
            proxy_present: state.proxy.is_some(),
            proxy_port: state.proxy_port,
            secret: state.secret.clone(),
            provider: state.provider.clone(),
            gateway_kind: state.gateway_kind.clone(),
            shim_mode: state.shim_mode.clone(),
            launch_id: state.launch_id.clone(),
            key_fp: state.key_fp,
            gateway_launch_context: state.gateway_launch_context.clone(),
            sandbox_present: state.sandbox.is_some(),
            sandbox_port: state.sandbox_port,
            sandbox_url: state.sandbox_url.clone(),
            science_runtime: state.science_runtime.clone(),
            science_confirmed_stopped: state.science_confirmed_stopped.clone(),
            history_recovery: state.history_recovery.clone(),
            pending_authority_cleanup: state.pending_authority_cleanup.clone(),
        }
    }

    fn restore(&self, state: &SharedAppState, proxy_action: ProxyAction) -> Result<(), String> {
        let mut current = lock(state);
        if proxy_action == ProxyAction::Restarted {
            current.stop_proxy();
        }
        if current.sandbox.is_some() && !self.sandbox_present {
            return Err("late-failure 补偿发现未预期的 Science child，拒绝伪造恢复状态".into());
        }
        if self.proxy_present != current.proxy.is_some() {
            return Err("late-failure 补偿无法恢复先前 Gateway child 所有权".into());
        }
        current.proxy_port = self.proxy_port;
        current.secret = self.secret.clone();
        current.provider = self.provider.clone();
        current.gateway_kind = self.gateway_kind.clone();
        current.shim_mode = self.shim_mode.clone();
        current.launch_id = self.launch_id.clone();
        current.key_fp = self.key_fp;
        current.gateway_launch_context = self.gateway_launch_context.clone();
        current.sandbox_port = self.sandbox_port;
        current.sandbox_url = self.sandbox_url.clone();
        current.science_runtime = self.science_runtime.clone();
        current.science_confirmed_stopped = self.science_confirmed_stopped.clone();
        current.history_recovery = self.history_recovery.clone();
        current.pending_authority_cleanup = self.pending_authority_cleanup.clone();
        Ok(())
    }

    fn restore_with_gateway<R: Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        state: &SharedAppState,
        lifecycle: &lifecycle::Lifecycle,
        auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
        proxy_action: ProxyAction,
    ) -> Result<(), String> {
        if proxy_action == ProxyAction::Restarted {
            lock(state).stop_proxy();
        }
        if self.proxy_present {
            let context = self
                .gateway_launch_context
                .as_ref()
                .ok_or("late-failure 补偿缺少先前 Gateway 内存启动上下文")?;
            start_proxy_for(
                app,
                state,
                lifecycle,
                &context.profile,
                context.science_runtime.as_ref(),
                None,
                auth_proof,
            )
            .map_err(|error| format!("late-failure 补偿无法重启先前 Gateway：{error}"))?;
            if lock(state).proxy.is_none() {
                return Err("late-failure 补偿未恢复先前 Gateway child 所有权".into());
            }
        } else {
            let mut current = lock(state);
            if current.proxy.is_some() {
                return Err("late-failure 补偿发现未预期的 Gateway child".into());
            }
            current.proxy_port = self.proxy_port;
            current.secret = self.secret.clone();
            current.provider = self.provider.clone();
            current.gateway_kind = self.gateway_kind.clone();
            current.shim_mode = self.shim_mode.clone();
            current.launch_id = self.launch_id.clone();
            current.key_fp = self.key_fp;
            current.gateway_launch_context = self.gateway_launch_context.clone();
        }
        let mut current = lock(state);
        if current.sandbox.is_some() && !self.sandbox_present {
            return Err("late-failure 补偿发现未预期的 Science child，拒绝伪造恢复状态".into());
        }
        current.sandbox_port = self.sandbox_port;
        current.sandbox_url = self.sandbox_url.clone();
        current.science_runtime = self.science_runtime.clone();
        current.science_confirmed_stopped = self.science_confirmed_stopped.clone();
        current.history_recovery = self.history_recovery.clone();
        current.pending_authority_cleanup = self.pending_authority_cleanup.clone();
        Ok(())
    }
}

struct OneClickAuthoritySnapshot {
    backup_root: PathBuf,
    cleanup_context: AuthorityCleanupContext,
    trees: Vec<AuthorityTreeSnapshot>,
    config: config::Config,
    app: AppAuthoritySnapshot,
    preserve_recovery: bool,
    cleanup_prepared: bool,
}

impl OneClickAuthoritySnapshot {
    fn capture(
        config_dir: &Path,
        sandbox_home: &Path,
        auth_dir: &Path,
        config: &config::Config,
        state: &SharedAppState,
    ) -> Result<Self, String> {
        #[cfg(test)]
        {
            let capture_seam = SANDBOX_SESSION_TEST_SEAMS
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .one_click_capture
                .as_ref()
                .filter(|(target_dir, _, _, _, _)| target_dir == config_dir)
                .cloned();
            if let Some((_, observation, _, expected_prior_pid, expected_receipt)) =
                capture_seam.as_ref()
            {
                let listener_state = if proc::loopback_port_in_use(
                    config.sandbox_port,
                    operation::LOCAL_HEALTH_TIMEOUT_MS,
                ) {
                    "running"
                } else {
                    "stopped"
                };
                let prior_process = if crate::runtime::science::test_process_start_identity_for_pid(
                    *expected_prior_pid,
                )
                .is_some()
                {
                    "alive"
                } else {
                    "absent"
                };
                let prior_receipt = if expected_receipt.exists() {
                    "present"
                } else {
                    "absent"
                };
                std::fs::write(
                    observation,
                    format!(
                        "expected_prior_pid={expected_prior_pid}\nexpected_receipt={}\nlistener={listener_state}\nprior_process={prior_process}\nprior_receipt={prior_receipt}\n",
                        expected_receipt.display()
                    ),
                )
                .map_err(|error| {
                        format!("test-only authority snapshot observation failed: {error}")
                    })?;
            }
            if capture_seam.is_some_and(|(_, _, fail, _, _)| fail) {
                return Err("test-only one-click authority snapshot capture failure".into());
            }
        }
        let sandbox_dir = sandbox_home
            .parent()
            .ok_or("沙箱 HOME 无父目录，无法建立事务快照")?;
        let cleanup_context =
            AuthorityCleanupContext::new(config_dir, sandbox_home, state)?;
        let backup_root = cleanup_context.root.clone();
        std::fs::create_dir(&backup_root)
            .map_err(|error| format!("无法创建 one-click 事务快照：{error}"))?;
        std::fs::set_permissions(&backup_root, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("无法收紧 one-click 事务快照权限：{error}"))?;
        let sources = [
            auth_dir.to_path_buf(),
            sandbox_dir.join("state"),
            config_dir.join("runtime"),
            config_dir.join("science-managed-launch.v1.json"),
        ];
        let mut trees = Vec::with_capacity(sources.len());
        for (index, source) in sources.into_iter().enumerate() {
            match AuthorityTreeSnapshot::capture(source, backup_root.join(index.to_string())) {
                Ok(snapshot) => trees.push(snapshot),
                Err(error) => {
                    let cleanup = register_authority_cleanup(&cleanup_context)
                        .and_then(|ticket| {
                            finalize_registered_authority_cleanup(
                                &cleanup_context,
                                &ticket,
                            )
                        });
                    if let Err(cleanup_error) = cleanup {
                        return Err(format!("{error}；{cleanup_error}"));
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            backup_root,
            cleanup_context,
            trees,
            config: config.clone(),
            app: AppAuthoritySnapshot::capture(state),
            preserve_recovery: false,
            cleanup_prepared: false,
        })
    }

    fn restore(
        &mut self,
        config_dir: &Path,
        state: &SharedAppState,
        proxy_action: ProxyAction,
    ) -> Result<(), String> {
        let mut errors = Vec::new();
        for tree in &mut self.trees {
            if let Err(error) = tree.restore() {
                errors.push(error);
            }
        }
        if let Err(error) =
            config::save_to(config_dir, &self.config).map_err(|error| error.to_string())
        {
            errors.push(error);
        }
        if let Err(error) = self.app.restore(state, proxy_action) {
            errors.push(error);
        }
        if errors.is_empty() {
            return self.cleanup_when_expendable();
        }
        self.preserve_recovery = true;
        Err(errors.join("; "))
    }

    fn restore_with_gateway<R: Runtime>(
        &mut self,
        app: &tauri::AppHandle<R>,
        config_dir: &Path,
        state: &SharedAppState,
        lifecycle: &lifecycle::Lifecycle,
        auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
        proxy_action: ProxyAction,
    ) -> Result<(), String> {
        if proxy_action == ProxyAction::Restarted {
            lock(state).stop_proxy();
        }
        let mut errors = Vec::new();
        for tree in &mut self.trees {
            if let Err(error) = tree.restore() {
                errors.push(error);
            }
        }
        if let Err(error) =
            config::save_to(config_dir, &self.config).map_err(|error| error.to_string())
        {
            errors.push(error);
        }
        if let Err(error) =
            self.app
                .restore_with_gateway(app, state, lifecycle, auth_proof, proxy_action)
        {
            errors.push(error);
        }
        #[cfg(test)]
        {
            let mut seams = SANDBOX_SESSION_TEST_SEAMS
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(canary) = seams.rollback_diagnostic_canary.clone() {
                seams.rollback_diagnostic_snapshot = Some(self.backup_root.clone());
                errors.push(format!("test-only rollback diagnostic {canary}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            self.preserve_recovery = true;
            Err(errors.join("; "))
        }
    }

    fn cleanup_when_expendable(&mut self) -> Result<(), String> {
        self.preserve_recovery = true;
        let ticket = register_authority_cleanup(&self.cleanup_context)?;
        match finalize_registered_authority_cleanup(&self.cleanup_context, &ticket) {
            Ok(()) => {
                self.preserve_recovery = false;
                self.cleanup_prepared = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn prepare_success(
        &mut self,
        value: &mut Value,
    ) -> Result<(), String> {
        match self.cleanup_when_expendable() {
            Ok(()) => Ok(()),
            Err(error) if error.contains("recovery_status=cleanup_required") => {
                self.preserve_recovery = true;
                self.cleanup_prepared = true;
                if let Some(object) = value.as_object_mut() {
                    object.insert("status".into(), Value::String("degraded".into()));
                    object.insert(
                        "recovery_status".into(),
                        Value::String("cleanup_required".into()),
                    );
                    object.insert(
                        "cleanup_recovery_path".into(),
                        Value::String(self.backup_root.to_string_lossy().into_owned()),
                    );
                    object.insert(
                        "cleanup_message".into(),
                        Value::String(
                            "one-click 已完成，但私有事务快照需要稍后安全清理。".into(),
                        ),
                    );
                }
                Ok(())
            }
            Err(error) => {
                self.preserve_recovery = true;
                Err(error)
            }
        }
    }

    fn commit(&mut self) {
        if !self.cleanup_prepared {
            let _ = self.cleanup_when_expendable();
        }
    }
}

impl Drop for OneClickAuthoritySnapshot {
    fn drop(&mut self) {
        if !self.preserve_recovery {
            let _ = remove_authority_snapshot_root(&self.backup_root);
        }
    }
}

fn validate_system_ssh_wrapper_path<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<PathBuf, String> {
    #[cfg(test)]
    let wrapper_override =
        std::env::var_os("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE").map(PathBuf::from);
    #[cfg(not(test))]
    let wrapper_override: Option<PathBuf> = None;
    let wrapper = match wrapper_override {
        Some(wrapper) => wrapper,
        None => {
            let root = asset_root(app).ok_or("打包的 CSSwitch SSH bridge 缺失")?;
            let scripts = root.join("scripts");
            let wrapper_dir = scripts.join("ssh-bridge");
            let wrapper = wrapper_dir.join("ssh");
            for path in [&root, &scripts, &wrapper_dir] {
                let metadata = std::fs::symlink_metadata(path)
                    .map_err(|_| "打包的 CSSwitch SSH bridge 缺失".to_string())?;
                if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                    return Err("打包的 CSSwitch SSH bridge 不是安全的可执行文件".into());
                }
            }
            wrapper
        }
    };
    let metadata = std::fs::symlink_metadata(&wrapper)
        .map_err(|_| "打包的 CSSwitch SSH bridge 缺失".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.len() > 128 * 1024
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err("打包的 CSSwitch SSH bridge 不是安全的可执行文件".into());
    }
    Ok(wrapper)
}

fn validate_running_system_ssh_bridge<R: Runtime>(
    app: &tauri::AppHandle<R>,
    sandbox_home: &Path,
) -> Result<(), String> {
    let _validated_wrapper =
        crate::runtime::sandbox_session::validate_system_ssh_wrapper_path(app)?;
    let expected_hosts = crate::runtime::ssh_bridge::validate_science_ssh_bridge(sandbox_home)?;
    crate::runtime::settings::validate_managed_sandbox_ssh_stub(sandbox_home, &expected_hosts)?;
    Ok(())
}

fn prevalidate_one_click_system_ssh<R: Runtime>(
    app: &tauri::AppHandle<R>,
    cfg: &config::Config,
    sandbox_home: &Path,
) -> Result<Vec<String>, String> {
    let expected_hosts = crate::runtime::ssh_bridge::prevalidate_science_ssh_bridge(
        sandbox_home,
        cfg.reuse_system_ssh,
    )?;
    crate::runtime::settings::prevalidate_sandbox_ssh_stub(
        sandbox_home,
        &expected_hosts,
        cfg.reuse_system_ssh,
    )?;
    if cfg.reuse_system_ssh {
        let _validated_wrapper =
            crate::runtime::sandbox_session::validate_system_ssh_wrapper_path(app)?;
    }
    Ok(expected_hosts)
}

fn verify_gateway_model_catalog(
    port: u16,
    secret: &str,
    profile: &config::Profile,
) -> Result<(), String> {
    let timeout_ms = gateway_model_catalog_timeout_ms(profile);
    let (status, body) =
        proc::http_get_body_cancellable(port, Some(secret), "/v1/models", timeout_ms, None)
            .ok_or("gateway 模型目录探活无响应")?;
    if status != 200 {
        return Err(format!("gateway 模型目录探活返回 {status}"));
    }
    let value: Value = serde_json::from_str(&body).map_err(|_| "gateway 模型目录不是合法 JSON")?;
    let ids: Vec<&str> = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .collect();
    if profile.model_policy == crate::provider_contracts::ModelPolicy::DynamicCatalog {
        if ids.is_empty()
            || ids
                .iter()
                .any(|id| !id.starts_with("claude-csswitch-codex-"))
        {
            return Err("Codex published model snapshot 为空或包含非法 alias".into());
        }
        return Ok(());
    }
    let expected: std::collections::BTreeSet<&str> = profile
        .model_catalog
        .iter()
        .map(|route| route.selector_id.as_str())
        .collect();
    let actual: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
    if actual != expected || ids.first().copied() != Some(profile.default_model_route_id.as_str()) {
        return Err("gateway 模型目录与已提交白名单/default selector 不一致".into());
    }
    Ok(())
}

fn gateway_model_catalog_timeout_ms(profile: &config::Profile) -> u64 {
    if profile.model_policy == crate::provider_contracts::ModelPolicy::DynamicCatalog {
        operation::CODEX_MODELS_PROBE_TIMEOUT_MS
    } else {
        operation::LOCAL_HEALTH_TIMEOUT_MS
    }
}

fn verify_gateway_model_catalog_traced(
    trace: &OperationTrace,
    port: u16,
    secret: &str,
    profile: &config::Profile,
) -> Result<(), String> {
    trace.stage(
        OperationStage::CatalogVerify,
        format!(
            "start policy={:?} timeout_ms={}",
            profile.model_policy,
            gateway_model_catalog_timeout_ms(profile)
        ),
    );
    #[cfg(test)]
    if SANDBOX_SESSION_TEST_SEAMS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .catalog_failure_port
        == Some(port)
    {
        trace.stage(OperationStage::CatalogVerify, "outcome=test_error");
        trace.finish("error=test_catalog_verify_after_gateway_restart");
        return Err("test-only healthy reopen catalog failure after Gateway restart".into());
    }
    match verify_gateway_model_catalog(port, secret, profile) {
        Ok(()) => {
            trace.stage(OperationStage::CatalogVerify, "outcome=ok");
            Ok(())
        }
        Err(error) => {
            trace.stage(OperationStage::CatalogVerify, "outcome=error");
            trace.finish("error=catalog_verify");
            Err(error)
        }
    }
}

fn configure_third_party_best_effort<R: Runtime>(
    app: &tauri::AppHandle<R>,
    status: RegistrationStatus,
    data_dir: &std::path::Path,
    port: u16,
    runtime: &ScienceRuntimeIdentity,
    force: bool,
) -> RegistrationStatus {
    if !matches!(
        status,
        RegistrationStatus::Registered | RegistrationStatus::AlreadyRegistered
    ) {
        let _ = invalidate_route_configuration(data_dir);
        return status;
    }
    let Some(science_version) = runtime.version.as_deref() else {
        let _ = invalidate_route_configuration(data_dir);
        return RegistrationStatus::Warning(
            "Science 版本无法确认，未记录第三方能力配置状态".into(),
        );
    };
    let needs_configuration = force
        || matches!(status, RegistrationStatus::Registered)
        || match route_configuration_is_current(data_dir, science_version) {
            Ok(current) => !current,
            Err(error) => return RegistrationStatus::Warning(error),
        };
    if !needs_configuration {
        return status;
    }
    if let Err(error) = invalidate_route_configuration(data_dir) {
        return RegistrationStatus::Warning(error);
    }
    let control_url = sandbox_url(port, runtime);
    if let Err(error) = configure_third_party_after_science_start(app, &control_url) {
        return RegistrationStatus::Warning(error);
    }
    match mark_route_configuration_current(data_dir, science_version) {
        Ok(()) => status,
        Err(error) => RegistrationStatus::Warning(error),
    }
}

/// Explicit doctor action: bypass the version cache and route marker without
/// starting Science or the proxy solely for diagnostics.
pub(crate) fn force_third_party_reconcile<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &SharedAppState,
) -> Result<String, String> {
    let cfg = config::load_from(&config::default_dir()).map_err(|error| error.to_string())?;
    let data_dir = sandbox_data_dir();
    let (remembered_runtime, version_cache) = {
        let st = lock(state);
        (st.science_runtime.clone(), st.science_version_cache.clone())
    };

    let (science_state, running_runtime) = match remembered_runtime {
        Some(mut runtime) => {
            if !runtime_identity_is_current(&runtime) {
                invalidate_route_configuration(&data_dir)?;
                return Ok(
                    "Science 二进制文件已变化；已安排下次停止并启动后重新选择 runtime。".into(),
                );
            }
            let previous_version = runtime.version.clone();
            let refreshed = version_cache
                .force_refresh(&runtime.path)
                .ok_or("Science 版本强制复检失败")?;
            if previous_version
                .as_deref()
                .is_some_and(|version| version != refreshed)
            {
                invalidate_route_configuration(&data_dir)?;
                return Ok(
                    "Science 二进制版本已变化；已安排下次停止并启动后重新配置 Skill 路由。".into(),
                );
            }
            runtime.version = Some(refreshed);
            let science_state = probe_known_runtime(cfg.sandbox_port, &runtime);
            let running = (science_state == SandboxScienceState::RunningHealthy).then_some(runtime);
            (science_state, running)
        }
        None => {
            version_cache.clear();
            probe_sandbox_runtime_cached(cfg.sandbox_port, &version_cache)?
        }
    };

    if cfg.mode == "official" {
        return Ok("官方模式无需核验 CSSwitch 第三方 Skill 路由。".into());
    }
    match science_state {
        SandboxScienceState::Stopped => {
            invalidate_route_configuration(&data_dir)?;
            Ok("Science 未运行；已安排下次一键开始重新核验 Skill 路由。".into())
        }
        SandboxScienceState::Unknown => {
            invalidate_route_configuration(&data_dir)?;
            Err("无法确认 Science 实例身份；已使路由标记失效，未执行修复".into())
        }
        SandboxScienceState::RunningHealthy => {
            let runtime = running_runtime.ok_or("Science 运行身份缺失")?;
            let secret = { lock(state).secret.clone() };
            if secret.is_empty() {
                invalidate_route_configuration(&data_dir)?;
                return Ok("当前代理身份不可用；已安排下次一键开始重新核验 Skill 路由。".into());
            }
            let bridge_dir = skill_install_bridge_dir(&secret)?;
            let bridge_key = match current_skill_install_bridge_key() {
                Ok(path) => path,
                Err(error) => {
                    invalidate_route_configuration(&data_dir)?;
                    return Ok(format!(
                        "Skill bridge 尚未就绪；已安排下次一键开始重新核验：{error}"
                    ));
                }
            };
            let status = inspect_while_science_running(app, &data_dir, &bridge_dir, &bridge_key);
            let status = configure_third_party_best_effort(
                app,
                status,
                &data_dir,
                cfg.sandbox_port,
                &runtime,
                true,
            );
            {
                let mut st = lock(state);
                st.science_runtime = Some(runtime);
                st.science_confirmed_stopped = None;
            }
            match status {
                RegistrationStatus::AlreadyRegistered | RegistrationStatus::Registered => {
                    Ok("Skill 路由已强制核验并同步。".into())
                }
                RegistrationStatus::RestartRequired => {
                    Ok("Skill 路由文件需要重启 Science 后加载；状态标记已失效。".into())
                }
                RegistrationStatus::Warning(message) => {
                    Ok(format!("Skill 路由核验未完成：{message}"))
                }
            }
        }
    }
}

/// One-click session startup: active proxy, virtual login, sandbox, browser.
///
/// Callers must hold the command serializer lock.
pub(crate) fn one_click_login<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    runtime_choice: Option<&str>,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
) -> Result<Value, String> {
    one_click_login_with_options(app, state, lifecycle, runtime_choice, auth_proof, true)
}

pub(crate) fn reconcile_science_for_active<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
) -> Result<Value, String> {
    one_click_login_with_options(app, state, lifecycle, None, auth_proof, false)
}

/// Rollback-only recovery path. The persisted config is already the old,
/// authoritative profile. Do not trust its previous runtime binding to decide
/// reuse: a healthy process may actually have loaded the failed candidate
/// catalog. Stop only the exact in-memory Science identity and start the
/// committed chain again from a clean process.
pub(crate) fn force_restart_science_for_active<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
) -> Result<Value, String> {
    let cfg = config::load_from(&config::default_dir()).map_err(|error| error.to_string())?;
    let remembered = { lock(&state).science_runtime.clone() };
    match remembered {
        Some(runtime) => match probe_known_runtime(cfg.sandbox_port, &runtime) {
            SandboxScienceState::RunningHealthy => {
                let mut st = lock(&state);
                st.science_runtime = Some(runtime);
                stop_sandbox_state(&app, &mut st).map_err(|error| {
                    format!("回滚时停止候选 Science 失败，未猜测 PID 或按端口结束进程：{error}")
                })?;
            }
            SandboxScienceState::Stopped => {
                let mut st = lock(&state);
                st.science_confirmed_stopped = Some(runtime);
                st.science_runtime = None;
            }
            SandboxScienceState::Unknown => {
                return Err(
                    "回滚时 Science 可能正在运行，但身份无法确认；已拒绝猜测 PID 或按端口结束进程。"
                        .into(),
                );
            }
        },
        None if proc::loopback_port_in_use(
            cfg.sandbox_port,
            operation::LOCAL_HEALTH_TIMEOUT_MS,
        ) =>
        {
            return Err(
                "回滚时 Science 端口仍被占用，但没有可确认的 runtime 身份；已拒绝强制结束。".into(),
            );
        }
        None => {}
    }
    one_click_login_with_options(app, state, lifecycle, None, auth_proof, false)
}

fn advance_runtime_transaction(
    dir: &Path,
    active_profile_id: &str,
    previous_binding: Option<config::RuntimeBindingCommit>,
    stage: &str,
) -> Result<(), String> {
    config::update(dir, |current| match current.runtime_transaction.as_mut() {
        Some(journal) if journal.target_profile_id == active_profile_id => {
            journal.stage = stage.to_string();
        }
        _ => {
            current.runtime_transaction = Some(config::RuntimeTransactionJournal {
                transaction_id: config::new_id(),
                target_profile_id: active_profile_id.to_string(),
                stage: stage.to_string(),
                previous_binding: previous_binding.clone(),
                previous_gateway: None,
            });
        }
    })
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[derive(Clone)]
struct OneClickRollbackContext {
    proxy_action: ProxyAction,
    launch_runtime: ScienceRuntimeIdentity,
    launch_token: Option<ScienceManagedLaunchToken>,
    ssh_stub_transaction: Option<crate::runtime::settings::ManagedSshStubTransaction>,
}

struct OneClickFailure {
    message: String,
    rollback: OneClickRollbackContext,
}

#[derive(Clone)]
struct PriorScienceContext {
    runtime: ScienceRuntimeIdentity,
    port: u16,
    launch_token: ScienceManagedLaunchToken,
}

impl OneClickRollbackContext {
    fn failure(&self, message: impl Into<String>) -> OneClickFailure {
        OneClickFailure {
            message: message.into(),
            rollback: self.clone(),
        }
    }
}

fn one_click_step<T, E: std::fmt::Display>(
    result: Result<T, E>,
    rollback: &OneClickRollbackContext,
) -> Result<T, OneClickFailure> {
    result.map_err(|error| rollback.failure(error.to_string()))
}

fn restart_prior_science<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &SharedAppState,
    _lifecycle: &lifecycle::Lifecycle,
    _auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
    prior: &PriorScienceContext,
) -> Result<(), String> {
    let dir = config::default_dir();
    let cfg = config::load_from(&dir).map_err(|error| error.to_string())?;
    if cfg.sandbox_port != prior.port {
        return Err("恢复 prior Science 时沙箱端口已变化".into());
    }
    if !runtime_identity_is_current(&prior.runtime) {
        return Err("恢复 prior Science 时 runtime 身份已变化".into());
    }
    if proc::loopback_port_in_use(prior.port, operation::LOCAL_HEALTH_TIMEOUT_MS) {
        return Err("恢复 prior Science 前端口仍被占用；拒绝接管未知 listener".into());
    }
    let (proxy_port, secret) = {
        let current = lock(state);
        if current.proxy.is_some() {
            (current.proxy_port, current.secret.clone())
        } else {
            (cfg.proxy_port, cfg.secret.clone())
        }
    };
    let ssh_hosts = if cfg.reuse_system_ssh {
        crate::runtime::ssh_bridge::validate_science_ssh_bridge(&sandbox_home())?
    } else {
        Vec::new()
    };
    let root = asset_root(app).ok_or("恢复 prior Science 时找不到打包资源")?;
    let launch = root.join("scripts/launch-virtual-sandbox.sh");
    if !launch.is_file() {
        return Err("恢复 prior Science 时启动脚本缺失".into());
    }
    let logf = open_log("sandbox.log").map_err(|error| error.to_string())?;
    let logf2 = logf.try_clone().map_err(|error| error.to_string())?;
    let proxy_url = format!("http://127.0.0.1:{proxy_port}/{secret}");
    let status = Command::new("zsh")
        .arg(&launch)
        .arg("--port")
        .arg(prior.port.to_string())
        .arg("--skip-oauth-forge")
        .env("SANDBOX_HOME", sandbox_home())
        .env("SCIENCE_BIN", &prior.runtime.path)
        .env("CSSWITCH_RUNTIME_VERSION_PRECHECKED", "1")
        .env("CSSWITCH_PROXY_URL", proxy_url)
        .env(
            "CSSWITCH_REUSE_SYSTEM_SSH",
            if cfg.reuse_system_ssh { "1" } else { "0" },
        )
        .env("CSSWITCH_SYSTEM_SSH_HOSTS", ssh_hosts.join(" "))
        .stdout(Stdio::from(logf))
        .stderr(Stdio::from(logf2))
        .status()
        .map_err(|error| format!("恢复 prior Science 启动失败：{error}"))?;
    if !status.success() {
        return Err(format!(
            "恢复 prior Science 启动脚本非零退出（{:?}）",
            status.code()
        ));
    }
    let mut healthy = false;
    for _ in 0..(operation::SANDBOX_HEALTH_BUDGET_MS / POLL_INTERVAL_MS) {
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        if proc::http_health(prior.port, None, operation::LOCAL_HEALTH_TIMEOUT_MS) {
            healthy = true;
            break;
        }
    }
    if !healthy || !sandbox_listener_matches_runtime(prior.port, &prior.runtime) {
        return Err("恢复 prior Science 后 listener 健康或 runtime 身份不一致".into());
    }
    let _candidate_token =
        crate::runtime::science::uncommitted_managed_science_launch_token(
            prior.port,
            &prior.runtime,
        )
        .ok_or("恢复 prior Science 后无法建立精确的未提交启动身份")?;
    #[cfg(test)]
    {
        let mut seams = SANDBOX_SESSION_TEST_SEAMS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if seams.prior_restart_post_spawn_failure_port == Some(prior.port) {
            let listener_pid = crate::runtime::science::test_unique_listener_pid(prior.port)
                .ok_or("test-only prior Science listener identity missing after verification")?;
            let process_start =
                crate::runtime::science::test_process_start_identity_for_pid(listener_pid)
                    .ok_or("test-only prior Science process-start identity missing")?;
            seams.prior_restart_post_spawn_identity = Some((listener_pid, process_start));
            drop(seams);
            let mut sandbox = None;
            let mut url = None;
            let cleanup = stop_sandbox_with_launch_token(
                app,
                &mut sandbox,
                &mut url,
                Some(&prior.runtime),
                Some(&_candidate_token),
            );
            return match cleanup {
                Ok(()) => Err("test-only prior Science post-spawn validation failure".into()),
                Err(_) => Err(
                    "prior_science_post_spawn_cleanup_failed：恢复 prior Science 的候选进程未能安全清理。"
                        .into(),
                ),
            };
        }
    }
    let token = match crate::runtime::science::record_managed_science_launch(
        prior.port,
        &prior.runtime,
    ) {
        Ok(token) => token,
        Err(error) => {
            let mut sandbox = None;
            let mut url = None;
            let cleanup = stop_sandbox_with_launch_token(
                app,
                &mut sandbox,
                &mut url,
                Some(&prior.runtime),
                error.token(),
            );
            return Err(format!(
                "恢复 prior Science 时 fresh managed receipt 提交失败：{}；cleanup={cleanup:?}",
                error.message()
            ));
        }
    };
    if !crate::runtime::science::managed_launch_token_is_current_for_runtime(
        &token,
        &prior.runtime,
    ) {
        let mut sandbox = None;
        let mut url = None;
        let _ = stop_sandbox_with_launch_token(
            app,
            &mut sandbox,
            &mut url,
            Some(&prior.runtime),
            Some(&token),
        );
        return Err("恢复 prior Science 后 fresh managed receipt 回读不一致".into());
    }
    let url = sandbox_url(prior.port, &prior.runtime);
    let mut current = lock(state);
    current.sandbox_port = prior.port;
    current.sandbox_url = Some(url);
    current.science_runtime = Some(prior.runtime.clone());
    current.science_confirmed_stopped = None;
    Ok(())
}

fn capture_authority_after_science_quiesce<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
    config_dir: &Path,
    sandbox_home: &Path,
    auth_dir: &Path,
    config: &config::Config,
    prior_science: Option<&PriorScienceContext>,
) -> Result<OneClickAuthoritySnapshot, String> {
    match OneClickAuthoritySnapshot::capture(
        config_dir,
        sandbox_home,
        auth_dir,
        config,
        state,
    ) {
        Ok(snapshot) => Ok(snapshot),
        Err(capture_error) => {
            if let Some(prior) = prior_science {
                match restart_prior_science(app, state, lifecycle, auth_proof, prior) {
                    Ok(()) => Err(capture_error),
                    Err(restart_error) => Err(format!(
                        "{capture_error}；prior_science_restart={restart_error}"
                    )),
                }
            } else {
                Err(capture_error)
            }
        }
    }
}

fn mark_stop_old_science_transaction(
    dir: &Path,
    active_profile_id: &str,
    previous_binding: Option<config::RuntimeBindingCommit>,
) -> Result<(), String> {
    config::update(dir, |current| {
        current.runtime_transaction = Some(config::RuntimeTransactionJournal {
            transaction_id: config::new_id(),
            target_profile_id: active_profile_id.to_string(),
            stage: "stop_old_science".into(),
            previous_binding: previous_binding.clone(),
            previous_gateway: None,
        });
    })
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn clear_runtime_transaction(dir: &Path) -> Result<(), String> {
    config::update(dir, |current| current.runtime_transaction = None)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn commit_runtime_binding(
    dir: &Path,
    binding: config::RuntimeBindingCommit,
) -> Result<(), String> {
    config::update(dir, |current| {
        current.runtime_binding = Some(binding.clone());
        current.runtime_transaction = None;
    })
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn history_recovery_choices(
    candidates: Vec<oauth_forge::HistoryOrgCandidate>,
) -> Result<(Vec<HistoryRecoveryChoice>, Vec<Value>), String> {
    if candidates.len() > 64 {
        return Err("历史记录候选超过安全上限（64），已拒绝生成恢复会话".into());
    }
    let choices = candidates
        .into_iter()
        .map(|candidate| HistoryRecoveryChoice {
            reference: config::new_id(),
            candidate,
        })
        .collect::<Vec<_>>();
    let visible = choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let label = if index < 26 {
                format!("历史记录 {}", (b'A' + index as u8) as char)
            } else {
                format!("历史记录 {}", index + 1)
            };
            json!({
                "reference": choice.reference,
                "label": label
            })
        })
        .collect();
    Ok((choices, visible))
}

fn compensate_one_click_failure<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
    dir: &Path,
    trace: &OperationTrace,
    authority_snapshot: &mut OneClickAuthoritySnapshot,
    prior_science: Option<&PriorScienceContext>,
    failure: OneClickFailure,
) -> Result<Value, String> {
    let cleanup = {
        let mut current = lock(state);
        let AppState {
            sandbox,
            sandbox_url,
            ..
        } = &mut *current;
        let result = stop_sandbox_with_launch_token(
            app,
            sandbox,
            sandbox_url,
            Some(&failure.rollback.launch_runtime),
            failure.rollback.launch_token.as_ref(),
        );
        if result.is_ok() {
            current.science_runtime = None;
            current.science_confirmed_stopped =
                Some(failure.rollback.launch_runtime.clone());
        }
        result
    };
    let ssh_cleanup = match failure.rollback.ssh_stub_transaction.as_ref() {
        Some(transaction) => transaction.compensate(&sandbox_home()),
        None => crate::runtime::settings::remove_managed_sandbox_ssh_stub(&sandbox_home()),
    };
    let rollback = authority_snapshot.restore_with_gateway(
        app,
        dir,
        state,
        lifecycle,
        auth_proof,
        failure.rollback.proxy_action,
    );
    let prior_restart = if rollback.is_ok() {
        prior_science
            .map(|prior| restart_prior_science(app, state, lifecycle, auth_proof, prior))
    } else {
        None
    };
    let authorities_restored = cleanup.is_ok()
        && ssh_cleanup.is_ok()
        && rollback.is_ok()
        && prior_restart.as_ref().is_none_or(Result::is_ok);
    let snapshot_cleanup = if authorities_restored {
        Some(authority_snapshot.cleanup_when_expendable())
    } else {
        authority_snapshot.preserve_recovery = true;
        None
    };
    trace.finish(if authorities_restored {
        "error=one_click_transaction_compensated"
    } else {
        "error=one_click_compensation_incomplete"
    });
    let mut codes = Vec::new();
    if cleanup.is_err() {
        codes.push("compensation_science_cleanup_failed".to_string());
    }
    if ssh_cleanup.is_err() {
        codes.push("compensation_ssh_cleanup_failed".to_string());
    }
    if rollback.is_err() {
        codes.push("compensation_restore_failed".to_string());
    }
    if let Some(Err(error)) = prior_restart {
        #[cfg(test)]
        if error.contains("test-only prior Science post-spawn validation failure") {
            codes.push("test-only prior Science post-spawn validation failure".to_string());
        } else {
            codes.push("compensation_prior_science_restart_failed".to_string());
        }
        #[cfg(not(test))]
        {
            let _ = error;
            codes.push("compensation_prior_science_restart_failed".to_string());
        }
    }
    if let Some(Err(error)) = snapshot_cleanup {
        if error.contains("recovery_status=cleanup_required") {
            codes.push(error);
        } else {
            codes.push("compensation_snapshot_register_failed".to_string());
        }
    }
    let suffix = (!codes.is_empty()).then(|| format!("；{}", codes.join("; ")));
    Err(format!("{}{}", failure.message, suffix.unwrap_or_default()))
}

fn healthy_reopen_with_gateway_rollback<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
    trace: &OperationTrace,
    dir: &Path,
    cfg: &config::Config,
    active_profile: &config::Profile,
    auth_dir: &Path,
    sport: u16,
    running_runtime: &ScienceRuntimeIdentity,
    open_surface: bool,
) -> Result<Value, String> {
    let app_snapshot = AppAuthoritySnapshot::capture(state);
    let prior_config = cfg.clone();
    let attempt = (|| -> Result<Value, String> {
        let (_pport, secret, proxy_action) = ensure_proxy(
            app,
            state,
            lifecycle,
            Some(running_runtime),
            Some(trace),
            auth_proof,
        )?;
        verify_gateway_model_catalog_traced(
            trace,
            cfg.proxy_port,
            &secret,
            active_profile,
        )?;
        let installer_bridge = skill_install_bridge_dir(&secret)?;
        let refreshed_cfg = config::load_from(dir).map_err(|error| error.to_string())?;
        let committed = crate::runtime::provider::desired_runtime_binding(
            &refreshed_cfg,
            refreshed_cfg
                .active_profile()
                .ok_or("生效 profile 在启动期间消失")?,
            running_runtime,
        )?;
        config::update(dir, |config| {
            config.runtime_binding = Some(committed.clone());
            config.runtime_transaction = None;
        })
        .map_err(|error| error.to_string())?;
        let installer = match current_skill_install_bridge_key() {
            Ok(installer_key) => inspect_while_science_running(
                app,
                auth_dir,
                &installer_bridge,
                &installer_key,
            ),
            Err(error) => RegistrationStatus::Warning(error),
        };
        let installer = configure_third_party_best_effort(
            app,
            installer,
            auth_dir,
            sport,
            running_runtime,
            false,
        );
        let url = sandbox_url(sport, running_runtime);
        {
            let mut current = lock(state);
            current.sandbox_port = sport;
            current.sandbox_url = Some(url.clone());
            current.science_runtime = Some(running_runtime.clone());
            current.science_confirmed_stopped = None;
        }
        let base = match proxy_action {
            ProxyAction::Reused => "已在运行",
            ProxyAction::Restarted => "已用新配置重启代理，Science 沿用不变",
        };
        let (message, fallback_url) = if open_surface {
            match open_science_surface(app, &url) {
                Ok("webview") => (format!("{base}，已重新打开 Science 窗口。"), None),
                Ok(_) => (format!("{base}，已向系统浏览器发送打开请求。"), None),
                Err(_) => (
                    format!("{base}，服务已就绪；自动打开失败。"),
                    Some(url.clone()),
                ),
            }
        } else {
            (format!("{base}，Science 绑定保持不变。"), None)
        };
        let message = append_installer_note(message, &installer);
        trace.finish(format!(
            "ok action=reopened proxy_action={}",
            proxy_action.as_str()
        ));
        Ok(json!({
            "msg": message,
            "action": "reopened",
            "stage": "complete",
            "status": "ok",
            "recovery_status": "not_needed",
            "fallback_url": fallback_url,
            "external_skill_installer": installer_status_json(&installer)
        }))
    })();
    match attempt {
        Ok(value) => Ok(value),
        Err(primary) => {
            let mut recovery_errors = Vec::new();
            if let Err(error) =
                config::save_to(dir, &prior_config).map_err(|error| error.to_string())
            {
                recovery_errors.push(format!("config={error}"));
            }
            if let Err(error) = app_snapshot.restore_with_gateway(
                app,
                state,
                lifecycle,
                auth_proof,
                ProxyAction::Restarted,
            ) {
                recovery_errors.push(format!("gateway={error}"));
            }
            if recovery_errors.is_empty() {
                Err(primary)
            } else {
                Err(format!(
                    "{primary}；healthy_reopen_recovery={}",
                    recovery_errors.join("; ")
                ))
            }
        }
    }
}

fn one_click_login_with_options<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: SharedAppState,
    lifecycle: &lifecycle::Lifecycle,
    runtime_choice: Option<&str>,
    auth_proof: Option<&crate::codex_auth_supervisor::CodexAuthReadyProof>,
    open_surface: bool,
) -> Result<Value, String> {
    let trace = OperationTrace::start(OperationKind::OneClickLogin, "command=one_click_login");
    let dir = config::default_dir();
    let cfg = config::load_from(&dir).map_err(|e| e.to_string())?;
    let active_profile = cfg
        .active_profile()
        .ok_or("未配置生效 profile，请先在面板选择或新建一条配置。")?;
    config::require_template_enabled(&cfg, &active_profile.template_id)?;
    let active_launch = crate::runtime::provider::resolve_launch_plan(active_profile)?;
    crate::commands::codex::require_provider_auth_proof(&active_launch.adapter, auth_proof)?;
    crate::runtime::settings::validate_runtime_ports(cfg.proxy_port, cfg.sandbox_port)?;
    let sport = cfg.sandbox_port;

    let sbx_home = sandbox_home();
    let auth_dir = sbx_home.join(".claude-science");
    let ssh_prevalidation =
        crate::runtime::sandbox_session::prevalidate_one_click_system_ssh(&app, &cfg, &sbx_home)?;
    let ssh_stub_transaction = cfg
        .reuse_system_ssh
        .then(|| {
            crate::runtime::settings::ManagedSshStubTransaction::capture(
                &sbx_home,
                &ssh_prevalidation,
            )
        })
        .transpose()?;
    retry_pending_authority_cleanup(&state)?;
    let version_cache = { lock(&state).science_version_cache.clone() };

    let (remembered_runtime, confirmed_stopped) = {
        let st = lock(&state);
        (
            st.science_runtime.clone(),
            st.science_confirmed_stopped.clone(),
        )
    };
    let (science_state, running_runtime) = match remembered_runtime {
        Some(runtime) => {
            let science_state = probe_known_runtime(sport, &runtime);
            let running_runtime =
                (science_state == SandboxScienceState::RunningHealthy).then_some(runtime);
            (science_state, running_runtime)
        }
        None if confirmed_stopped
            .as_ref()
            .is_some_and(|runtime| runtime.source != ScienceRuntimeSource::CachedOnce)
            && !proc::loopback_port_in_use(sport, 100) =>
        {
            (SandboxScienceState::Stopped, None)
        }
        None => probe_sandbox_runtime_cached(sport, &version_cache)?,
    };
    let mut running_runtime_to_stop = None;
    let launch_runtime: ScienceRuntimeIdentity = match science_state {
        SandboxScienceState::RunningHealthy => {
            let running_runtime =
                running_runtime.ok_or("Science 状态为运行中，但无法确认其 binary 身份")?;
            let desired_binding = crate::runtime::provider::desired_runtime_binding(
                &cfg,
                active_profile,
                &running_runtime,
            )?;
            let science_binding_matches = !crate::runtime::provider::science_restart_required(
                cfg.runtime_binding.as_ref(),
                &desired_binding,
            );
            let login_intact =
                oauth_forge::login_intact(&auth_dir, "virtual@localhost.invalid", &sbx_home);
            if login_intact && science_binding_matches {
                if cfg.reuse_system_ssh {
                    validate_running_system_ssh_bridge(&app, &sbx_home)?;
                }
                oauth_forge::bootstrap_marker_for_intact_login(
                    &auth_dir,
                    "virtual@localhost.invalid",
                    &sbx_home,
                )
                .map_err(|error| format!("补齐历史恢复标记失败：{error}"))?;
                return healthy_reopen_with_gateway_rollback(
                    &app,
                    &state,
                    lifecycle,
                    auth_proof,
                    &trace,
                    &dir,
                    &cfg,
                    active_profile,
                    &auth_dir,
                    sport,
                    &running_runtime,
                    open_surface,
                );
            }
            let selected = if login_intact {
                running_runtime
            } else {
                select_science_runtime_cached(runtime_choice, &version_cache)?
            };
            running_runtime_to_stop = Some(selected.clone());
            selected
        }
        SandboxScienceState::Stopped => {
            select_science_runtime_cached(runtime_choice, &version_cache)?
        }
        SandboxScienceState::Unknown => {
            trace.finish("error=sandbox_state_unknown_before_start");
            return Err(format!(
                "无法确认隔离 Science 状态（端口 {sport} 或 data-dir 状态不一致）。请先停止占用该端口的进程后重试。"
            ));
        }
    };
    let mut rollback_context = OneClickRollbackContext {
        proxy_action: ProxyAction::Reused,
        launch_runtime: launch_runtime.clone(),
        launch_token: None,
        ssh_stub_transaction,
    };
    let prior_science = match running_runtime_to_stop.as_ref() {
        Some(runtime) => Some(PriorScienceContext {
            runtime: runtime.clone(),
            port: sport,
            launch_token: crate::runtime::science::managed_launch_token_for_runtime(
                sport,
                runtime,
            )
            .ok_or("prior Science managed launch 身份无法确认，拒绝停止或快照")?,
        }),
        None => None,
    };
    if let Some(prior) = prior_science.as_ref() {
        {
            let mut current = lock(&state);
            let AppState {
                sandbox,
                sandbox_url,
                ..
            } = &mut *current;
            stop_sandbox_with_launch_token(
                &app,
                sandbox,
                sandbox_url,
                Some(&prior.runtime),
                Some(&prior.launch_token),
            )?;
            current.science_runtime = None;
            current.science_confirmed_stopped = Some(prior.runtime.clone());
        }
        let receipt = dir.join("science-managed-launch.v1.json");
        if proc::loopback_port_in_use(sport, operation::LOCAL_HEALTH_TIMEOUT_MS)
            || crate::runtime::science::managed_launch_token_process_is_alive(
                &prior.launch_token,
            )
            || receipt.exists()
        {
            let restart = restart_prior_science(
                &app,
                &state,
                lifecycle,
                auth_proof,
                prior,
            );
            return Err(format!(
                "prior Science 未完成 verified stop，拒绝建立 authority 快照；restart={restart:?}"
            ));
        }
    }
    let prior_science_for_compensation = prior_science.as_ref();
    let mut authority_snapshot = capture_authority_after_science_quiesce(
        &app,
        &state,
        lifecycle,
        auth_proof,
        &dir,
        &sbx_home,
        &auth_dir,
        &cfg,
        prior_science_for_compensation,
    )?;
    let transaction_result = (|| -> Result<Value, OneClickFailure> {
        if running_runtime_to_stop.is_some() {
            one_click_step(
                mark_stop_old_science_transaction(
                    &dir,
                    &active_profile.id,
                    cfg.runtime_binding.clone(),
                ),
                &rollback_context,
            )?;
        }
        let transaction_cfg =
            one_click_step(config::load_from(&dir), &rollback_context)?;
        one_click_step(
            advance_runtime_transaction(
                &dir,
                &active_profile.id,
                transaction_cfg.runtime_binding.clone(),
                "start_gateway",
            ),
            &rollback_context,
        )?;
        let preview_port = match sport.checked_add(1) {
            Some(port) => port,
            None => {
                return Err(rollback_context
                    .failure("沙箱端口必须小于 65535，才能分配隔离预览端口。"))
            }
        };
        if proc::loopback_port_in_use(preview_port, operation::LOCAL_HEALTH_TIMEOUT_MS) {
            return Err(rollback_context.failure(format!(
                "隔离 Science 预览端口 {preview_port} 已被占用；未启动或结束任何占用者。请修改沙箱端口后重试。"
            )));
        }
        lock(&state).science_confirmed_stopped = None;

        trace.stage(OperationStage::SandboxLogin, "ensure_virtual_login");
        let (forged, login_action) = match oauth_forge::ensure_virtual_login(
            &auth_dir,
            "virtual@localhost.invalid",
            &sbx_home,
        ) {
            Ok(result) => result,
            Err(oauth_forge::EnsureVirtualLoginError::HistoryChoiceRequired(candidates)) => {
                let (choices, visible_choices) = one_click_step(
                    history_recovery_choices(candidates),
                    &rollback_context,
                )?;
                {
                    let mut app_state = lock(&state);
                    app_state.science_confirmed_stopped = Some(launch_runtime.clone());
                    app_state.history_recovery = Some(HistoryRecoverySession {
                        active_profile_id: active_profile.id.clone(),
                        sandbox_port: sport,
                        auth_dir: auth_dir.clone(),
                        sandbox_root: sbx_home.clone(),
                        choices,
                    });
                }
                one_click_step(clear_runtime_transaction(&dir), &rollback_context)?;
                trace.finish("attention=history_choice_required");
                let mut value = json!({
                    "msg": "检测到多份旧历史记录。请选择要恢复的一份；CSSwitch 不会删除其他记录。",
                    "action": "history_choice_required",
                    "stage": "history_recovery",
                    "status": "attention",
                    "recovery_status": "choice_required",
                    "choices": visible_choices,
                    "fallback_url": null
                });
                one_click_step(
                    authority_snapshot.prepare_success(&mut value),
                    &rollback_context,
                )?;
                return Ok(value);
            }
            Err(oauth_forge::EnsureVirtualLoginError::Message(message)) => {
                return Err(rollback_context.failure(format!("写虚拟登录失败：{message}")));
            }
        };
        let _validated_login_identity = (
            &forged.auth_dir,
            &forged.account_uuid,
            &forged.org_uuid,
            &forged.enc_file,
        );
        let root = match asset_root(&app) {
            Some(root) => root,
            None => {
                return Err(rollback_context.failure(
                    "找不到 scripts/launch-virtual-sandbox.sh（打包资源或仓库根均未命中）。",
                ))
            }
        };
        let launch = root.join("scripts/launch-virtual-sandbox.sh");
        if !launch.is_file() {
            return Err(
                rollback_context.failure("找不到 scripts/launch-virtual-sandbox.sh。")
            );
        }
        let ssh_hosts = if cfg.reuse_system_ssh {
            one_click_step(
                crate::runtime::ssh_bridge::prepare_science_ssh_bridge(&sbx_home),
                &rollback_context,
            )?
        } else {
            one_click_step(
                crate::runtime::ssh_bridge::revoke_science_ssh_bridge(&sbx_home),
                &rollback_context,
            )?;
            Vec::new()
        };
        let (pport, secret, proxy_action) = one_click_step(
            ensure_proxy(
                &app,
                &state,
                lifecycle,
                Some(&launch_runtime),
                Some(&trace),
                auth_proof,
            ),
            &rollback_context,
        )?;
        rollback_context.proxy_action = proxy_action;
        one_click_step(
            verify_gateway_model_catalog_traced(&trace, pport, &secret, active_profile),
            &rollback_context,
        )?;
        one_click_step(
            advance_runtime_transaction(
                &dir,
                &active_profile.id,
                transaction_cfg.runtime_binding.clone(),
                "start_science",
            ),
            &rollback_context,
        )?;
        let installer_bridge =
            one_click_step(skill_install_bridge_dir(&secret), &rollback_context)?;
        let installer = match current_skill_install_bridge_key() {
            Ok(installer_key) => {
                register_before_science_start(&app, &auth_dir, &installer_bridge, &installer_key)
            }
            Err(error) => RegistrationStatus::Warning(error),
        };
        let proxy_url = format!("http://127.0.0.1:{pport}/{secret}");
        let logf = match open_log("sandbox.log") {
            Ok(file) => file,
            Err(error) => {
                return Err(rollback_context.failure(format!("建日志失败：{error}")))
            }
        };
        {
            use std::io::Write;
            let mut writer = &logf;
            let _ = writeln!(
                writer,
                "[oauth] 虚拟登录已就绪（Rust，零 node；action={:?}；isolated=true）",
                login_action
            );
        }
        let logf2 = one_click_step(logf.try_clone(), &rollback_context)?;
        trace.stage(OperationStage::SandboxLaunch, format!("port={sport}"));
        if !runtime_identity_is_current(&launch_runtime) {
            return Err(
                rollback_context.failure("Science runtime 在预检后发生变化；已拒绝启动，请重试")
            );
        }
        #[cfg(test)]
        if let Some(foreign_stub) =
            std::env::var_os("CSSWITCH_TEST_SSH_LATE_FOREIGN_STUB").map(std::path::PathBuf::from)
        {
            let parent = match foreign_stub.parent() {
                Some(parent) => parent,
                None => {
                    return Err(
                        rollback_context.failure("SSH late-failure test stub has no parent")
                    )
                }
            };
            one_click_step(std::fs::create_dir_all(parent), &rollback_context)?;
            one_click_step(
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)),
                &rollback_context,
            )?;
            let mut file = one_click_step(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&foreign_stub),
                &rollback_context,
            )?;
            one_click_step(
                std::io::Write::write_all(
                    &mut file,
                    b"foreign-test-stub-must-survive\n",
                ),
                &rollback_context,
            )?;
            one_click_step(file.sync_all(), &rollback_context)?;
        }
        let status_result = Command::new("zsh")
            .arg(&launch)
            .arg("--port")
            .arg(sport.to_string())
            .arg("--skip-oauth-forge")
            .env("SANDBOX_HOME", sandbox_home())
            .env("SCIENCE_BIN", &launch_runtime.path)
            .env("CSSWITCH_RUNTIME_VERSION_PRECHECKED", "1")
            .env("CSSWITCH_PROXY_URL", &proxy_url)
            .env(
                "CSSWITCH_REUSE_SYSTEM_SSH",
                if cfg.reuse_system_ssh { "1" } else { "0" },
            )
            .env("CSSWITCH_SYSTEM_SSH_HOSTS", ssh_hosts.join(" "))
            .stdout(Stdio::from(logf))
            .stderr(Stdio::from(logf2))
            .status();
        if status_result.is_ok() {
            if let Some(transaction) = rollback_context.ssh_stub_transaction.as_mut() {
                transaction.observe_after_launch(&sbx_home);
            }
        }
        let status = match status_result {
            Ok(status) => status,
            Err(error) => {
                return Err(rollback_context.failure(format!("起沙箱失败：{error}")))
            }
        };
        if !status.success() {
            let tail = redact(&tail_file(&log_path("sandbox.log"), 600), &secret);
            return Err(
                rollback_context.failure(format!("起沙箱脚本失败。\n{tail}"))
            );
        }
        {
            let mut current = lock(&state);
            current.sandbox_port = sport;
            current.science_runtime = Some(launch_runtime.clone());
            current.science_confirmed_stopped = None;
        }
        let mut healthy = false;
        for _ in 0..(operation::SANDBOX_HEALTH_BUDGET_MS / POLL_INTERVAL_MS) {
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            if proc::http_health(sport, None, operation::LOCAL_HEALTH_TIMEOUT_MS) {
                healthy = true;
                break;
            }
        }
        trace.stage(
            OperationStage::SandboxHealth,
            if healthy { "ready" } else { "not_ready" },
        );
        if !healthy {
            let tail = redact(&tail_file(&log_path("sandbox.log"), 600), &secret);
            return Err(rollback_context.failure(format!(
                "沙箱起后探活超时（端口 {sport}）。\n{tail}"
            )));
        }
        if !sandbox_listener_matches_runtime(sport, &launch_runtime) {
            return Err(rollback_context.failure(format!(
                "端口 {sport} 有服务响应，但按 data-dir 确认不是本沙箱 Science（疑似被其它服务占用）。"
            )));
        }
        match crate::runtime::science::record_managed_science_launch(sport, &launch_runtime) {
            Ok(token) => rollback_context.launch_token = Some(token),
            Err(error) => {
                rollback_context.launch_token = error.token().cloned();
                return Err(rollback_context.failure(format!(
                    "Science 已启动但受管启动身份无法安全提交：{}",
                    error.message()
                )));
            }
        }
        one_click_step(
            advance_runtime_transaction(
                &dir,
                &active_profile.id,
                transaction_cfg.runtime_binding.clone(),
                "verify_science_catalog",
            ),
            &rollback_context,
        )?;
        let installer = configure_third_party_best_effort(
            &app,
            installer,
            &auth_dir,
            sport,
            &launch_runtime,
            false,
        );
        let url = sandbox_url(sport, &launch_runtime);
        {
            let mut current = lock(&state);
            current.sandbox_port = sport;
            current.sandbox_url = Some(url.clone());
            current.science_runtime = Some(launch_runtime.clone());
            current.science_confirmed_stopped = None;
        }
        let started = match login_action {
            oauth_forge::LoginAction::Created => "已启动",
            _ => "沙箱已重新启动，沿用原有对话",
        };
        let refreshed_cfg =
            one_click_step(config::load_from(&dir), &rollback_context)?;
        let refreshed_profile = match refreshed_cfg.active_profile() {
            Some(profile) => profile,
            None => {
                return Err(rollback_context.failure("生效 profile 在启动期间消失"))
            }
        };
        let committed = one_click_step(
            crate::runtime::provider::desired_runtime_binding(
                &refreshed_cfg,
                refreshed_profile,
                &launch_runtime,
            ),
            &rollback_context,
        )?;
        one_click_step(
            commit_runtime_binding(&dir, committed),
            &rollback_context,
        )?;
        let (message, fallback_url) = if open_surface {
            match open_science_surface(&app, &url) {
                Ok("webview") => (format!("{started}，已打开 Science 窗口。"), None),
                Ok(_) => (format!("{started}，已向系统浏览器发送打开请求。"), None),
                Err(_) => (
                    format!("{started}，服务已就绪；自动打开失败。"),
                    Some(url.clone()),
                ),
            }
        } else {
            (format!("{started}，Science 已按新模型目录刷新。"), None)
        };
        let message = append_installer_note(message, &installer);
        trace.stage(OperationStage::OpenBrowser, "done");
        trace.finish(format!(
            "ok action=started proxy_action={}",
            proxy_action.as_str()
        ));
        let mut value = json!({
            "msg": message,
            "action": "started",
            "stage": "complete",
            "status": "ok",
            "recovery_status": "not_needed",
            "fallback_url": fallback_url,
            "external_skill_installer": installer_status_json(&installer)
        });
        one_click_step(
            authority_snapshot.prepare_success(&mut value),
            &rollback_context,
        )?;
        Ok(value)
    })();
    match transaction_result {
        Ok(value) => {
            authority_snapshot.commit();
            Ok(value)
        }
        Err(failure) => compensate_one_click_failure(
            &app,
            &state,
            lifecycle,
            auth_proof,
            &dir,
            &trace,
            &mut authority_snapshot,
            prior_science_for_compensation,
            failure,
        ),
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::{
        advance_runtime_transaction, gateway_model_catalog_timeout_ms,
        prevalidate_one_click_system_ssh, test_arm_authority_snapshot_capture_failure,
        test_arm_authority_snapshot_cleanup_fault, test_arm_authority_snapshot_directory_barrier,
        verify_gateway_model_catalog, AuthorityTreeSnapshot, OneClickAuthoritySnapshot,
    };
    use crate::config::{self, Config, RuntimeBindingCommit};
    use crate::provider_contracts::ModelPolicy;
    use crate::runtime::proxy::ProxyAction;
    use crate::{AppState, SharedAppState};
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct ScopedEnv {
        saved: Vec<(String, Option<std::ffi::OsString>)>,
    }

    impl ScopedEnv {
        fn new() -> Self {
            Self { saved: Vec::new() }
        }

        fn set(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) {
            self.saved.push((key.to_string(), std::env::var_os(key)));
            std::env::set_var(key, value);
        }

        fn remove(&mut self, key: &str) {
            self.saved.push((key.to_string(), std::env::var_os(key)));
            std::env::remove_var(key);
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            for (key, value) in self.saved.iter().rev() {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TreeEntry {
        kind: &'static str,
        mode: u32,
        bytes: Vec<u8>,
    }

    fn isolated_tmpdir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "csswitch-sandbox-transaction-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    fn tree(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
        fn walk(root: &Path, current: &Path, entries: &mut BTreeMap<PathBuf, TreeEntry>) {
            let metadata = match fs::symlink_metadata(current) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => panic!("cannot inspect {}: {error}", current.display()),
            };
            let relative = current.strip_prefix(root).unwrap().to_path_buf();
            if metadata.file_type().is_symlink() {
                entries.insert(
                    relative,
                    TreeEntry {
                        kind: "symlink",
                        mode: metadata.permissions().mode() & 0o777,
                        bytes: fs::read_link(current)
                            .unwrap()
                            .as_os_str()
                            .as_encoded_bytes()
                            .to_vec(),
                    },
                );
            } else if metadata.is_file() {
                entries.insert(
                    relative,
                    TreeEntry {
                        kind: "file",
                        mode: metadata.permissions().mode() & 0o777,
                        bytes: fs::read(current).unwrap(),
                    },
                );
            } else {
                assert!(metadata.is_dir(), "fixture contains a special file");
                entries.insert(
                    relative,
                    TreeEntry {
                        kind: "dir",
                        mode: metadata.permissions().mode() & 0o777,
                        bytes: Vec::new(),
                    },
                );
                let mut children = fs::read_dir(current)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect::<Vec<_>>();
                children.sort();
                for child in children {
                    walk(root, &child, entries);
                }
            }
        }

        let mut entries = BTreeMap::new();
        walk(root, root, &mut entries);
        entries
    }

    #[test]
    fn authority_snapshot_uses_independent_inodes_and_restores_in_place_mutation() {
        let tmp = isolated_tmpdir("independent-inodes");
        let source = tmp.join("authority");
        let backup = tmp.join("rollback/authority");
        fs::create_dir_all(source.join("nested/empty")).unwrap();
        fs::create_dir(tmp.join("rollback")).unwrap();
        fs::write(source.join("database.db"), b"prior-database-bytes\n").unwrap();
        fs::write(source.join("nested/state.json"), br#"{"prior":true}"#).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(
            source.join("database.db"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::set_permissions(
            source.join("nested/state.json"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        let before = tree(&source);

        let mut snapshot =
            AuthorityTreeSnapshot::capture(source.clone(), backup.clone()).unwrap();
        for relative in [Path::new("database.db"), Path::new("nested/state.json")] {
            let live = fs::metadata(source.join(relative)).unwrap();
            let saved = fs::metadata(backup.join(relative)).unwrap();
            assert_eq!(
                live.dev(),
                saved.dev(),
                "transaction snapshot must stay on the same isolated filesystem"
            );
            assert_ne!(
                live.ino(),
                saved.ino(),
                "transaction snapshot must never share a mutable inode with live authority"
            );
        }
        assert_eq!(tree(&backup), before);

        let mut database = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(source.join("database.db"))
            .unwrap();
        database.write_all(b"mutated-in-place\n").unwrap();
        database.sync_all().unwrap();
        fs::set_permissions(
            source.join("database.db"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        fs::remove_file(source.join("nested/state.json")).unwrap();
        fs::write(source.join("nested/new-authority"), b"must disappear\n").unwrap();
        fs::create_dir(source.join("new-directory")).unwrap();

        snapshot.restore().unwrap();
        assert_eq!(
            tree(&source),
            before,
            "restore must recover exact bytes, modes, empty directories, and object set"
        );
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn authority_snapshot_fails_closed_when_directory_membership_changes_mid_capture() {
        let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let tmp = isolated_tmpdir("directory-membership-race");
        let source = tmp.join("authority");
        let backup = tmp.join("rollback/authority");
        let barrier = tmp.join("barrier");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir(tmp.join("rollback")).unwrap();
        fs::write(source.join("database.db"), b"prior-database\n").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(
            source.join("database.db"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let _seam =
            test_arm_authority_snapshot_directory_barrier(source.clone(), barrier.clone());

        let capture_source = source.clone();
        let capture_backup = backup.clone();
        let worker =
            thread::spawn(move || AuthorityTreeSnapshot::capture(capture_source, capture_backup));
        for _ in 0..200 {
            if barrier.join("ready").is_file() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            barrier.join("ready").is_file(),
            "test-only barrier must observe the directory enumeration boundary"
        );
        fs::write(source.join("database.db-wal"), b"concurrent-wal\n").unwrap();
        fs::set_permissions(
            source.join("database.db-wal"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::write(barrier.join("release"), b"release\n").unwrap();
        let capture = worker.join().unwrap();
        let accepted_torn_tree = capture.is_ok()
            && backup.join("database.db").is_file()
            && !backup.join("database.db-wal").exists();
        let _ = fs::remove_dir_all(&tmp);

        assert!(
            capture.is_err(),
            "authority snapshot must fail closed when a DB/WAL directory entry appears after enumeration; accepted_torn_tree={accepted_torn_tree}"
        );
    }

    #[test]
    fn one_shot_commit_cleanup_fault_is_retried_before_success() {
        let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let tmp = isolated_tmpdir("commit-cleanup-once");
        let config_dir = tmp.join("config");
        let sandbox_home = tmp.join("sandbox/home");
        let auth_dir = sandbox_home.join(".claude-science");
        let cleanup_log = tmp.join("cleanup.log");
        fs::create_dir_all(&auth_dir).unwrap();
        fs::write(auth_dir.join("auth.json"), b"private-authority\n").unwrap();
        fs::set_permissions(
            auth_dir.join("auth.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let config = Config::default();
        let state: SharedAppState = Arc::new(Mutex::new(AppState::default()));
        let _seam = test_arm_authority_snapshot_cleanup_fault(
            tmp.clone(),
            "once",
            cleanup_log.clone(),
        );
        let mut snapshot = OneClickAuthoritySnapshot::capture(
            &config_dir,
            &sandbox_home,
            &auth_dir,
            &config,
            &state,
        )
        .unwrap();
        let backup_root = snapshot.backup_root.clone();
        snapshot.commit();
        let cleanup_attempts = fs::read_to_string(&cleanup_log)
            .unwrap_or_default()
            .lines()
            .count();
        let root_removed_before_success = !backup_root.exists();
        if backup_root.exists() {
            fs::remove_dir_all(&backup_root).unwrap();
        }
        let _ = fs::remove_dir_all(&tmp);

        assert!(
            root_removed_before_success && cleanup_attempts >= 2,
            "one-shot cleanup fault must be retried before commit reports success: attempts={cleanup_attempts}, root_removed={root_removed_before_success}"
        );
    }

    #[test]
    fn pending_cleanup_observer_mapping_only_does_not_claim_durable_cleanup() {
        let managed_id = ".one-click-rollback-0123456789abcdef0123456789abcdef";
        let identity = config::PendingCleanupIdentity {
            managed_id: managed_id.to_string(),
            path: PathBuf::from("/synthetic/mapping-only").join(managed_id),
            device: 41,
            inode: 73,
            marker: managed_id.to_string(),
        };
        let different = config::PendingCleanupIdentity {
            inode: 74,
            ..identity.clone()
        };
        let _lifecycle = config::test_arm_pending_cleanup_lifecycle(None);
        config::test_observe_pending_cleanup_manifest_validated(identity.clone());

        config::test_observe_pending_cleanup_initial_ticket(
            config::PendingCleanupInitialTicket::Present(identity.clone()),
        );
        config::test_observe_pending_cleanup_completion(
            config::PendingCleanupRemovalOutcome::Removed,
            config::PendingCleanupFinalState::NotFound,
        );
        config::test_observe_pending_cleanup_initial_ticket(
            config::PendingCleanupInitialTicket::Missing(identity.clone()),
        );
        config::test_observe_pending_cleanup_completion(
            config::PendingCleanupRemovalOutcome::AlreadyAbsent,
            config::PendingCleanupFinalState::NotFound,
        );

        for (ticket, outcome, final_state) in [
            (
                config::PendingCleanupInitialTicket::Present(identity.clone()),
                config::PendingCleanupRemovalOutcome::AlreadyAbsent,
                config::PendingCleanupFinalState::NotFound,
            ),
            (
                config::PendingCleanupInitialTicket::Missing(identity.clone()),
                config::PendingCleanupRemovalOutcome::Removed,
                config::PendingCleanupFinalState::NotFound,
            ),
            (
                config::PendingCleanupInitialTicket::Present(identity.clone()),
                config::PendingCleanupRemovalOutcome::Error,
                config::PendingCleanupFinalState::Error,
            ),
            (
                config::PendingCleanupInitialTicket::Present(identity.clone()),
                config::PendingCleanupRemovalOutcome::Removed,
                config::PendingCleanupFinalState::Present(different.clone()),
            ),
        ] {
            config::test_observe_pending_cleanup_initial_ticket(ticket);
            config::test_observe_pending_cleanup_completion(outcome, final_state);
        }

        config::test_observe_pending_cleanup_initial_ticket(
            config::PendingCleanupInitialTicket::Present(identity.clone()),
        );
        config::test_observe_pending_cleanup_completion(
            config::PendingCleanupRemovalOutcome::Removed,
            config::PendingCleanupFinalState::NotFound,
        );
        let observation = config::test_pending_cleanup_lifecycle_observation();
        assert_eq!(
            observation.events,
            vec![
                config::PendingCleanupLifecycleEvent::Register(identity.clone()),
                config::PendingCleanupLifecycleEvent::Remove {
                    identity: identity.clone(),
                    not_found: false,
                },
                config::PendingCleanupLifecycleEvent::Remove {
                    identity: identity.clone(),
                    not_found: true,
                },
                config::PendingCleanupLifecycleEvent::Remove {
                    identity,
                    not_found: false,
                },
            ],
            "mapping-only seam self-test must preserve exact Present/Removed=false and Missing/AlreadyAbsent=true outcomes without deduplication"
        );
        assert_eq!(
            observation.causal_mismatch_count, 4,
            "Present+AlreadyAbsent, Missing+Removed, error, and final Present are causal mismatches and must emit zero Remove"
        );
        assert_eq!(observation.completion_count, 7);
    }

    #[test]
    fn partial_capture_cleanup_failure_returns_tracked_degraded_recovery() {
        let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let tmp = isolated_tmpdir("partial-capture-cleanup");
        let config_dir = tmp.join("config");
        let sandbox_home = tmp.join("sandbox/home");
        let auth_dir = sandbox_home.join(".claude-science");
        let cleanup_log = tmp.join("cleanup.log");
        fs::create_dir_all(&auth_dir).unwrap();
        fs::write(auth_dir.join("auth.json"), b"private-authority\n").unwrap();
        fs::set_permissions(
            auth_dir.join("auth.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let config = Config::default();
        let state: SharedAppState = Arc::new(Mutex::new(AppState::default()));
        let _capture_seam = test_arm_authority_snapshot_capture_failure(
            sandbox_home.parent().unwrap().join("state"),
        );
        let _cleanup_seam = test_arm_authority_snapshot_cleanup_fault(
            tmp.clone(),
            "persistent",
            cleanup_log.clone(),
        );
        let failure = OneClickAuthoritySnapshot::capture(
            &config_dir,
            &sandbox_home,
            &auth_dir,
            &config,
            &state,
        )
        .err()
        .expect("partial capture fault must fail");
        let cleanup_line = fs::read_to_string(&cleanup_log)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        let backup_root = PathBuf::from(
            cleanup_line
                .split('\t')
                .nth(2)
                .expect("cleanup observation must track the exact root"),
        );
        let degraded_and_tracked = (failure.contains("cleanup_required")
            || failure.contains("degraded"))
            && failure.contains(&backup_root.to_string_lossy().to_string())
            && backup_root.exists();
        if backup_root.exists() {
            fs::remove_dir_all(&backup_root).unwrap();
        }
        let _ = fs::remove_dir_all(&tmp);

        assert!(
            degraded_and_tracked,
            "partial-capture cleanup failure must return explicit degraded cleanup_required state with the exact residual path: failure={failure:?}, root={}",
            backup_root.display()
        );
    }

    #[test]
    fn rollback_refusal_restores_independent_authorities_and_preserves_recovery_snapshot() {
        let tmp = isolated_tmpdir("rollback-refusal");
        let config_dir = tmp.join("config");
        let sandbox_home = tmp.join("sandbox/home");
        let auth_dir = sandbox_home.join(".claude-science");
        let private_state = sandbox_home.parent().unwrap().join("state");
        let runtime_dir = config_dir.join("runtime");
        let receipt = config_dir.join("science-managed-launch.v1.json");
        fs::create_dir_all(&auth_dir).unwrap();
        fs::create_dir_all(&private_state).unwrap();
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(auth_dir.join("auth.json"), b"prior-auth\n").unwrap();
        fs::write(private_state.join("private.json"), b"prior-private\n").unwrap();
        fs::write(runtime_dir.join("bridge.key"), b"prior-runtime\n").unwrap();
        fs::write(&receipt, b"prior-receipt\n").unwrap();
        for path in [
            auth_dir.join("auth.json"),
            private_state.join("private.json"),
            runtime_dir.join("bridge.key"),
            receipt.clone(),
        ] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let config = Config::default();
        config::save_to(&config_dir, &config).unwrap();
        let state: SharedAppState = Arc::new(Mutex::new(AppState::default()));
        let mut snapshot = OneClickAuthoritySnapshot::capture(
            &config_dir,
            &sandbox_home,
            &auth_dir,
            &config,
            &state,
        )
        .unwrap();
        let backup_root = snapshot.backup_root.clone();
        let auth_before = tree(&auth_dir);
        let private_before = tree(&private_state);
        let runtime_before = tree(&runtime_dir);
        let receipt_before = tree(&receipt);
        let config_before = config::load_from(&config_dir).unwrap();

        fs::remove_dir_all(&auth_dir).unwrap();
        let foreign = tmp.join("foreign-target");
        fs::write(&foreign, b"foreign-must-not-change\n").unwrap();
        fs::set_permissions(&foreign, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&foreign, &auth_dir).unwrap();
        fs::write(private_state.join("private.json"), b"mutated-private\n").unwrap();
        fs::write(runtime_dir.join("bridge.key"), b"mutated-runtime\n").unwrap();
        fs::write(&receipt, b"mutated-receipt\n").unwrap();
        config::update(&config_dir, |current| {
            current.proxy_port = 54321;
            current.secret = "candidate-config-secret".into();
            current.reuse_system_ssh = true;
        })
        .unwrap();
        {
            let mut app = state.lock().unwrap();
            app.proxy_port = 54321;
            app.secret = "candidate-app-secret".into();
            app.provider = "candidate-provider".into();
            app.gateway_kind = "candidate-gateway".into();
            app.shim_mode = "candidate-shim".into();
            app.launch_id = "candidate-launch".into();
            app.key_fp = 54321;
            app.sandbox_port = 54322;
            app.sandbox_url = Some("http://127.0.0.1:54322/candidate".into());
        }

        let error = snapshot
            .restore(&config_dir, &state, ProxyAction::Reused)
            .unwrap_err();
        let refused_without_following = error.contains("变成符号链接，拒绝跟随")
            && fs::read(&foreign).unwrap() == b"foreign-must-not-change\n"
            && fs::symlink_metadata(&auth_dir)
                .unwrap()
                .file_type()
                .is_symlink();
        let independent_authorities_restored = tree(&private_state) == private_before
            && tree(&runtime_dir) == runtime_before
            && tree(&receipt) == receipt_before;
        let config_restored = config::load_from(&config_dir).unwrap() == config_before;
        let app_restored = {
            let app = state.lock().unwrap();
            app.proxy.is_none()
                && app.proxy_port == 0
                && app.secret.is_empty()
                && app.provider.is_empty()
                && app.gateway_kind.is_empty()
                && app.shim_mode.is_empty()
                && app.launch_id.is_empty()
                && app.key_fp == 0
                && app.sandbox.is_none()
                && app.sandbox_port == 0
                && app.sandbox_url.is_none()
        };
        drop(snapshot);
        let recovery_metadata = fs::symlink_metadata(&backup_root).ok();
        let recovery_root_preserved = recovery_metadata.as_ref().is_some_and(|metadata| {
            metadata.is_dir() && metadata.permissions().mode() & 0o777 == 0o700
        });
        let immutable_recovery_complete = tree(&backup_root.join("0")) == auth_before
            && tree(&backup_root.join("1")) == private_before
            && tree(&backup_root.join("2")) == runtime_before
            && tree(&backup_root.join("3")) == receipt_before;
        let recovery_has_no_symlink = tree(&backup_root)
            .values()
            .all(|entry| entry.kind != "symlink");
        assert!(
            refused_without_following
                && independent_authorities_restored
                && config_restored
                && app_restored
                && recovery_root_preserved
                && immutable_recovery_complete
                && recovery_has_no_symlink,
            "rollback refusal must aggregate safely: error={error}; independent={independent_authorities_restored}; config={config_restored}; app={app_restored}; recovery_root={recovery_root_preserved}; recovery_complete={immutable_recovery_complete}; no_symlink={recovery_has_no_symlink}"
        );
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    #[ignore = "explicit Acceptance-boundary optional SSH feature-off smoke; temp HOME only"]
    fn reuse_system_ssh_false_does_not_require_packaged_wrapper_or_system_home() {
        let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut env = ScopedEnv::new();
        let tmp = isolated_tmpdir("ssh-feature-off");
        let missing_wrapper = tmp.join("missing-wrapper");
        env.remove("HOME");
        env.set("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", &missing_wrapper);
        let cfg = Config {
            reuse_system_ssh: false,
            ..Default::default()
        };
        let sandbox_home = tmp.join("sandbox/home");
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let result =
            prevalidate_one_click_system_ssh(&app.handle().clone(), &cfg, &sandbox_home);
        assert!(
            result.is_ok(),
            "disabled SSH must not require HOME, system config, host parsing, or packaged wrapper: {result:?}"
        );
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    #[ignore = "explicit Acceptance-boundary SSH write-authority prevalidation; temp HOME only"]
    fn enabled_ssh_prevalidation_rejects_unwritable_science_authority() {
        let _env_lock = TEST_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let mut env = ScopedEnv::new();
        let tmp = isolated_tmpdir("ssh-unwritable-authority");
        let home = tmp.join("home");
        let sandbox_home = tmp.join("sandbox/home");
        let science_data = sandbox_home.join(".claude-science");
        fs::create_dir_all(home.join(".ssh")).unwrap();
        fs::write(home.join(".ssh/config"), b"Host isolated-test-host\n").unwrap();
        fs::set_permissions(
            home.join(".ssh/config"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::create_dir_all(&science_data).unwrap();
        fs::write(science_data.join("config.toml"), b"quiet_logs = true\n").unwrap();
        fs::set_permissions(
            science_data.join("config.toml"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::set_permissions(&science_data, fs::Permissions::from_mode(0o500)).unwrap();
        let wrapper = tmp.join("ssh-wrapper");
        fs::write(&wrapper, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        env.set("HOME", &home);
        env.set("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", &wrapper);
        let cfg = Config {
            reuse_system_ssh: true,
            ..Default::default()
        };
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let system_before = tree(&home.join(".ssh"));
        let science_before = tree(&science_data);
        let sandbox_stub_before = tree(&sandbox_home.join(".ssh"));
        let result =
            prevalidate_one_click_system_ssh(&app.handle().clone(), &cfg, &sandbox_home);
        let system_after = tree(&home.join(".ssh"));
        let science_after = tree(&science_data);
        let sandbox_stub_after = tree(&sandbox_home.join(".ssh"));
        let probe_residue = fs::read_dir(&science_data)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| {
                let name = name.to_string_lossy();
                name.starts_with('.') && name.contains("tmp")
            })
            .collect::<Vec<_>>();
        fs::set_permissions(&science_data, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error == "隔离 Science SSH authority 不可写"),
            "enabled SSH must reject a statically unwritable bridge authority before OAuth or journal mutation"
        );
        assert_eq!(system_after, system_before);
        assert_eq!(science_after, science_before);
        assert_eq!(sandbox_stub_after, sandbox_stub_before);
        assert!(
            probe_residue.is_empty(),
            "read-only prevalidation must not create a write probe or temp residue"
        );
        let _ = fs::remove_dir_all(tmp);
    }

    fn profile_with_policy(policy: ModelPolicy) -> config::Profile {
        config::Profile {
            model_policy: policy,
            ..Default::default()
        }
    }

    fn serve_models_after(
        delay: Duration,
        body: &'static str,
    ) -> (u16, Arc<AtomicUsize>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_ne!(port, 8765);
        let requests = Arc::new(AtomicUsize::new(0));
        let server_requests = requests.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /test-secret/v1/models HTTP/1.0\r\n"));
            server_requests.fetch_add(1, Ordering::SeqCst);
            thread::sleep(delay);
            write!(
                stream,
                "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        (port, requests, server)
    }

    #[test]
    fn gateway_catalog_timeout_matches_model_policy_contract() {
        assert_eq!(
            gateway_model_catalog_timeout_ms(&profile_with_policy(ModelPolicy::DynamicCatalog)),
            crate::runtime::operation::CODEX_MODELS_PROBE_TIMEOUT_MS
        );
        assert_eq!(
            gateway_model_catalog_timeout_ms(&profile_with_policy(ModelPolicy::SavedCatalog)),
            crate::runtime::operation::LOCAL_HEALTH_TIMEOUT_MS
        );
    }

    #[test]
    fn dynamic_catalog_cold_response_uses_one_long_local_request() {
        let body = r#"{"data":[{"id":"claude-csswitch-codex-gpt-5"}]}"#;
        let (port, requests, server) = serve_models_after(Duration::from_millis(600), body);
        let profile = profile_with_policy(ModelPolicy::DynamicCatalog);

        verify_gateway_model_catalog(port, "test-secret", &profile).unwrap();
        server.join().unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dynamic_catalog_still_rejects_empty_or_non_codex_aliases() {
        for body in [r#"{"data":[]}"#, r#"{"data":[{"id":"gpt-5"}]}"#] {
            let (port, _requests, server) = serve_models_after(Duration::ZERO, body);
            let profile = profile_with_policy(ModelPolicy::DynamicCatalog);
            let error = verify_gateway_model_catalog(port, "test-secret", &profile).unwrap_err();
            assert!(error.contains("Codex published model snapshot"));
            server.join().unwrap();
        }
    }

    #[test]
    fn runtime_journal_advances_in_place_and_retargets_without_secrets() {
        let dir = std::env::temp_dir().join(format!(
            "csswitch-runtime-journal-{}-{}",
            std::process::id(),
            config::new_id()
        ));
        let previous = RuntimeBindingCommit {
            profile_id: "old".into(),
            route_fp: "route-fp".into(),
            catalog_fp: "catalog-fp".into(),
            binding_fp: "binding-fp".into(),
        };
        config::save_to(
            &dir,
            &Config {
                runtime_binding: Some(previous.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        advance_runtime_transaction(&dir, "new", Some(previous.clone()), "start_gateway").unwrap();
        let first = config::load_from(&dir)
            .unwrap()
            .runtime_transaction
            .unwrap();
        assert_eq!(first.target_profile_id, "new");
        assert_eq!(first.stage, "start_gateway");
        assert_eq!(first.previous_binding, Some(previous.clone()));

        advance_runtime_transaction(&dir, "new", Some(previous.clone()), "start_science").unwrap();
        let second = config::load_from(&dir)
            .unwrap()
            .runtime_transaction
            .unwrap();
        assert_eq!(second.transaction_id, first.transaction_id);
        assert_eq!(second.stage, "start_science");

        advance_runtime_transaction(&dir, "newer", Some(previous), "start_gateway").unwrap();
        let retargeted = config::load_from(&dir)
            .unwrap()
            .runtime_transaction
            .unwrap();
        assert_ne!(retargeted.transaction_id, second.transaction_id);
        assert_eq!(retargeted.target_profile_id, "newer");
        let encoded = serde_json::to_string(&retargeted).unwrap();
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("base_url"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_click_snapshot_has_one_commit_and_one_failure_compensation_funnel() {
        use syn::visit::{self, Visit};
        use syn::{Expr, ExprCall, ExprMethodCall, Item, ItemFn, Pat, Stmt};

        fn top_level<'a>(file: &'a syn::File, name: &str) -> Option<&'a ItemFn> {
            file.items.iter().find_map(|item| match item {
                Item::Fn(function) if function.sig.ident == name => Some(function),
                _ => None,
            })
        }

        fn local_name(local: &syn::Local) -> Option<&syn::Ident> {
            match &local.pat {
                Pat::Ident(ident) => Some(&ident.ident),
                Pat::Type(typed) => match &*typed.pat {
                    Pat::Ident(ident) => Some(&ident.ident),
                    _ => None,
                },
                _ => None,
            }
        }

        fn result_arm_name(pattern: &Pat) -> Option<&syn::Ident> {
            match pattern {
                Pat::TupleStruct(tuple) => tuple.path.segments.last().map(|segment| &segment.ident),
                Pat::Struct(structure) => {
                    structure.path.segments.last().map(|segment| &segment.ident)
                }
                Pat::Path(path) => path.path.segments.last().map(|segment| &segment.ident),
                Pat::Ident(ident) => ident
                    .subpat
                    .as_ref()
                    .and_then(|(_, pattern)| result_arm_name(pattern)),
                _ => None,
            }
        }

        fn peel_expr(mut expression: &Expr) -> &Expr {
            loop {
                expression = match expression {
                    Expr::Group(group) => &group.expr,
                    Expr::Paren(paren) => &paren.expr,
                    _ => return expression,
                };
            }
        }

        fn direct_call_name(expression: &Expr) -> Option<&syn::Ident> {
            let Expr::Call(call) = peel_expr(expression) else {
                return None;
            };
            let Expr::Path(path) = peel_expr(&call.func) else {
                return None;
            };
            path.path.segments.last().map(|segment| &segment.ident)
        }

        fn success_tail_is_infallible(expression: &Expr) -> bool {
            match peel_expr(expression) {
                Expr::Path(_) => true,
                Expr::Call(call)
                    if direct_call_name(expression).is_some_and(|name| name == "Ok")
                        && call.args.len() == 1 =>
                {
                    matches!(peel_expr(call.args.first().unwrap()), Expr::Path(_))
                }
                _ => false,
            }
        }

        #[derive(Default)]
        struct FlowFacts {
            calls: Vec<String>,
            methods: Vec<String>,
            tries: usize,
            closures: usize,
        }

        impl<'ast> Visit<'ast> for FlowFacts {
            fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
                if let Expr::Path(path) = &*expression.func {
                    if let Some(segment) = path.path.segments.last() {
                        self.calls.push(segment.ident.to_string());
                    }
                }
                visit::visit_expr_call(self, expression);
            }

            fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
                self.methods.push(expression.method.to_string());
                visit::visit_expr_method_call(self, expression);
            }

            fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
                self.tries += 1;
                visit::visit_expr_try(self, expression);
            }

            fn visit_expr_closure(&mut self, expression: &'ast syn::ExprClosure) {
                self.closures += 1;
                visit::visit_expr_closure(self, expression);
            }
        }

        #[derive(Debug, Default)]
        struct OuterFacts {
            calls: Vec<String>,
            methods: Vec<String>,
            tries: usize,
            returns: usize,
            assignments: usize,
            macros: usize,
            closures: usize,
        }

        impl<'ast> Visit<'ast> for OuterFacts {
            fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
                if let Expr::Path(path) = &*expression.func {
                    if let Some(segment) = path.path.segments.last() {
                        self.calls.push(segment.ident.to_string());
                    }
                }
                visit::visit_expr_call(self, expression);
            }

            fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
                self.methods.push(expression.method.to_string());
                visit::visit_expr_method_call(self, expression);
            }

            fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
                self.tries += 1;
                visit::visit_expr_try(self, expression);
            }

            fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
                self.returns += 1;
                visit::visit_expr_return(self, expression);
            }

            fn visit_expr_assign(&mut self, expression: &'ast syn::ExprAssign) {
                self.assignments += 1;
                visit::visit_expr_assign(self, expression);
            }

            fn visit_macro(&mut self, expression: &'ast syn::Macro) {
                self.macros += 1;
                visit::visit_macro(self, expression);
            }

            fn visit_expr_closure(&mut self, _expression: &'ast syn::ExprClosure) {
                self.closures += 1;
            }
            fn visit_item_fn(&mut self, _function: &'ast ItemFn) {}
        }

        #[derive(Default)]
        struct TransactionLocalCount(usize);

        impl<'ast> Visit<'ast> for TransactionLocalCount {
            fn visit_local(&mut self, local: &'ast syn::Local) {
                if local_name(local).is_some_and(|name| name == "transaction_result") {
                    self.0 += 1;
                }
                visit::visit_local(self, local);
            }
        }

        let source = include_str!("sandbox_session.rs");
        let product_source = &source[..source
            .find("#[cfg(test)]\nmod transaction_tests")
            .expect("product source must precede transaction tests")];
        let file = syn::parse_file(product_source).expect("product Rust source must parse");
        let one_click = top_level(&file, "one_click_login_with_options")
            .expect("one-click product function must remain module-level");
        assert!(
            top_level(&file, "compensate_one_click_failure").is_some(),
            "one-click must expose one release-visible failure compensation helper"
        );
        let snapshot_index = one_click
            .block
            .stmts
            .iter()
            .position(|statement| {
                matches!(
                    statement,
                    Stmt::Local(local)
                        if local_name(local).is_some_and(|name| name == "authority_snapshot")
                )
            })
            .expect("one-click must capture authority_snapshot before mutation");
        assert_eq!(
            one_click.block.stmts.len(),
            snapshot_index + 3,
            "authority_snapshot must be followed by exactly transaction_result and its final match"
        );
        let transaction_index = snapshot_index + 1;
        let transaction_statement = &one_click.block.stmts[transaction_index];
        let transaction_local = match transaction_statement {
            Stmt::Local(local)
                if local_name(local).is_some_and(|name| name == "transaction_result") =>
            {
                local
            }
            _ => panic!("authority_snapshot must be followed immediately by let transaction_result"),
        };
        let mut transaction_locals = TransactionLocalCount::default();
        transaction_locals.visit_item_fn(one_click);
        assert_eq!(
            transaction_locals.0, 1,
            "one-click must contain exactly one transaction_result local"
        );
        let initializer = transaction_local
            .init
            .as_ref()
            .expect("transaction_result must have an immediate closure initializer");
        let transaction_call = match peel_expr(&initializer.expr) {
            Expr::Call(call) => call,
            _ => panic!("transaction_result initializer must directly invoke a closure"),
        };
        assert!(
            transaction_call.args.is_empty(),
            "transaction_result closure invocation must have zero arguments"
        );
        let transaction_closure = match peel_expr(&transaction_call.func) {
            Expr::Closure(closure) => closure,
            _ => panic!("transaction_result initializer must be a directly invoked closure"),
        };
        assert!(
            transaction_closure.inputs.is_empty(),
            "transaction_result closure must accept zero arguments"
        );
        let mut transaction = FlowFacts::default();
        transaction.visit_stmt(transaction_statement);
        assert_eq!(
            transaction.closures, 1,
            "transaction_result must be produced by one bounded mutation closure"
        );
        for required in [
            "ensure_virtual_login",
            "prepare_science_ssh_bridge",
            "revoke_science_ssh_bridge",
            "ensure_proxy",
            "record_managed_science_launch",
        ] {
            assert!(
                transaction.calls.iter().any(|call| call == required),
                "the single mutation closure must own the {required} error edge"
            );
        }
        assert!(
            transaction.methods.iter().any(|method| method == "status"),
            "the single mutation closure must own the shell spawn/nonzero status edge"
        );
        assert!(
            !transaction.methods.iter().any(|method| method == "commit")
                && !transaction
                    .calls
                    .iter()
                    .any(|call| call == "compensate_one_click_failure"),
            "transaction_result closure must neither commit nor compensate its own snapshot"
        );

        let final_statement = one_click
            .block
            .stmts
            .last()
            .expect("one-click must end in the transaction result match");
        let final_match = match final_statement {
            Stmt::Expr(Expr::Match(expression), _) => expression,
            _ => panic!("one-click must end with exactly one success/failure transaction match"),
        };
        assert!(
            matches!(
                &*final_match.expr,
                Expr::Path(path) if path.path.is_ident("transaction_result")
            ),
            "the final transaction match must consume transaction_result directly"
        );
        assert_eq!(
            final_match.arms.len(),
            2,
            "the final transaction match must contain only one success and one failure arm"
        );
        assert!(
            final_match.arms.iter().all(|arm| arm.guard.is_none()),
            "the final transaction match must not use guarded arms"
        );
        let success = final_match
            .arms
            .iter()
            .find(|arm| result_arm_name(&arm.pat).is_some_and(|name| name == "Ok"))
            .expect("the final transaction match must contain one Ok arm");
        let failure = final_match
            .arms
            .iter()
            .find(|arm| result_arm_name(&arm.pat).is_some_and(|name| name == "Err"))
            .expect("the final transaction match must contain one Err arm");
        let success_block = match peel_expr(&success.body) {
            Expr::Block(block) => &block.block,
            _ => panic!("Ok arm must be a block containing commit and an infallible tail"),
        };
        assert_eq!(
            success_block.stmts.len(),
            2,
            "Ok arm must contain only snapshot commit and an infallible success tail"
        );
        let direct_commit = match &success_block.stmts[0] {
            Stmt::Expr(Expr::MethodCall(call), Some(_)) => {
                call.method == "commit"
                    && call.args.is_empty()
                    && matches!(
                        peel_expr(&call.receiver),
                        Expr::Path(path) if path.path.is_ident("authority_snapshot")
                    )
            }
            _ => false,
        };
        assert!(
            direct_commit,
            "Ok arm must begin with the sole direct authority_snapshot.commit()"
        );
        assert!(
            matches!(
                &success_block.stmts[1],
                Stmt::Expr(tail, None) if success_tail_is_infallible(tail)
            ),
            "Ok arm must end with only an infallible path or Ok(path) tail"
        );

        let failure_expression = match peel_expr(&failure.body) {
            Expr::Block(block)
                if matches!(
                    block.block.stmts.as_slice(),
                    [Stmt::Expr(_, None)]
                ) =>
            {
                match &block.block.stmts[0] {
                    Stmt::Expr(expression, None) => expression,
                    _ => unreachable!(),
                }
            }
            expression => expression,
        };
        assert!(
            direct_call_name(failure_expression)
                .is_some_and(|name| name == "compensate_one_click_failure"),
            "Err arm must be exactly one direct compensate_one_click_failure call"
        );
        let Expr::Call(failure_call) = peel_expr(failure_expression) else {
            unreachable!()
        };
        let mut failure_arguments = OuterFacts::default();
        for argument in &failure_call.args {
            failure_arguments.visit_expr(argument);
        }
        assert!(
            failure_arguments.calls.is_empty()
                && failure_arguments.methods.is_empty()
                && failure_arguments.tries == 0
                && failure_arguments.returns == 0
                && failure_arguments.assignments == 0
                && failure_arguments.macros == 0
                && failure_arguments.closures == 0,
            "Err compensation arguments must be operation-free: {failure_arguments:?}"
        );

        let mut post_snapshot = FlowFacts::default();
        for statement in one_click.block.stmts.iter().skip(snapshot_index + 1) {
            post_snapshot.visit_stmt(statement);
        }
        assert_eq!(
            post_snapshot
                .methods
                .iter()
                .filter(|method| *method == "commit")
                .count(),
            1,
            "all post-snapshot AST must contain exactly one commit, solely in Ok"
        );
        assert_eq!(
            post_snapshot
                .calls
                .iter()
                .filter(|call| *call == "compensate_one_click_failure")
                .count(),
            1,
            "all post-snapshot AST must contain exactly one compensation call, solely in Err"
        );
    }

    #[test]
    fn ssh_wrapper_prevalidation_uses_the_running_runtime_validator_before_oauth() {
        use syn::visit::{self, Visit};
        use syn::{
            Attribute, Expr, ExprCall, ExprLit, GenericArgument, Item, ItemFn, Lit, Pat,
            PathArguments, Stmt, Type,
        };

        fn top_level<'a>(file: &'a syn::File, name: &str) -> &'a ItemFn {
            file.items
                .iter()
                .find_map(|item| match item {
                    Item::Fn(function) if function.sig.ident == name => Some(function),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing module-level product function {name}"))
        }

        fn is_cfg(attribute: &Attribute) -> bool {
            attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
        }

        fn cfg_tokens(attribute: &Attribute) -> String {
            attribute
                .meta
                .require_list()
                .map(|list| list.tokens.to_string())
                .unwrap_or_default()
        }

        fn reject_cfg(attributes: &[Attribute], label: &str) {
            assert!(
                !attributes.iter().any(is_cfg),
                "{label} must be present in every release build"
            );
        }

        #[derive(Default)]
        struct Facts {
            calls: Vec<String>,
            strings: Vec<String>,
            has_cfg: bool,
        }

        impl<'ast> Visit<'ast> for Facts {
            fn visit_attribute(&mut self, attribute: &'ast Attribute) {
                self.has_cfg |= is_cfg(attribute);
                visit::visit_attribute(self, attribute);
            }

            fn visit_expr_call(&mut self, call: &'ast ExprCall) {
                if let Expr::Path(path) = &*call.func {
                    if let Some(segment) = path.path.segments.last() {
                        self.calls.push(segment.ident.to_string());
                    }
                }
                visit::visit_expr_call(self, call);
            }

            fn visit_expr_lit(&mut self, literal: &'ast ExprLit) {
                if let Lit::Str(value) = &literal.lit {
                    self.strings.push(value.value());
                }
                visit::visit_expr_lit(self, literal);
            }

            fn visit_item_fn(&mut self, _function: &'ast ItemFn) {}
            fn visit_expr_closure(&mut self, _closure: &'ast syn::ExprClosure) {}
            fn visit_expr_async(&mut self, _expression: &'ast syn::ExprAsync) {}
        }

        fn statement_facts(statement: &Stmt) -> Facts {
            let mut facts = Facts::default();
            facts.visit_stmt(statement);
            facts
        }

        fn function_facts(function: &ItemFn) -> Facts {
            let mut facts = Facts::default();
            facts.visit_block(&function.block);
            facts
        }

        fn local_name(local: &syn::Local) -> Option<&syn::Ident> {
            match &local.pat {
                Pat::Ident(ident) => Some(&ident.ident),
                Pat::Type(typed) => match &*typed.pat {
                    Pat::Ident(ident) => Some(&ident.ident),
                    _ => None,
                },
                _ => None,
            }
        }

        fn peel_expression(mut expression: &Expr) -> &Expr {
            loop {
                expression = match expression {
                    Expr::Group(group) => &group.expr,
                    Expr::Paren(paren) => &paren.expr,
                    _ => return expression,
                };
            }
        }

        fn direct_zero_arg_closure_body(local: &syn::Local) -> Option<&syn::Block> {
            let initializer = local.init.as_ref()?;
            let Expr::Call(call) = peel_expression(&initializer.expr) else {
                return None;
            };
            if !call.args.is_empty() {
                return None;
            }
            let Expr::Closure(closure) = peel_expression(&call.func) else {
                return None;
            };
            closure
                .inputs
                .is_empty()
                .then_some(&closure.body)
                .and_then(|body| match peel_expression(body) {
                    Expr::Block(block) => Some(&block.block),
                    _ => None,
                })
        }

        fn direct_call(expression: &Expr) -> Option<&ExprCall> {
            match expression {
                Expr::Call(call) => Some(call),
                Expr::Await(awaited) => direct_call(&awaited.base),
                Expr::Group(group) => direct_call(&group.expr),
                Expr::Paren(paren) => direct_call(&paren.expr),
                Expr::Try(tried) => direct_call(&tried.expr),
                _ => None,
            }
        }

        fn call_path(call: &ExprCall) -> Option<String> {
            let Expr::Path(path) = &*call.func else {
                return None;
            };
            Some(
                path.path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }

        fn expression_path(expression: &Expr) -> Option<String> {
            let Expr::Path(path) = expression else {
                return None;
            };
            Some(
                path.path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }

        fn statement_directly_calls(statement: &Stmt, expected: &str) -> bool {
            let expression = match statement {
                Stmt::Local(local) => local.init.as_ref().map(|init| &*init.expr),
                Stmt::Expr(expression, _) => Some(expression),
                _ => None,
            };
            expression
                .and_then(direct_call)
                .and_then(call_path)
                .as_deref()
                == Some(expected)
        }

        fn simple_argument_name(expression: &Expr) -> Option<String> {
            let expression = match expression {
                Expr::Reference(reference) => &*reference.expr,
                Expr::Group(group) => &*group.expr,
                Expr::Paren(paren) => &*paren.expr,
                expression => expression,
            };
            let Expr::Path(path) = expression else {
                return None;
            };
            (path.qself.is_none() && path.path.segments.len() == 1)
                .then(|| path.path.segments[0].ident.to_string())
        }

        fn statement_direct_call_arguments(statement: &Stmt) -> Option<Vec<String>> {
            let expression = match statement {
                Stmt::Local(local) => local.init.as_ref().map(|init| &*init.expr),
                Stmt::Expr(expression, _) => Some(expression),
                _ => None,
            }?;
            let call = direct_call(expression)?;
            call.args.iter().map(simple_argument_name).collect()
        }

        fn statement_propagates_direct_call(statement: &Stmt, expected: &str) -> bool {
            let expression = match statement {
                Stmt::Local(local) => local.init.as_ref().map(|init| &*init.expr),
                Stmt::Expr(expression, _) => Some(expression),
                _ => None,
            };
            let Some(Expr::Try(tried)) = expression else {
                return false;
            };
            let Expr::Call(call) = &*tried.expr else {
                return false;
            };
            call_path(call).as_deref() == Some(expected)
        }

        fn returns_result_pathbuf_string(function: &ItemFn) -> bool {
            let syn::ReturnType::Type(_, returned) = &function.sig.output else {
                return false;
            };
            let Type::Path(path) = &**returned else {
                return false;
            };
            let Some(result) = path.path.segments.last() else {
                return false;
            };
            if result.ident != "Result" {
                return false;
            }
            let PathArguments::AngleBracketed(arguments) = &result.arguments else {
                return false;
            };
            let types = arguments
                .args
                .iter()
                .filter_map(|argument| match argument {
                    GenericArgument::Type(Type::Path(path)) => {
                        path.path.segments.last().map(|segment| segment.ident.to_string())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            types == ["PathBuf", "String"]
        }

        #[derive(Default)]
        struct EarlyExitFacts {
            count: usize,
        }

        impl<'ast> Visit<'ast> for EarlyExitFacts {
            fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
                self.count += 1;
                visit::visit_expr_return(self, expression);
            }

            fn visit_expr_break(&mut self, expression: &'ast syn::ExprBreak) {
                self.count += 1;
                visit::visit_expr_break(self, expression);
            }

            fn visit_expr_continue(&mut self, expression: &'ast syn::ExprContinue) {
                self.count += 1;
                visit::visit_expr_continue(self, expression);
            }

            fn visit_expr_loop(&mut self, expression: &'ast syn::ExprLoop) {
                self.count += 1;
                visit::visit_expr_loop(self, expression);
            }

            fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
                self.count += 1;
                visit::visit_expr_while(self, expression);
            }

            fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
                self.count += 1;
                visit::visit_expr_for_loop(self, expression);
            }

            fn visit_expr_call(&mut self, call: &'ast ExprCall) {
                if call_path(call).is_some_and(|path| {
                    matches!(
                        path.rsplit("::").next(),
                        Some("exit" | "abort" | "abort_internal")
                    )
                }) {
                    self.count += 1;
                }
                visit::visit_expr_call(self, call);
            }

            fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
                if invocation.path.segments.last().is_some_and(|segment| {
                    matches!(segment.ident.to_string().as_str(), "panic" | "todo" | "unreachable")
                }) || token_stream_contains_any(
                    invocation.tokens.clone(),
                    &["return", "break", "continue"],
                ) {
                    self.count += 1;
                }
                visit::visit_macro(self, invocation);
            }
        }

        #[derive(Default)]
        struct ProductLiterals(Vec<String>);

        impl<'ast> Visit<'ast> for ProductLiterals {
            fn visit_expr_lit(&mut self, literal: &'ast ExprLit) {
                if let Lit::Str(value) = &literal.lit {
                    self.0.push(value.value());
                }
                visit::visit_expr_lit(self, literal);
            }
        }

        fn use_tree_contains_ident(tree: &syn::UseTree, expected: &str) -> bool {
            match tree {
                syn::UseTree::Path(path) => {
                    path.ident == expected || use_tree_contains_ident(&path.tree, expected)
                }
                syn::UseTree::Name(name) => name.ident == expected,
                syn::UseTree::Rename(rename) => rename.ident == expected,
                syn::UseTree::Group(group) => group
                    .items
                    .iter()
                    .any(|tree| use_tree_contains_ident(tree, expected)),
                syn::UseTree::Glob(_) => false,
            }
        }

        fn token_stream_contains_any(
            tokens: proc_macro2::TokenStream,
            expected: &[&str],
        ) -> bool {
            tokens.into_iter().any(|token| match token {
                proc_macro2::TokenTree::Ident(ident) => {
                    expected.iter().any(|value| ident == *value)
                }
                proc_macro2::TokenTree::Group(group) => {
                    token_stream_contains_any(group.stream(), expected)
                }
                _ => false,
            })
        }

        #[derive(Default)]
        struct ForbiddenCfgMacros(usize);

        impl<'ast> Visit<'ast> for ForbiddenCfgMacros {
            fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
                if invocation
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "cfg")
                    || token_stream_contains_any(invocation.tokens.clone(), &["cfg"])
                {
                    self.0 += 1;
                }
                visit::visit_macro(self, invocation);
            }

            fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
                if use_tree_contains_ident(&item.tree, "cfg") {
                    self.0 += 1;
                }
                visit::visit_item_use(self, item);
            }
        }

        #[derive(Default)]
        struct ValidatorFacts {
            cfg_attributes: Vec<String>,
            environment_reads: Vec<String>,
            environment_paths: Vec<String>,
            environment_imports: usize,
        }

        #[derive(Default)]
        struct ProductEnvironmentFacts {
            environment_paths: Vec<String>,
            environment_imports: usize,
        }

        impl<'ast> Visit<'ast> for ProductEnvironmentFacts {
            fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
                let path = expression
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let last = path.rsplit("::").next().unwrap_or_default();
                if path.split("::").any(|segment| segment == "env")
                    || matches!(last, "var" | "var_os" | "vars" | "vars_os")
                {
                    self.environment_paths.push(path);
                }
                visit::visit_expr_path(self, expression);
            }

            fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
                if use_tree_contains_ident(&item.tree, "env") {
                    self.environment_imports += 1;
                }
                visit::visit_item_use(self, item);
            }

            fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
                if token_stream_contains_any(
                    invocation.tokens.clone(),
                    &["env", "var", "var_os", "vars", "vars_os"],
                ) {
                    self.environment_imports += 1;
                }
                visit::visit_macro(self, invocation);
            }
        }

        impl<'ast> Visit<'ast> for ValidatorFacts {
            fn visit_attribute(&mut self, attribute: &'ast Attribute) {
                if is_cfg(attribute) {
                    self.cfg_attributes.push(cfg_tokens(attribute));
                }
                visit::visit_attribute(self, attribute);
            }

            fn visit_expr_call(&mut self, call: &'ast ExprCall) {
                if let Some(path) = call_path(call) {
                    let last = path.rsplit("::").next().unwrap_or_default();
                    if path.split("::").any(|segment| segment == "env")
                        || matches!(last, "var" | "var_os" | "vars" | "vars_os")
                    {
                        self.environment_reads.push(path);
                    }
                }
                visit::visit_expr_call(self, call);
            }

            fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
                let path = expression
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let last = path.rsplit("::").next().unwrap_or_default();
                if path.split("::").any(|segment| segment == "env")
                    || matches!(last, "var" | "var_os" | "vars" | "vars_os")
                {
                    self.environment_paths.push(path);
                }
                visit::visit_expr_path(self, expression);
            }

            fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
                if use_tree_contains_ident(&item.tree, "env") {
                    self.environment_imports += 1;
                }
                visit::visit_item_use(self, item);
            }
        }

        #[derive(Default)]
        struct WrapperLocalCount(usize);

        impl<'ast> Visit<'ast> for WrapperLocalCount {
            fn visit_local(&mut self, local: &'ast syn::Local) {
                if local_name(local).is_some_and(|name| name == "wrapper_override") {
                    self.0 += 1;
                }
                visit::visit_local(self, local);
            }
        }

        #[derive(Default)]
        struct CfgAttributes(Vec<String>);

        impl<'ast> Visit<'ast> for CfgAttributes {
            fn visit_attribute(&mut self, attribute: &'ast Attribute) {
                if is_cfg(attribute) {
                    self.0.push(cfg_tokens(attribute));
                }
                visit::visit_attribute(self, attribute);
            }
        }

        fn exact_test_override(local: &syn::Local) -> bool {
            if local.attrs.len() != 1
                || !local.attrs[0].path().is_ident("cfg")
                || cfg_tokens(&local.attrs[0]) != "test"
                || !matches!(&local.pat, Pat::Ident(ident) if ident.ident == "wrapper_override")
            {
                return false;
            }
            let Some(initializer) = &local.init else {
                return false;
            };
            let Expr::MethodCall(mapped) = &*initializer.expr else {
                return false;
            };
            if mapped.method != "map" || mapped.args.len() != 1 {
                return false;
            }
            let Expr::Call(read) = &*mapped.receiver else {
                return false;
            };
            if call_path(read).as_deref() != Some("std::env::var_os") || read.args.len() != 1 {
                return false;
            }
            if !matches!(
                read.args.first(),
                Some(Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                })) if value.value() == "CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE"
            ) {
                return false;
            }
            expression_path(mapped.args.first().unwrap()).as_deref() == Some("PathBuf::from")
        }

        fn option_pathbuf_type(pattern: &Pat) -> bool {
            let Pat::Type(typed) = pattern else {
                return false;
            };
            if !matches!(&*typed.pat, Pat::Ident(ident) if ident.ident == "wrapper_override") {
                return false;
            }
            let Type::Path(path) = &*typed.ty else {
                return false;
            };
            let Some(option) = path.path.segments.last() else {
                return false;
            };
            if option.ident != "Option" {
                return false;
            }
            let PathArguments::AngleBracketed(arguments) = &option.arguments else {
                return false;
            };
            matches!(
                arguments.args.first(),
                Some(GenericArgument::Type(Type::Path(inner)))
                    if inner.path.segments.last().is_some_and(|segment| segment.ident == "PathBuf")
            )
        }

        fn exact_release_override(local: &syn::Local) -> bool {
            if local.attrs.len() != 1
                || !local.attrs[0].path().is_ident("cfg")
                || cfg_tokens(&local.attrs[0]).replace(' ', "") != "not(test)"
                || !option_pathbuf_type(&local.pat)
            {
                return false;
            }
            let Some(initializer) = &local.init else {
                return false;
            };
            expression_path(&initializer.expr).as_deref() == Some("None")
        }

        let source = include_str!("sandbox_session.rs");
        let product_source = &source[..source
            .find("#[cfg(test)]\nmod transaction_tests")
            .expect("product source must precede transaction tests")];
        let file = syn::parse_file(product_source).expect("product Rust source must parse");
        let mut forbidden_cfg_macros = ForbiddenCfgMacros::default();
        forbidden_cfg_macros.visit_file(&file);
        assert_eq!(
            forbidden_cfg_macros.0, 0,
            "product SSH transaction source must not branch on cfg!(test)"
        );
        let validator = top_level(&file, "validate_system_ssh_wrapper_path");
        let running = top_level(&file, "validate_running_system_ssh_bridge");
        let prevalidation = top_level(&file, "prevalidate_one_click_system_ssh");
        let one_click = top_level(&file, "one_click_login_with_options");
        assert!(
            returns_result_pathbuf_string(validator),
            "shared wrapper validator must return Result<PathBuf, String>"
        );
        let mut product_environment = ProductEnvironmentFacts::default();
        product_environment.visit_file(&file);
        product_environment.environment_paths.sort();
        assert_eq!(
            product_environment.environment_imports, 0,
            "product SSH transaction source must not import or alias environment APIs"
        );
        assert_eq!(
            product_environment.environment_paths,
            [
                "std::env::var".to_string(),
                "std::env::var_os".to_string(),
                "std::env::var_os".to_string(),
            ],
            "product SSH transaction source may reference only the existing spike seam, exact wrapper override, and exact late-failure seam environment APIs"
        );

        for (name, function) in [
            ("shared wrapper validator", validator),
            ("running SSH validator", running),
            ("pre-OAuth SSH validator", prevalidation),
            ("one-click product path", one_click),
        ] {
            reject_cfg(&function.attrs, name);
        }

        for (name, function) in [("running SSH validator", running)] {
            let facts = function_facts(function);
            assert!(!facts.has_cfg, "{name} must not contain cfg-gated call sites");
        }
        let prevalidation_facts = function_facts(prevalidation);
        assert!(
            !prevalidation_facts.has_cfg,
            "pre-OAuth SSH validation must not contain cfg-gated call sites"
        );
        assert_eq!(
            prevalidation_facts
                .calls
                .iter()
                .filter(|call| *call == "validate_system_ssh_wrapper_path")
                .count(),
            1,
            "enabled pre-OAuth validation must use the same shared wrapper validator exactly once"
        );
        for required in [
            "prevalidate_science_ssh_bridge",
            "prevalidate_sandbox_ssh_stub",
        ] {
            assert!(
                prevalidation_facts.calls.iter().any(|call| call == required),
                "pre-OAuth validation must preserve disabled-mode read-only conflict validation via {required}"
            );
        }

        for (name, function) in [("running SSH validator", running)] {
            let shared_call_positions = function
                .block
                .stmts
                .iter()
                .enumerate()
                .filter_map(|(index, statement)| {
                    statement_directly_calls(
                        statement,
                        "crate::runtime::sandbox_session::validate_system_ssh_wrapper_path",
                    )
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                shared_call_positions,
                [0],
                "{name} must execute the shared wrapper validator exactly once as its first statement"
            );
            assert_eq!(
                statement_direct_call_arguments(&function.block.stmts[0]),
                Some(vec!["app".to_string()]),
                "{name} shared-validator call may receive only the simple app argument"
            );
            assert!(
                statement_propagates_direct_call(
                    &function.block.stmts[0],
                    "crate::runtime::sandbox_session::validate_system_ssh_wrapper_path",
                ),
                "{name} must propagate the shared-validator Result with an exact Try(Call)"
            );
            let mut call_exit = EarlyExitFacts::default();
            call_exit.visit_stmt(&function.block.stmts[0]);
            assert_eq!(
                call_exit.count, 0,
                "{name} shared-validator call statement must not hide early-exit control flow in its arguments"
            );
        }

        let prevalidate_statement = one_click
            .block
            .stmts
            .iter()
            .position(|statement| {
                matches!(statement, Stmt::Local(_))
                    && statement_directly_calls(
                        statement,
                        "crate::runtime::sandbox_session::prevalidate_one_click_system_ssh",
                    )
            })
            .expect("one-click must execute prevalidation in a top-level local statement");
        assert_eq!(
            one_click
                .block
                .stmts
                .iter()
                .filter(|statement| {
                    statement_directly_calls(
                        statement,
                        "crate::runtime::sandbox_session::prevalidate_one_click_system_ssh",
                    )
                })
                .count(),
            1,
            "one-click must execute exactly one direct prevalidation call"
        );
        assert_eq!(
            statement_direct_call_arguments(&one_click.block.stmts[prevalidate_statement]),
            Some(vec![
                "app".to_string(),
                "cfg".to_string(),
                "sbx_home".to_string(),
            ]),
            "one-click prevalidation may receive only simple app, cfg, and sbx_home arguments"
        );
        assert!(
            statement_propagates_direct_call(
                &one_click.block.stmts[prevalidate_statement],
                "crate::runtime::sandbox_session::prevalidate_one_click_system_ssh",
            ),
            "one-click must propagate the prevalidation Result with an exact Try(Call)"
        );
        let mut early_exit = EarlyExitFacts::default();
        for statement in &one_click.block.stmts[..prevalidate_statement] {
            early_exit.visit_stmt(statement);
        }
        early_exit.visit_stmt(&one_click.block.stmts[prevalidate_statement]);
        assert_eq!(
            early_exit.count, 0,
            "one-click prevalidation statement and its prefix must be reachable before explicit early-exit control flow"
        );
        let authority_snapshot_statement = one_click
            .block
            .stmts
            .iter()
            .position(|statement| {
                matches!(
                    statement,
                    Stmt::Local(local)
                        if local_name(local).is_some_and(|name| name == "authority_snapshot")
                )
            })
            .expect("one-click authority snapshot statement must exist");
        let transaction_statement = one_click
            .block
            .stmts
            .iter()
            .position(|statement| {
                matches!(
                    statement,
                    Stmt::Local(local)
                        if local_name(local).is_some_and(|name| name == "transaction_result")
                )
            })
            .expect("one-click transaction_result statement must exist");
        assert!(
            prevalidate_statement < authority_snapshot_statement
                && authority_snapshot_statement < transaction_statement,
            "SSH prevalidation must precede authority snapshot and the mutation transaction"
        );
        assert!(
            one_click.block.stmts[..transaction_statement]
                .iter()
                .all(|statement| !statement_facts(statement)
                    .calls
                    .iter()
                    .any(|call| call == "ensure_virtual_login")),
            "one-click must not execute OAuth mutation before transaction_result"
        );
        let transaction_local = match &one_click.block.stmts[transaction_statement] {
            Stmt::Local(local) => local,
            _ => unreachable!(),
        };
        let transaction_body = direct_zero_arg_closure_body(transaction_local)
            .expect("transaction_result must directly invoke one zero-argument closure block");
        let oauth_statements = transaction_body
            .stmts
            .iter()
            .enumerate()
            .filter(|(_, statement)| {
                statement_facts(statement)
                    .calls
                    .iter()
                    .any(|call| call == "ensure_virtual_login")
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            oauth_statements.len(),
            1,
            "transaction body must contain exactly one top-level OAuth mutation statement"
        );
        let mut one_click_cfg = CfgAttributes::default();
        one_click_cfg.visit_block(&one_click.block);
        assert_eq!(
            one_click_cfg.0,
            ["test".to_string()],
            "one-click may contain only the exact late-failure cfg(test) seam"
        );
        let late_seam_statements = transaction_body
            .stmts
            .iter()
            .enumerate()
            .filter(|(_, statement)| {
                let facts = statement_facts(statement);
                facts.has_cfg
                    && facts
                        .strings
                        .iter()
                        .any(|value| value == "CSSWITCH_TEST_SSH_LATE_FOREIGN_STUB")
                    && matches!(
                        statement,
                        Stmt::Expr(Expr::If(expression), _)
                            if expression.attrs.len() == 1
                                && expression.attrs[0].path().is_ident("cfg")
                                && cfg_tokens(&expression.attrs[0]) == "test"
                    )
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            late_seam_statements.len(),
            1,
            "transaction body must contain exactly one top-level exact cfg(test) late-failure seam"
        );
        assert!(
            oauth_statements[0] < late_seam_statements[0],
            "the sole transaction cfg(test) seam must remain after OAuth mutation"
        );

        let wrapper_locals = validator
            .block
            .stmts
            .iter()
            .filter_map(|statement| match statement {
                Stmt::Local(local)
                    if local_name(local).is_some_and(|name| name == "wrapper_override") =>
                {
                    Some(local)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut wrapper_local_count = WrapperLocalCount::default();
        wrapper_local_count.visit_block(&validator.block);
        assert_eq!(
            wrapper_local_count.0, 2,
            "shared validator must not hide extra wrapper_override locals in nested product code"
        );
        assert_eq!(
            wrapper_locals.len(),
            2,
            "shared validator must define test and release wrapper_override locals"
        );
        assert!(
            wrapper_locals.iter().any(|local| exact_test_override(local)),
            "test wrapper_override must be the sole cfg(test) var_os literal mapped through PathBuf::from"
        );
        assert!(
            wrapper_locals.iter().any(|local| exact_release_override(local)),
            "release wrapper_override must be exactly cfg(not(test)) Option<PathBuf> = None"
        );
        let mut validator_facts = ValidatorFacts::default();
        validator_facts.visit_block(&validator.block);
        validator_facts.cfg_attributes.sort();
        assert_eq!(
            validator_facts.cfg_attributes,
            ["not (test)".to_string(), "test".to_string()],
            "shared validator may contain only the two exact wrapper_override cfg attributes"
        );
        assert_eq!(
            validator_facts.environment_reads,
            ["std::env::var_os".to_string()],
            "shared validator may perform only the guarded test var_os environment read"
        );
        assert_eq!(
            validator_facts.environment_paths,
            ["std::env::var_os".to_string()],
            "shared validator may reference only the guarded test var_os environment path"
        );
        assert_eq!(
            validator_facts.environment_imports, 0,
            "shared validator must not import or alias environment APIs"
        );
        let mut product_literals = ProductLiterals::default();
        product_literals.visit_file(&file);
        assert_eq!(
            product_literals
                .0
                .iter()
                .filter(|value| value.as_str() == "CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE")
                .count(),
            1,
            "the test wrapper environment variable may appear only in its guarded local"
        );

        struct RestoreWrapperOverride(Option<std::ffi::OsString>);
        impl Drop for RestoreWrapperOverride {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(value) => {
                        std::env::set_var("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", value)
                    }
                    None => std::env::remove_var("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE"),
                }
            }
        }

        use std::os::unix::fs::PermissionsExt;
        let _override_guard =
            RestoreWrapperOverride(std::env::var_os("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE"));
        let root = std::env::temp_dir().join(format!(
            "csswitch-shared-ssh-validator-{}-{}",
            std::process::id(),
            crate::config::new_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let missing = root.join("missing-wrapper");
        std::env::set_var("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", &missing);
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap_err(),
            "打包的 CSSwitch SSH bridge 缺失"
        );
        let wrapper = root.join("ssh");
        std::fs::write(&wrapper, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::env::set_var("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", &wrapper);
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap_err(),
            "打包的 CSSwitch SSH bridge 不是安全的可执行文件"
        );
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap(),
            wrapper
        );
        let wrapper_link = root.join("ssh-link");
        std::os::unix::fs::symlink(&wrapper, &wrapper_link).unwrap();
        std::env::set_var("CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE", &wrapper_link);
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap_err(),
            "打包的 CSSwitch SSH bridge 不是安全的可执行文件"
        );
        let wrapper_directory = root.join("ssh-directory");
        std::fs::create_dir(&wrapper_directory).unwrap();
        std::fs::set_permissions(
            &wrapper_directory,
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        std::env::set_var(
            "CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE",
            &wrapper_directory,
        );
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap_err(),
            "打包的 CSSwitch SSH bridge 不是安全的可执行文件"
        );
        let oversized_wrapper = root.join("ssh-oversized");
        std::fs::write(&oversized_wrapper, vec![b'x'; 128 * 1024 + 1]).unwrap();
        std::fs::set_permissions(
            &oversized_wrapper,
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        std::env::set_var(
            "CSSWITCH_TEST_SSH_WRAPPER_OVERRIDE",
            &oversized_wrapper,
        );
        assert_eq!(
            super::validate_system_ssh_wrapper_path(app.handle()).unwrap_err(),
            "打包的 CSSwitch SSH bridge 不是安全的可执行文件"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
