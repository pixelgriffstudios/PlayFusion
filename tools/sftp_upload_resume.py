#!/usr/bin/env python3
"""Resume a large SFTP upload and report periodic progress."""

from __future__ import annotations

import argparse
import os
import time

import paramiko


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source")
    parser.add_argument("destination")
    parser.add_argument("--host", required=True)
    parser.add_argument("--user", required=True)
    parser.add_argument(
        "--password",
        default=os.environ.get("SUPER_KAZETA_SFTP_PASSWORD"),
    )
    parser.add_argument(
        "--restart",
        action="store_true",
        help="replace an existing remote file instead of resuming it",
    )
    args = parser.parse_args()

    if not args.password:
        parser.error(
            "--password or the SUPER_KAZETA_SFTP_PASSWORD environment variable is required"
        )

    total = os.path.getsize(args.source)
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(
        args.host,
        username=args.user,
        password=args.password,
        timeout=15,
    )
    sftp = client.open_sftp()
    try:
        if args.restart:
            try:
                sftp.remove(args.destination)
            except FileNotFoundError:
                pass
        try:
            offset = sftp.stat(args.destination).st_size
        except FileNotFoundError:
            offset = 0
        if offset > total:
            raise RuntimeError("remote file is larger than the local source")

        started = time.monotonic()
        last_report = started
        transferred = offset
        print(f"RESUME_BYTES={offset}", flush=True)

        with open(args.source, "rb") as source:
            source.seek(offset)
            with sftp.open(args.destination, "ab") as destination:
                destination.set_pipelined(True)
                while True:
                    block = source.read(1024 * 1024)
                    if not block:
                        break
                    destination.write(block)
                    transferred += len(block)
                    now = time.monotonic()
                    if now - last_report >= 5:
                        elapsed = max(now - started, 0.001)
                        rate = (transferred - offset) / elapsed
                        remaining = total - transferred
                        eta = remaining / rate if rate else 0
                        print(
                            f"PROGRESS={transferred * 100 / total:.1f}% "
                            f"RATE_MIB={rate / 1024 / 1024:.1f} ETA_SEC={eta:.0f}",
                            flush=True,
                        )
                        last_report = now
        remote_size = sftp.stat(args.destination).st_size
        if remote_size != total:
            raise RuntimeError(
                f"size mismatch after upload: remote={remote_size}, local={total}"
            )
        print(f"COMPLETE_BYTES={remote_size}", flush=True)
        return 0
    finally:
        sftp.close()
        client.close()


if __name__ == "__main__":
    raise SystemExit(main())
