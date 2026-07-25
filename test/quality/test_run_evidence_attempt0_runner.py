"""Focused RUE-05A adversarial tests; all process state is temporary."""
from __future__ import annotations

import hashlib
import contextlib
import os
import shutil
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

import test.quality.run_evidence.atomic_store as store
from test.quality.run_evidence.atomic_store import RunStoreError, create_run_layout
import test.quality.run_evidence.attempt0_runner as runner
from test.quality.run_evidence.attempt0_runner import _run_attempt0, run_attempt0


class Attempt0RunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=os.path.realpath(tempfile.gettempdir()))
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"; self.state = self.base / "state"; self.evidence = self.base / "evidence"
        self.state.mkdir(mode=0o700); self.evidence.mkdir(mode=0o700)
        self.fixture = self.repo / "test/quality/fixtures/run_evidence/attempt0_fixture.py"
        self.fixture.parent.mkdir(parents=True)
        source = Path(__file__).parent / "fixtures/run_evidence/attempt0_fixture.py"
        shutil.copyfile(source, self.fixture)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _layout(self):
        layout = create_run_layout(str(self.state), str(self.evidence))
        self.addCleanup(lambda: self._close(layout))
        raw = self.fixture.read_bytes()
        manifest = {
            "schema": "source-snapshot-manifest.v1", "run_id": layout.run_id, "head_sha": "a" * 40,
            "snapshot_mode": "clean-commit", "entry_count": 1, "total_bytes": len(raw),
            "entries": [{"path": "test/quality/fixtures/run_evidence/attempt0_fixture.py", "type": "file", "mode": "100644", "size": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}],
        }
        with layout.snapshot_capture_lease() as lease:
            ticket = layout.publish_snapshot_manifest(manifest, expected_head_sha="a" * 40, lease=lease)
            layout.linearize_snapshot_success(ticket, lease=lease)
        return layout

    @staticmethod
    def _close(layout) -> None:
        try: layout.close()
        except RunStoreError: pass

    def test_01_happy_path_is_fixed_public_runner_and_private_record(self):
        layout = self._layout()
        decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code, decision.attempt_record.process_exit), ("PASS", "NONE", 0))
        self.assertTrue((Path(layout.state_path) / "attempts/attempt-0.json").is_file())

    def test_02_stdout_marker_and_nonzero_rc_cannot_green(self):
        layout = self._layout()
        decision = _run_attempt0(repo_root=str(self.repo), layout=layout, scenario="fake-marker")
        self.assertEqual((decision.disposition, decision.reason_code, decision.attempt_record.process_exit), ("INFRA", "EXIT_STATUS_MISMATCH", 7))

    def test_03_missing_malformed_extra_timeout_and_output_limit_are_fail_closed(self):
        cases = {
            "missing": ("INFRA", "ADAPTER_MISSING"), "malformed": ("INFRA", "ADAPTER_MALFORMED"),
            "oversize": ("INFRA", "ADAPTER_MALFORMED"), "extra": ("INFRA", "ADAPTER_MALFORMED"),
            "partial-header": ("INFRA", "ADAPTER_MALFORMED"), "partial-payload": ("INFRA", "ADAPTER_MALFORMED"),
            "late": ("INFRA", "ADAPTER_LATE"), "timeout": ("HARD_TIMEOUT", "PROCESS_TIMEOUT"),
            "output-limit": ("INFRA", "OUTPUT_LIMIT"),
        }
        for scenario, expected in cases.items():
            with self.subTest(scenario=scenario):
                layout = self._layout()
                decision = _run_attempt0(repo_root=str(self.repo), layout=layout, scenario=scenario)
                self.assertEqual((decision.disposition, decision.reason_code), expected)
                self.assertIsNotNone(decision.attempt_record.process_exit)

    def test_04_source_replacement_before_copy_is_not_executed(self):
        layout = self._layout()
        self.fixture.write_text("raise SystemExit(0)\n")
        decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code), ("INFRA", "TOOL_IDENTITY_CHANGED"))

        for replacement_mode in (0o600, 0o755):
            with self.subTest(replacement_mode=oct(replacement_mode)):
                os.chmod(self.fixture, 0o644)
                layout = self._layout()
                os.chmod(self.fixture, replacement_mode)
                decision = run_attempt0(repo_root=str(self.repo), layout=layout)
                self.assertEqual(
                    (decision.disposition, decision.reason_code),
                    ("INFRA", "TOOL_IDENTITY_CHANGED"),
                )

    def test_05_started_claim_and_publication_replay_cannot_green(self):
        layout = self._layout()
        run_attempt0(repo_root=str(self.repo), layout=layout)
        with self.assertRaises(RunStoreError) as started:
            run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual(started.exception.code, "ATTEMPT_DUPLICATE")

    def test_06_cache_name_replacement_after_held_fd_cannot_change_execution(self):
        layout = self._layout()
        real_move = runner._moved_child_fd
        swapped = {"done": False}

        def replace_cache(fd):
            if not swapped["done"]:
                swapped["done"] = True
                replacement = Path(layout.state_path) / "cache/replacement"
                replacement.write_text("raise SystemExit(99)\n")
                os.replace(replacement, Path(layout.state_path) / "cache/attempt0-fixture.py")
            return real_move(fd)

        with mock.patch.object(runner, "_moved_child_fd", side_effect=replace_cache):
            decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code, decision.attempt_record.process_exit), ("PASS", "NONE", 0))

    def test_07_note_exit_cutoff_is_independent_of_delayed_reaper(self):
        layout = self._layout()
        real_wait = runner._wait_once
        calls = []

        def delayed(pid, slot, done):
            time.sleep(0.20)
            calls.append(pid)
            real_wait(pid, slot, done)

        with mock.patch.object(runner, "_wait_once", side_effect=delayed):
            decision = _run_attempt0(repo_root=str(self.repo), layout=layout, scenario="late")
        self.assertEqual((decision.disposition, decision.reason_code), ("INFRA", "ADAPTER_LATE"))
        self.assertEqual(len(calls), 1)

    def test_08_ack_short_writes_complete_and_zero_write_fails_without_publication(self):
        layout = self._layout()
        writes = []

        def one_byte(peer, remaining):
            writes.append(len(remaining))
            return peer.send(remaining[:1])

        with mock.patch.object(runner, "_send_ack", side_effect=one_byte):
            decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code), ("PASS", "NONE"))
        self.assertGreaterEqual(len(writes), 4)

        layout = self._layout()
        with mock.patch.object(runner, "_send_ack", return_value=0):
            with self.assertRaises(runner.Attempt0RunnerError) as raised:
                run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual(raised.exception.code, "ACK_FAILED")
        self.assertFalse((Path(layout.state_path) / "attempts/attempt-0.json").exists())

    def test_09_descendant_holding_fds_is_killed_and_drained_before_pass(self):
        layout = self._layout()
        started = time.monotonic()
        decision = _run_attempt0(repo_root=str(self.repo), layout=layout, scenario="hold-after-frame")
        elapsed = time.monotonic() - started
        self.assertEqual((decision.disposition, decision.reason_code), ("PASS", "NONE"))
        self.assertGreaterEqual(elapsed, 0.05)
        self.assertLess(elapsed, 1.0)

        layout = self._layout()
        with self.assertRaises(runner.Attempt0RunnerError) as incomplete:
            _run_attempt0(repo_root=str(self.repo), layout=layout, scenario="terminal-drain-incomplete")
        self.assertEqual(incomplete.exception.code, "TERMINAL_DRAIN_INCOMPLETE")
        self.assertFalse((Path(layout.state_path) / "attempts/attempt-0.json").exists())
        time.sleep(0.25)  # The deliberately escaped test descendant exits at 0.45s.

        layout = self._layout(); pids = []; real_spawn = runner.os.posix_spawn
        def capture_spawn(*args, **kwargs):
            pid = real_spawn(*args, **kwargs); pids.append(pid); return pid
        with mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn):
            decision = _run_attempt0(repo_root=str(self.repo), layout=layout, scenario="closed-fd-descendant")
        self.assertEqual((decision.disposition, decision.reason_code), ("PASS", "NONE"))
        with self.assertRaises(ProcessLookupError):
            os.killpg(pids[0], 0)

    def test_10_waitpid_once_reaps_child_and_fixed_fd_actions_do_not_close_destinations(self):
        layout = self._layout()
        real_spawn, real_waitpid = runner.os.posix_spawn, runner.os.waitpid
        pids, waits = [], []

        def capture_spawn(*args, **kwargs):
            pid = real_spawn(*args, **kwargs); pids.append(pid); return pid

        def capture_wait(pid, options):
            waits.append((pid, options)); return real_waitpid(pid, options)

        with mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn), \
             mock.patch.object(runner.os, "waitpid", side_effect=capture_wait):
            self.assertEqual(run_attempt0(repo_root=str(self.repo), layout=layout).disposition, "PASS")
        self.assertEqual(waits, [(pids[0], 0)])
        with self.assertRaises(ProcessLookupError):
            os.kill(pids[0], 0)
        actions = runner._spawn_actions(198, 199, 198, 200, 201, 202)
        closed = {action[1] for action in actions if action[0] == os.POSIX_SPAWN_CLOSE}
        self.assertNotIn(198, closed); self.assertNotIn(199, closed)
        self.assertIn((os.POSIX_SPAWN_DUP2, 200, 198), actions)
        self.assertIn((os.POSIX_SPAWN_DUP2, 201, 199), actions)

    def test_11_spawn_and_event_loop_failures_close_fds_and_reap(self):
        layout = self._layout(); before = len(os.listdir("/dev/fd"))
        with mock.patch.object(runner.os, "pipe", side_effect=OSError("pipe")):
            decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code), ("INFRA", "EXEC_FAILED"))
        self.assertLessEqual(len(os.listdir("/dev/fd")), before)

        layout = self._layout(); before = len(os.listdir("/dev/fd"))
        with mock.patch.object(runner.os, "posix_spawn", side_effect=OSError("spawn")):
            decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual((decision.disposition, decision.reason_code), ("INFRA", "EXEC_FAILED"))
        self.assertLessEqual(len(os.listdir("/dev/fd")), before)

        layout = self._layout(); before = len(os.listdir("/dev/fd")); pids = []
        real_spawn = runner.os.posix_spawn
        def capture_spawn(*args, **kwargs):
            pid = real_spawn(*args, **kwargs); pids.append(pid); return pid
        with mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn), \
             mock.patch.object(runner, "_send_ack", side_effect=RuntimeError("unexpected")):
            with self.assertRaisesRegex(RuntimeError, "unexpected"):
                run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertLessEqual(len(os.listdir("/dev/fd")), before)
        with self.assertRaises(ProcessLookupError):
            os.kill(pids[0], 0)

        layout = self._layout(); before = len(os.listdir("/dev/fd")); real_read = runner._read_exact; calls = []
        def fail_verify(fd, size):
            calls.append(fd)
            if len(calls) == 2:
                raise runner.Attempt0RunnerError("FD_DRIFT")
            return real_read(fd, size)
        with mock.patch.object(runner, "_read_exact", side_effect=fail_verify):
            with self.assertRaises(runner.Attempt0RunnerError):
                run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertLessEqual(len(os.listdir("/dev/fd")), before)

    def test_12_uncertain_publication_raises_instead_of_returning_decision(self):
        layout = self._layout()
        uncertain = RunStoreError("PUBLISH_VERIFY_FAILED", published_may_exist=True)
        with mock.patch.object(store, "_publish", side_effect=uncertain):
            with self.assertRaises(RunStoreError) as raised:
                run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertTrue(raised.exception.published_may_exist)

    def test_13_pre_reaper_failures_use_one_synchronous_wait_and_close_fds(self):
        real_spawn, real_waitpid = runner.os.posix_spawn, runner.os.waitpid

        class RegisterFailure:
            def __init__(self): self.inner = runner.select.kqueue()
            def control(self, *args, **kwargs): raise OSError("register")
            def close(self): self.inner.close()

        cases = (
            ("create", True, lambda stack: stack.enter_context(mock.patch.object(runner.select, "kqueue", side_effect=OSError("create")))),
            ("register", False, lambda stack: stack.enter_context(mock.patch.object(runner.select, "kqueue", return_value=RegisterFailure()))),
            ("thread-start", False, lambda stack: stack.enter_context(mock.patch.object(runner.threading.Thread, "start", side_effect=RuntimeError("start")))),
        )
        for label, inject_eintr, install in cases:
            with self.subTest(label=label):
                layout = self._layout(); before = len(os.listdir("/dev/fd")); pids, attempts, successes = [], [], []
                def capture_spawn(*args, **kwargs):
                    pid = real_spawn(*args, **kwargs); pids.append(pid); return pid
                def capture_wait(pid, options):
                    attempts.append((pid, options))
                    if inject_eintr and len(attempts) == 1:
                        raise InterruptedError()
                    value = real_waitpid(pid, options)
                    successes.append(value)
                    return value
                # Use a local ExitStack so each failure injection ends before
                # the next subtest and cannot affect fixture setup.
                with contextlib.ExitStack() as stack:
                    stack.enter_context(mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn))
                    stack.enter_context(mock.patch.object(runner.os, "waitpid", side_effect=capture_wait))
                    install(stack)
                    with self.assertRaises((OSError, RuntimeError)):
                        run_attempt0(repo_root=str(self.repo), layout=layout)
                self.assertEqual(attempts, [(pids[0], 0)] * (2 if inject_eintr else 1))
                self.assertEqual(len(successes), 1)
                self.assertLessEqual(len(os.listdir("/dev/fd")), before)
                with self.assertRaises(ProcessLookupError):
                    os.kill(pids[0], 0)

    def test_14_reaper_retries_eintr_and_started_reaper_cleanup_waits_for_completion(self):
        layout = self._layout()
        real_waitpid = runner.os.waitpid
        calls, successes = [], []

        def interrupted_once(pid, options):
            calls.append((pid, options))
            if len(calls) == 1:
                raise InterruptedError()
            value = real_waitpid(pid, options)
            successes.append(value)
            return value

        with mock.patch.object(runner.os, "waitpid", side_effect=interrupted_once):
            decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertEqual(decision.disposition, "PASS")
        self.assertEqual(len(calls), 2)
        self.assertEqual(len(successes), 1)

        layout = self._layout(); pids, waits = [], []
        real_spawn, real_waitpid, real_reaper = runner.os.posix_spawn, runner.os.waitpid, runner._wait_once
        def capture_spawn(*args, **kwargs):
            pid = real_spawn(*args, **kwargs); pids.append(pid); return pid
        def capture_wait(pid, options):
            value = real_waitpid(pid, options); waits.append(value); return value
        def delayed_reaper(pid, slot, done):
            time.sleep(1.20)
            real_reaper(pid, slot, done)
        started = time.monotonic()
        with mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn), \
             mock.patch.object(runner.os, "waitpid", side_effect=capture_wait), \
             mock.patch.object(runner, "_wait_once", side_effect=delayed_reaper), \
             mock.patch.object(runner, "_send_ack", side_effect=RuntimeError("event-loop")):
            with self.assertRaisesRegex(RuntimeError, "event-loop"):
                run_attempt0(repo_root=str(self.repo), layout=layout)
        self.assertGreaterEqual(time.monotonic() - started, 1.15)
        self.assertEqual(len(waits), 1)
        with self.assertRaises(ProcessLookupError):
            os.kill(pids[0], 0)

    def test_15_unversioned_cwd_sitecustomize_cannot_preload_before_fixture(self):
        layout = self._layout()
        unversioned_cwd = self.base / "unversioned-cwd"
        unversioned_cwd.mkdir()
        sentinel = unversioned_cwd / "sitecustomize-loaded"
        (unversioned_cwd / "sitecustomize.py").write_text(
            "from pathlib import Path\n"
            f"Path({str(sentinel)!r}).write_text('loaded')\n"
        )
        real_spawn = runner.os.posix_spawn
        spawned_argv = []

        def capture_spawn(path, argv, *args, **kwargs):
            spawned_argv.append(list(argv))
            return real_spawn(path, argv, *args, **kwargs)

        previous_cwd = os.getcwd()
        try:
            os.chdir(unversioned_cwd)
            with mock.patch.object(runner.os, "posix_spawn", side_effect=capture_spawn):
                decision = run_attempt0(repo_root=str(self.repo), layout=layout)
        finally:
            os.chdir(previous_cwd)
        self.assertEqual(
            (decision.disposition, decision.reason_code, decision.attempt_record.process_exit),
            ("PASS", "NONE", 0),
        )
        self.assertEqual(spawned_argv[0][1:3], ["-I", "-S"])
        self.assertFalse(sentinel.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
