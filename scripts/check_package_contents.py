#!/usr/bin/env python3
"""Verify the release archives carry the canonical licenses and matching versions."""

import argparse
from pathlib import Path
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    args = parser.parse_args()
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = workspace["workspace"]["package"]["version"]
    for name in ("framefinery-api", "framefinery-codecs", "framefinery"):
        prefix = f"{name}-{version}"
        path = args.target_dir / "package" / f"{prefix}.crate"
        with tarfile.open(path) as archive:
            for document in ("LICENSE", "COMMERCIAL-LICENSE.md"):
                member = archive.extractfile(f"{prefix}/{document}")
                if member is None or member.read() != (ROOT / document).read_bytes():
                    raise SystemExit(f"{path}: missing or stale {document}")
            manifest = archive.extractfile(f"{prefix}/Cargo.toml")
            if manifest is None:
                raise SystemExit(f"{path}: missing manifest")
            package = tomllib.loads(manifest.read().decode())
            if package["package"]["version"] != version:
                raise SystemExit(f"{path}: inconsistent package version")
            for dependency in ("framefinery-api", "framefinery-codecs"):
                if dependency in package.get("dependencies", {}):
                    if package["dependencies"][dependency]["version"] != version:
                        raise SystemExit(f"{path}: inconsistent {dependency} version")
        print(f"PASS: {path} licenses and dependency versions")


if __name__ == "__main__":
    main()
