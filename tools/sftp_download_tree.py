#!/usr/bin/env python3
"""Download a remote directory tree over SFTP while preserving relative paths."""

from __future__ import annotations

import argparse
import os
import posixpath
import stat
import time

import paramiko


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source")
    parser.add_argument("destination")
    parser.add_argument("--host", required=True)
    parser.add_argument("--user", required=True)
    parser.add_argument("--password", required=True)
    args = parser.parse_args()

    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(args.host, username=args.user, password=args.password, timeout=15)
    sftp = client.open_sftp()
    try:
        files: list[tuple[str, str, int]] = []

        def walk(remote_dir: str, local_dir: str) -> None:
            os.makedirs(local_dir, exist_ok=True)
            for entry in sftp.listdir_attr(remote_dir):
                remote_path = posixpath.join(remote_dir, entry.filename)
                local_path = os.path.join(local_dir, entry.filename)
                if stat.S_ISDIR(entry.st_mode):
                    walk(remote_path, local_path)
                elif stat.S_ISREG(entry.st_mode):
                    files.append((remote_path, local_path, entry.st_size))

        walk(args.source.rstrip("/"), os.path.abspath(args.destination))
        total = sum(size for _, _, size in files)
        completed = 0
        started = time.monotonic()
        last_report = started

        for remote_path, local_path, size in files:
            os.makedirs(os.path.dirname(local_path), exist_ok=True)
            with sftp.open(remote_path, "rb") as source, open(local_path, "wb") as target:
                while True:
                    block = source.read(1024 * 1024)
                    if not block:
                        break
                    target.write(block)
                    completed += len(block)
                    now = time.monotonic()
                    if now - last_report >= 5:
                        elapsed = max(now - started, 0.001)
                        rate = completed / elapsed
                        eta = (total - completed) / rate if rate else 0
                        print(
                            f"PROGRESS={completed * 100 / max(total, 1):.1f}% "
                            f"RATE_MIB={rate / 1024 / 1024:.1f} ETA_SEC={eta:.0f}",
                            flush=True,
                        )
                        last_report = now
            if os.path.getsize(local_path) != size:
                raise RuntimeError(f"size mismatch for {remote_path}")

        print(f"COMPLETE_FILES={len(files)} COMPLETE_BYTES={completed}", flush=True)
        return 0
    finally:
        sftp.close()
        client.close()


if __name__ == "__main__":
    raise SystemExit(main())
