"""The deliberately small RUE-05A/RUE-06 fixed attempt executor.

This module owns one fixture, one child, one adapter channel and one private
attempt record at a time.  Policy and retry scheduling remain outside it.
"""
from __future__ import annotations

import errno
import fcntl
import hashlib
import os
import select
import signal
import socket
import stat
import sys
import threading
import time
from collections.abc import Mapping
from pathlib import Path
from typing import Any

from .atomic_store import RunLayout, RunStoreError
from .contracts import AttemptDecisionV1, adjudicate_adapter_attempt, adjudicate_parent_event
from .manifest_contracts import canonical_json_bytes, load_canonical_json


_FIXTURE_RELATIVE = ("test", "quality", "fixtures", "run_evidence", "attempt0_fixture.py")
_FIXTURE_LOGICAL = "/".join(_FIXTURE_RELATIVE)
_SUITE_ID = "SUITE-RUE05A"
_ENTRYPOINT_ID = "ENTRY-RUE05A-ATTEMPT0"
_CHILD_CACHE_FD = 198
_CHILD_ADAPTER_FD = 199
_CACHE_BOOTSTRAP = "import os;os.lseek(198,0,0);p='/dev/fd/198';exec(compile(open(p,'rb').read(),p,'exec'))"
_MAX_FIXTURE_BYTES = 1024 * 1024
_MAX_ADAPTER_BYTES = 64 * 1024
_MAX_OUTPUT_BYTES = 64 * 1024
_TIMEOUT_SECONDS = 2.0
_TERM_GRACE_SECONDS = 0.20
_TERMINAL_DRAIN_SECONDS = 0.25


class Attempt0RunnerError(RuntimeError):
    """A typed local executor failure; no result schema is invented for it."""

    def __init__(self, code: str) -> None:
        self.code = code
        super().__init__(code)


