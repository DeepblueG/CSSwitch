"""Dedicated, versioned RUE-05A child fixture.

It is intentionally not a general test launcher.  The parent supplies only
the attempt identity and a single inherited AF_UNIX descriptor.  Test-only
scenarios are private to this fixture/runner pair.
"""
from __future__ import annotations

import os
import socket
import sys
import time


def _frame(value: bytes) -> bytes:
    return len(value).to_bytes(4, "big") + value


def _recv_ack(peer: socket.socket) -> bool:
    peer.settimeout(0.75)
    value = bytearray()
    try:
        while len(value) < 4:
            chunk = peer.recv(4 - len(value))
            if not chunk:
                return False
            value.extend(chunk)
    except OSError:
        return False
    return bytes(value) == b"ACK!"


def main(argv: list[str]) -> int:
    if len(argv) != 6 or argv[1] != "--adapter-fd":
        return 64
    fd = int(argv[2])
    run_id, suite_id, entrypoint_id = argv[3], argv[4], argv[5]
    if entrypoint_id != os.environ["RUE05A_ENTRYPOINT"]:
        return 65
    scenario = os.environ.get("RUE05A_PRIVATE_SCENARIO", "normal")
    peer = socket.socket(fileno=fd)
    try:
        if scenario == "timeout":
            time.sleep(10)
            return 0
        if scenario == "output-limit":
            sys.stdout.write("x" * 131072)
            sys.stdout.flush()
            return 0
        if scenario == "missing":
            return 0
        if scenario == "malformed":
            peer.sendall(_frame(b"{}"))
            time.sleep(0.05)
            return 0
        if scenario == "oversize":
            peer.sendall((65537).to_bytes(4, "big"))
            time.sleep(0.05)
            return 0
        if scenario == "partial-header":
            peer.sendall(b"\x00\x00")
            time.sleep(0.05)
            return 0
        if scenario == "partial-payload":
            peer.sendall((10).to_bytes(4, "big") + b"abc")
            time.sleep(0.05)
            return 0
        adapter = (
            b'{"attempt_index":0,"classification_hint":"NONE","entrypoint_id":"' + entrypoint_id.encode("ascii")
            + b'","outcome_hint":"PASS","reason_code":"NONE","run_id":"' + run_id.encode("ascii")
            + b'","schema":"adapter-result.v1","suite_id":"' + suite_id.encode("ascii") + b'"}'
        )
        if scenario == "fake-marker":
            sys.stdout.write("PASS 0\\n")
            sys.stdout.flush()
            peer.sendall(_frame(adapter))
            return 7 if _recv_ack(peer) else 66
        if scenario == "extra":
            peer.sendall(_frame(adapter) + b"x")
            time.sleep(0.05)
            return 0
        if scenario == "late":
            parent_pid = os.getpid()
            child = os.fork()
            if child:
                return 0
            deadline = time.monotonic() + 0.20
            while os.getppid() == parent_pid:
                if time.monotonic() >= deadline:
                    os._exit(67)
                time.sleep(0.001)
            peer.sendall(_frame(adapter))
            os._exit(0)
        if scenario == "hold-after-frame":
            peer.sendall(_frame(adapter))
            if not _recv_ack(peer):
                return 66
            child = os.fork()
            if child:
                return 0
            time.sleep(10)
            os._exit(0)
        if scenario == "terminal-drain-incomplete":
            peer.sendall(_frame(adapter))
            if not _recv_ack(peer):
                return 66
            child = os.fork()
            if child:
                return 0
            os.setsid()
            time.sleep(0.45)
            os._exit(0)
        if scenario == "closed-fd-descendant":
            peer.sendall(_frame(adapter))
            if not _recv_ack(peer):
                return 66
            child = os.fork()
            if child:
                return 0
            peer.close()
            os.close(1)
            os.close(2)
            time.sleep(10)
            os._exit(0)
        peer.sendall(_frame(adapter))
        return 0 if _recv_ack(peer) else 66
    finally:
        peer.close()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
