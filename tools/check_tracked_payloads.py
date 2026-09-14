#!/usr/bin/env python3
"""Reject compiled native payloads in the tracked tree, including containers."""
from __future__ import annotations

import base64
import gzip
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path.cwd()
MAX_DEPTH = 4
MAX_EXPANDED = 64 * 1024 * 1024


class PayloadError(Exception):
    pass


def kind(data: bytes) -> str | None:
    if data.startswith(b"\x7fELF"):
        return "ELF"
    if data.startswith((b"MZ", b"\x00asm")):
        return "PE/COFF" if data.startswith(b"MZ") else None
    if data[:2] in {b"\x4c\x01", b"\x64\x86", b"\x64\xaa"}:
        return "PE/COFF"
    if data.startswith((b"!<arch>\n", b"!<thin>\n")):
        return "thin Unix archive" if data.startswith(b"!<thin>") else "Unix archive"
    if len(data) >= 4 and data[:4] in {b"\xfe\xed\xfa\xce", b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}:
        return "Mach-O/fat binary"
    return None


def fail(path: str, chain: list[str], detected: str) -> None:
    member = " -> ".join([path, *chain])
    raise PayloadError(f"tracked native payload: path={path} member={member} kind={detected}")


def scan(path: str, data: bytes, chain: list[str] = [], depth: int = 0) -> None:
    if len(data) > MAX_EXPANDED:
        raise PayloadError(f"tracked container exceeds expanded-byte limit: path={path} member={' -> '.join([path, *chain])}")
    if detected := kind(data):
        fail(path, chain, detected)
    try:
        is_gzip = data.startswith(b"\x1f\x8b")
        is_zip = zipfile.is_zipfile(io.BytesIO(data))
        is_tar = len(data) >= 262 and data[257:262] == b"ustar"
        if depth >= MAX_DEPTH and (is_gzip or is_zip or is_tar):
            raise PayloadError(f"tracked container exceeds nesting-depth limit: path={path} member={' -> '.join([path, *chain])}")
        if depth >= MAX_DEPTH:
            return
        if is_gzip:
            scan(path, gzip.decompress(data), [*chain, "gzip"], depth + 1)
        elif is_zip:
            with zipfile.ZipFile(io.BytesIO(data)) as container:
                for item in container.infolist():
                    if item.is_dir():
                        continue
                    if item.file_size > MAX_EXPANDED:
                        raise PayloadError(f"tracked container exceeds expanded-byte limit: path={path} member={' -> '.join([path, *chain, item.filename])}")
                    scan(path, container.read(item), [*chain, item.filename], depth + 1)
        elif is_tar:
            with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as container:
                for item in container:
                    if not item.isfile():
                        continue
                    if item.size > MAX_EXPANDED:
                        raise PayloadError(f"tracked container exceeds expanded-byte limit: path={path} member={' -> '.join([path, *chain, item.name])}")
                    source = container.extractfile(item)
                    if source is None:
                        raise PayloadError(f"cannot inspect tracked container: path={path} member={item.name}")
                    scan(path, source.read(), [*chain, item.name], depth + 1)
    except (OSError, EOFError, tarfile.TarError, zipfile.BadZipFile) as error:
        raise PayloadError(f"cannot inspect tracked container: path={path} member={' -> '.join([path, *chain])}: {error}") from error
    try:
        text = data.decode("ascii")
    except UnicodeDecodeError:
        return
    compact = "".join(text.split())
    if len(compact) >= 4 and len(compact) % 4 == 0:
        try:
            decoded = base64.b64decode(compact, validate=True)
        except ValueError:
            return
        if decoded != data:
            scan(path, decoded, [*chain, "base64"], depth + 1)


def main() -> int:
    listed = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT)
    try:
        for raw in listed.split(b"\0"):
            if not raw:
                continue
            relative = Path(raw.decode("utf-8", "surrogateescape"))
            source = ROOT / relative
            if source.is_file() and not source.is_symlink():
                scan(str(relative), source.read_bytes())
    except PayloadError as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