def _identity(item: os.stat_result) -> tuple[int, int, int, int, int, int, int, int]:
    return (item.st_dev, item.st_ino, item.st_uid, stat.S_IMODE(item.st_mode), item.st_nlink, item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def _read_exact(fd: int, size: int) -> bytes:
    parts: list[bytes] = []
    remaining = size
    os.lseek(fd, 0, os.SEEK_SET)
    while remaining:
        chunk = os.read(fd, min(65536, remaining))
        if not chunk:
            raise Attempt0RunnerError("FD_DRIFT")
        parts.append(chunk)
        remaining -= len(chunk)
    if os.read(fd, 1):
        raise Attempt0RunnerError("FD_DRIFT")
    return b"".join(parts)


def _open_repo_root(repo_root: str | os.PathLike[str]) -> int:
    text = os.fspath(repo_root)
    if not isinstance(text, str) or not text.startswith("/") or text == "/" or "//" in text:
        raise Attempt0RunnerError("REPOSITORY_UNSAFE")
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        for component in text[1:].split("/"):
            if not component or component in {".", ".."}:
                raise Attempt0RunnerError("REPOSITORY_UNSAFE")
            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        item = os.fstat(fd)
        if not stat.S_ISDIR(item.st_mode):
            raise Attempt0RunnerError("REPOSITORY_UNSAFE")
        return fd
    except BaseException:
        os.close(fd)
        raise


def _snapshot_fixture_digest(manifest: Mapping[str, Any]) -> tuple[str, int, str]:
    entries = manifest.get("entries")
    if not isinstance(entries, list):
        raise Attempt0RunnerError("SNAPSHOT_BINDING_MISMATCH")
    found = [entry for entry in entries if isinstance(entry, Mapping) and entry.get("path") == _FIXTURE_LOGICAL]
    if len(found) != 1 or found[0].get("type") != "file" or found[0].get("mode") not in {"100644", "100755"}:
        raise Attempt0RunnerError("SNAPSHOT_BINDING_MISMATCH")
    digest = found[0].get("sha256")
    size, mode = found[0].get("size"), found[0].get("mode")
    if not isinstance(digest, str) or len(digest) != 64 or not isinstance(size, int) or isinstance(size, bool):
        raise Attempt0RunnerError("SNAPSHOT_BINDING_MISMATCH")
    return digest, size, mode


def _copy_bound_fixture(
    repo_root: str | os.PathLike[str], layout: RunLayout, expected_digest: str,
    expected_size: int, expected_mode: str, attempt_index: int,
) -> int:
    """Copy source bytes through held descriptors and return a held cache FD."""
    root_fd = source_fd = cache_fd = verify_fd = result_fd = None
    try:
        root_fd = _open_repo_root(repo_root)
        parent = root_fd
        for leaf in _FIXTURE_RELATIVE[:-1]:
            child = os.open(leaf, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
            if parent != root_fd:
                os.close(parent)
            parent = child
        source_fd = os.open(_FIXTURE_RELATIVE[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
        named_before = os.stat(_FIXTURE_RELATIVE[-1], dir_fd=parent, follow_symlinks=False)
        first = os.fstat(source_fd)
        expected_permissions = {"100644": 0o644, "100755": 0o755}[expected_mode]
        if (
            not stat.S_ISREG(first.st_mode) or first.st_nlink != 1
            or _identity(first) != _identity(named_before)
            or first.st_size != expected_size or first.st_size > _MAX_FIXTURE_BYTES
            or stat.S_IMODE(first.st_mode) != expected_permissions
        ):
            raise Attempt0RunnerError("TOOL_IDENTITY_CHANGED")
        raw = _read_exact(source_fd, first.st_size)
        named_after = os.stat(_FIXTURE_RELATIVE[-1], dir_fd=parent, follow_symlinks=False)
        after = os.fstat(source_fd)
        if (
            _identity(first) != _identity(after) or _identity(after) != _identity(named_after)
            or after.st_size != expected_size or hashlib.sha256(raw).hexdigest() != expected_digest
        ):
            raise Attempt0RunnerError("TOOL_IDENTITY_CHANGED")
        try:
            cache_leaf = f"attempt{attempt_index}-fixture.py"
            cache_fd = os.open(cache_leaf, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=layout._cache_fd_required())
        except FileExistsError:
            raise Attempt0RunnerError("CACHE_REPLAY")
        offset = 0
        while offset < len(raw):
            count = os.write(cache_fd, raw[offset:])
            if count <= 0:
                raise Attempt0RunnerError("FD_DRIFT")
            offset += count
        os.fsync(cache_fd)
        written = os.fstat(cache_fd)
        if not stat.S_ISREG(written.st_mode) or written.st_nlink != 1 or written.st_size != expected_size:
            raise Attempt0RunnerError("FD_DRIFT")
        written_identity = _identity(written)
        verify_fd = os.open(cache_leaf, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=layout._cache_fd_required())
        cache_named = os.stat(cache_leaf, dir_fd=layout._cache_fd_required(), follow_symlinks=False)
        verified = os.fstat(verify_fd)
        if (
            not stat.S_ISREG(verified.st_mode)
            or stat.S_IMODE(verified.st_mode) != 0o600
            or verified.st_uid != os.geteuid()
            or verified.st_nlink != 1
            or verified.st_size != expected_size
            or expected_size > _MAX_FIXTURE_BYTES
            or _identity(verified) != written_identity
            or _identity(verified) != _identity(cache_named)
        ):
            raise Attempt0RunnerError("TOOL_IDENTITY_CHANGED")
        reread = _read_exact(verify_fd, expected_size)
        verified_after = os.fstat(verify_fd)
        cache_named_after = os.stat(
            cache_leaf,
            dir_fd=layout._cache_fd_required(),
            follow_symlinks=False,
        )
        if (
            _identity(verified_after) != written_identity
            or _identity(cache_named_after) != written_identity
            or hashlib.sha256(reread).hexdigest() != expected_digest
        ):
            raise Attempt0RunnerError("TOOL_IDENTITY_CHANGED")
        os.close(cache_fd); cache_fd = None
        result_fd, verify_fd = verify_fd, None
        return result_fd
    except OSError as exc:
        raise Attempt0RunnerError("TOOL_IDENTITY_CHANGED") from exc
    finally:
        for fd in (verify_fd, cache_fd, source_fd):
            if fd is not None:
                try: os.close(fd)
                except OSError: pass
        # ``parent`` may equal root, but close only once.
        if 'parent' in locals() and parent != root_fd:
            try: os.close(parent)
            except OSError: pass
        if root_fd is not None:
            try: os.close(root_fd)
            except OSError: pass


def _moved_child_fd(fd: int) -> int:
    return fcntl.fcntl(fd, fcntl.F_DUPFD_CLOEXEC, 200)


def _wait_once(pid: int, slot: list[int], done: threading.Event) -> None:
    try:
        while True:
            try:
                _, status = os.waitpid(pid, 0)
                break
            except InterruptedError:
                continue
        slot.append(status)  # The wait slot is written before NOTE_EXIT is observable.
    finally:
        done.set()


def _send_ack(peer: socket.socket, remaining: memoryview) -> int:
    return peer.send(remaining)


def _spawn_actions(
    held_cache_fd: int, adapter_fd: int, output_fd: int,
    child_cache: int, child_adapter: int, child_output: int,
) -> list[tuple[int, ...]]:
    actions: list[tuple[int, ...]] = [
        (os.POSIX_SPAWN_DUP2, child_cache, _CHILD_CACHE_FD),
        (os.POSIX_SPAWN_DUP2, child_adapter, _CHILD_ADAPTER_FD),
        (os.POSIX_SPAWN_DUP2, child_output, 1),
        (os.POSIX_SPAWN_DUP2, child_output, 2),
    ]
    for original in (held_cache_fd, adapter_fd, output_fd, child_cache, child_adapter, child_output):
        if original not in {_CHILD_CACHE_FD, _CHILD_ADAPTER_FD, 1, 2}:
            actions.append((os.POSIX_SPAWN_CLOSE, original))
    return actions


def _close_fds(values: list[int | None]) -> None:
    for fd in values:
        if fd is not None:
            try:
                os.close(fd)
            except OSError:
                pass


def _signal_group(pid: int, sig: int) -> None:
    try:
        os.killpg(pid, sig)
    except ProcessLookupError:
        pass
    except OSError as error:
        if error.errno != errno.ESRCH:
            raise Attempt0RunnerError("PROCESS_GROUP_CLEANUP_FAILED") from error


def _process_group_gone(pid: int, *, leader_reaped: bool) -> bool:
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return True
    except OSError as error:
        if error.errno == errno.ESRCH:
            return True
        if error.errno == errno.EPERM:
            # Darwin may report EPERM for the now-empty pgid after the sole
            # leader has been reaped.  Before reap it remains unconfirmed.
            return leader_reaped
        raise Attempt0RunnerError("PROCESS_GROUP_CLEANUP_FAILED") from error
    return False


def _delete_kevent(kqueue: Any, ident: int, filter_value: int) -> None:
    try:
        kqueue.control([select.kevent(ident, filter=filter_value, flags=select.KQ_EV_DELETE)], 0, 0)
    except OSError as error:
        if error.errno not in {errno.ENOENT, errno.EBADF}:
            raise


def _exit_code(status: int) -> int:
    if os.WIFEXITED(status):
        return os.WEXITSTATUS(status)
    if os.WIFSIGNALED(status):
        return -os.WTERMSIG(status)
    return -255


def _infra(reason: str, rc: int, run_id: str, attempt_index: int) -> AttemptDecisionV1:
    # Existing RUE-01 decision type and reason vocabulary; no runner schema.
    from .contracts import AttemptRecord
    return AttemptDecisionV1(
        run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index,
        AttemptRecord(attempt_index, rc), "INFRA", reason,
    )


def _run_attempt(
    *, repo_root: str | os.PathLike[str], layout: RunLayout,
    attempt_index: int, scenario: str = "normal",
) -> AttemptDecisionV1:
    if attempt_index == 0:
        manifest = layout.begin_attempt0()
        publish = layout.publish_attempt0_decision
    elif attempt_index == 1:
        manifest = layout.begin_attempt1()
        publish = layout.publish_attempt1_decision
    else:
        raise Attempt0RunnerError("ATTEMPT_INDEX_UNSAFE")
    digest, fixture_size, fixture_mode = _snapshot_fixture_digest(manifest)
    try:
        held_cache_fd = _copy_bound_fixture(
            repo_root, layout, digest, fixture_size, fixture_mode, attempt_index,
        )
    except Attempt0RunnerError as error:
        if error.code == "TOOL_IDENTITY_CHANGED":
            decision = adjudicate_parent_event(
                "TOOL_IDENTITY_CHANGED", None, layout.run_id,
                _SUITE_ID, _ENTRYPOINT_ID, attempt_index,
            )
            publish(decision)
            return decision
        raise
    parent_sock: socket.socket | None = None
    child_sock: socket.socket | None = None
    out_read = out_write = child_cache = child_adapter = child_output = None
    pid: int | None = None
    kqueue = None
    reaper: threading.Thread | None = None
    reaped = threading.Event()
    slot: list[int] = []
    reaper_started = False
    child_reaped = False
    try:
        parent_sock, child_sock = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
        out_read, out_write = os.pipe()
        parent_sock.setblocking(False)
        os.set_blocking(out_read, False)
        # /dev/fd inherits the open-file description offset; execution must
        # start from the verified bytes rather than the preceding hash read.
        os.lseek(held_cache_fd, 0, os.SEEK_SET)
        child_cache = _moved_child_fd(held_cache_fd)
        child_adapter = _moved_child_fd(child_sock.fileno())
        child_output = _moved_child_fd(out_write)
        env = {
            "HOME": layout.state_path, "PATH": os.defpath, "PYTHONNOUSERSITE": "1",
            "RUE05A_ENTRYPOINT": _ENTRYPOINT_ID,
        }
        if scenario != "normal": env["RUE05A_PRIVATE_SCENARIO"] = scenario
        actions = _spawn_actions(held_cache_fd, child_sock.fileno(), out_write, child_cache, child_adapter, child_output)
        argv = [
            sys.executable, "-I", "-S", "-c", _CACHE_BOOTSTRAP,
            "--adapter-fd", str(_CHILD_ADAPTER_FD), "--attempt-index",
            str(attempt_index), layout.run_id, _SUITE_ID, _ENTRYPOINT_ID,
        ]
        pid = os.posix_spawn(sys.executable, argv, env, file_actions=actions, setpgroup=0)
    except OSError:
        _close_fds([child_cache, child_adapter, child_output, held_cache_fd, out_read, out_write])
        if parent_sock is not None: parent_sock.close()
        if child_sock is not None: child_sock.close()
        decision = adjudicate_parent_event(
            "SPAWN_EXEC_FAILED", None, layout.run_id,
            _SUITE_ID, _ENTRYPOINT_ID, attempt_index,
        )
        publish(decision)
        return decision
    _close_fds([child_cache, child_adapter, child_output, held_cache_fd, out_write])
    held_cache_fd = out_write = None
    child_sock.close(); child_sock = None
    buf = bytearray()
    adapter: Any = None
    adapter_error: str | None = None
    adapter_eof = output_eof = False
    ack_offset = 0
    output = 0
    output_limit = timed_out = False
    term_at: float | None = None
    cutoff_at: float | None = None
    group_term_at: float | None = None
    group_kill_sent = False
    start = time.monotonic()
    decision: AttemptDecisionV1 | None = None
    try:
        kqueue = select.kqueue()
        kqueue.control([
            select.kevent(pid, filter=select.KQ_FILTER_PROC, flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE | select.KQ_EV_ONESHOT, fflags=select.KQ_NOTE_EXIT),
            select.kevent(parent_sock.fileno(), filter=select.KQ_FILTER_READ, flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE),
            select.kevent(out_read, filter=select.KQ_FILTER_READ, flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE),
        ], 0, 0)
        # Register kernel exit readiness before the sole reaper can consume it.
        reaper = threading.Thread(target=_wait_once, args=(pid, slot, reaped), daemon=True)
        reaper.start()
        reaper_started = True
        while True:
            now = time.monotonic()
            if cutoff_at is None and now - start >= _TIMEOUT_SECONDS and term_at is None:
                timed_out = True
                _signal_group(pid, signal.SIGTERM)
                term_at = now
            if term_at is not None and cutoff_at is None and now - term_at >= _TERM_GRACE_SECONDS:
                _signal_group(pid, signal.SIGKILL)
            group_gone = cutoff_at is not None and _process_group_gone(pid, leader_reaped=bool(slot))
            if cutoff_at is not None:
                since_cutoff = now - cutoff_at
                if (
                    not group_gone and group_term_at is None and since_cutoff >= 0.06
                    and (bool(slot) or not (adapter_eof and output_eof))
                ):
                    _signal_group(pid, signal.SIGTERM)
                    group_term_at = now
                if not group_gone and group_term_at is not None and now - group_term_at >= 0.10 and not group_kill_sent:
                    _signal_group(pid, signal.SIGKILL)
                    group_kill_sent = True
                if since_cutoff >= _TERMINAL_DRAIN_SECONDS:
                    if not group_gone:
                        raise Attempt0RunnerError("PROCESS_GROUP_CLEANUP_FAILED")
                    if not (adapter_eof and output_eof):
                        raise Attempt0RunnerError("TERMINAL_DRAIN_INCOMPLETE")
            if cutoff_at is not None and adapter_eof and output_eof and group_gone:
                break
            deadline = start + _TIMEOUT_SECONDS if cutoff_at is None else cutoff_at + _TERMINAL_DRAIN_SECONDS
            events = kqueue.control(None, 8, max(0.0, min(0.05, deadline - now)))
            # Kernel NOTE_EXIT is the cutoff.  It wins over every other event
            # returned in the same batch, independent of reaper scheduling.
            if any(event.filter == select.KQ_FILTER_PROC and event.fflags & select.KQ_NOTE_EXIT for event in events):
                cutoff_at = cutoff_at or time.monotonic()
            for event in events:
                if event.filter == select.KQ_FILTER_PROC:
                    continue
                if event.filter == select.KQ_FILTER_READ and event.ident == parent_sock.fileno():
                    try:
                        chunk = parent_sock.recv(65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        adapter_eof = True
                        _delete_kevent(kqueue, parent_sock.fileno(), select.KQ_FILTER_READ)
                        continue
                    if cutoff_at is not None:
                        adapter_error = "ADAPTER_LATE"
                        continue
                    if adapter is not None or adapter_error is not None:
                        adapter_error = "ADAPTER_MALFORMED"
                        continue
                    buf.extend(chunk)
                    if len(buf) < 4:
                        continue
                    length = int.from_bytes(buf[:4], "big")
                    if length == 0 or length > _MAX_ADAPTER_BYTES:
                        adapter_error = "ADAPTER_MALFORMED"
                        continue
                    if len(buf) < 4 + length:
                        continue
                    if len(buf) != 4 + length:
                        adapter_error = "ADAPTER_MALFORMED"
                        continue
                    raw = bytes(buf[4:])
                    try:
                        adapter = load_canonical_json(raw)
                        if canonical_json_bytes(adapter) != raw:
                            raise ValueError()
                    except Exception:
                        adapter_error = "ADAPTER_MALFORMED"
                        adapter = None
                        continue
                    kqueue.control([select.kevent(parent_sock.fileno(), filter=select.KQ_FILTER_WRITE, flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE)], 0, 0)
                elif event.filter == select.KQ_FILTER_WRITE and event.ident == parent_sock.fileno():
                    if cutoff_at is not None or adapter is None or ack_offset == 4:
                        continue
                    try:
                        count = _send_ack(parent_sock, memoryview(b"ACK!")[ack_offset:])
                    except BlockingIOError:
                        continue
                    except OSError:
                        adapter_error = "ADAPTER_MALFORMED"
                        _delete_kevent(kqueue, parent_sock.fileno(), select.KQ_FILTER_WRITE)
                        continue
                    if count <= 0:
                        raise Attempt0RunnerError("ACK_FAILED")
                    ack_offset += count
                    if ack_offset == 4:
                        _delete_kevent(kqueue, parent_sock.fileno(), select.KQ_FILTER_WRITE)
                elif event.filter == select.KQ_FILTER_READ and event.ident == out_read:
                    try:
                        chunk = os.read(out_read, 65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        output_eof = True
                        _delete_kevent(kqueue, out_read, select.KQ_FILTER_READ)
                    else:
                        output += len(chunk)
                        if output > _MAX_OUTPUT_BYTES and not output_limit:
                            output_limit = True
                            if term_at is None:
                                _signal_group(pid, signal.SIGTERM)
                                term_at = time.monotonic()
        if not adapter_eof or not output_eof:
            raise Attempt0RunnerError("TERMINAL_DRAIN_INCOMPLETE")
        if not _process_group_gone(pid, leader_reaped=bool(slot)):
            raise Attempt0RunnerError("PROCESS_GROUP_CLEANUP_FAILED")
        if buf and adapter is None and adapter_error is None:
            adapter_error = "ADAPTER_MALFORMED"
        if adapter is not None and ack_offset != 4 and adapter_error is None:
            adapter_error = "ADAPTER_MALFORMED"
        reaper.join(1.0)
        if not slot:
            raise Attempt0RunnerError("REAP_FAILED")
        child_reaped = True
        rc = _exit_code(slot[0])
        if output_limit:
            decision = adjudicate_parent_event("OUTPUT_LIMIT", rc, layout.run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index)
        elif timed_out:
            decision = adjudicate_parent_event("HARD_TIMEOUT", rc, layout.run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index)
        elif adapter_error == "ADAPTER_LATE":
            decision = _infra("ADAPTER_LATE", rc, layout.run_id, attempt_index)
        elif adapter_error is not None:
            decision = adjudicate_adapter_attempt({}, rc, layout.run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index)
        elif adapter is None:
            decision = adjudicate_adapter_attempt(None, rc, layout.run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index)
        else:
            decision = adjudicate_adapter_attempt(adapter, rc, layout.run_id, _SUITE_ID, _ENTRYPOINT_ID, attempt_index)
    except BaseException:
        child_reaped = child_reaped or bool(slot)
        if pid is not None and not child_reaped:
            try: _signal_group(pid, signal.SIGTERM)
            except Attempt0RunnerError: pass
            if reaper_started:
                reaped.wait(0.10)
                try:
                    _signal_group(pid, signal.SIGKILL)
                except Attempt0RunnerError:
                    pass
                # Once the sole reaper starts, no fallback owner may call
                # waitpid.  After KILL, wait for that owner to finish and join
                # it before this function can return or raise.
                reaped.wait()
                if reaper is not None: reaper.join()
                child_reaped = bool(slot)
                if not child_reaped:
                    raise Attempt0RunnerError("REAP_FAILED")
            else:
                # kqueue creation/registration or Thread.start failed.  No
                # other owner may reap, so this path performs the sole wait.
                time.sleep(0.05)
                try: _signal_group(pid, signal.SIGKILL)
                except Attempt0RunnerError: pass
                try:
                    while True:
                        try:
                            _, status = os.waitpid(pid, 0)
                            break
                        except InterruptedError:
                            continue
                    slot.append(status)
                    reaped.set()
                    child_reaped = True
                except OSError as error:
                    raise Attempt0RunnerError("REAP_FAILED") from error
        elif pid is not None:
            try: _signal_group(pid, signal.SIGTERM)
            except Attempt0RunnerError: pass
        raise
    finally:
        if kqueue is not None:
            kqueue.close()
        if parent_sock is not None:
            parent_sock.close()
        _close_fds([out_read])
    publish(decision)
    return decision


def _run_attempt0(*, repo_root: str | os.PathLike[str], layout: RunLayout, scenario: str = "normal") -> AttemptDecisionV1:
    return _run_attempt(
        repo_root=repo_root, layout=layout, attempt_index=0, scenario=scenario,
    )


def run_attempt0(*, repo_root: str | os.PathLike[str], layout: RunLayout) -> AttemptDecisionV1:
    """Run the one fixed RUE-05A normal attempt; no caller-controlled fixture or argv."""
    return _run_attempt0(repo_root=repo_root, layout=layout)
