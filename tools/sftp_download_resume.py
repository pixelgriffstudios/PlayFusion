#!/usr/bin/env python3
"""Resume an SFTP download and verify the completed file size."""

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
    parser.add_argument("--password", required=True)
    parser.add_argument(
        "--restart",
        action="store_true",
        help="discard an existing partial destination instead of resuming it",
    )
    args = parser.parse_args()

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
        total = sftp.stat(args.source).st_size
        os.makedirs(os.path.dirname(os.path.abspath(args.destination)), exist_ok=True)
        if args.restart and os.path.exists(args.destination):
            os.remove(args.destination)
        offset = os.path.getsize(args.destination) if os.path.exists(args.destination) else 0
        if offset > total:
            raise RuntimeError("local file is larger than the remote source")

        started = time.monotonic()
        last_report = started
        transferred = offset
        print(f"RESUME_BYTES={offset}", flush=True)
        with sftp.open(args.source, "rb") as source:
            source.seek(offset)
            # Pipeline remote reads so large image downloads are limited by
            # network/disk throughput rather than one SFTP round trip per
            # block. Capping the request window avoids Paramiko spawning an
            # unbounded number of background requests.
            source.prefetch(
                file_size=total,
                max_concurrent_requests=64,
            )
            with open(args.destination, "ab") as destination:
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
        completed = os.path.getsize(args.destination)
        if completed != total:
            raise RuntimeError(
                f"size mismatch after download: remote={total}, local={completed}"
            )
        print(f"COMPLETE_BYTES={completed}", flush=True)
        return 0
    finally:
        sftp.close()
        client.close()


if __name__ == "__main__":
    raise SystemExit(main())
