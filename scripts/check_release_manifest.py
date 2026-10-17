#!/usr/bin/env python3
"""Check core crate version consistency and an optional release tag (Python 3.11+)."""

import argparse
import pathlib
import tomllib


def check_release(root, tag=None):
    with (root / "Cargo.toml").open("rb") as source:
        manifest = tomllib.load(source)
    with (root / "Cargo.lock").open("rb") as source:
        lockfile = tomllib.load(source)
    package = manifest["package"]
    name, version = package["name"], package["version"]
    if name != "rustfs-erasure-codec" or not isinstance(version, str) or not version:
        raise ValueError("expected the core rustfs-erasure-codec package and a version")
    if manifest["workspace"]["dependencies"][name]["version"] != version:
        raise ValueError("core package and workspace dependency versions differ")
    locked = [entry["version"] for entry in lockfile["package"] if entry["name"] == name]
    if locked != [version]:
        raise ValueError("core package and lockfile versions differ")
    if tag is not None and tag not in (version, "v" + version):
        raise ValueError(f"release tag must be {version} or v{version}")
    return version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--tag", help="Actual tag ref name, not a branch name")
    args = parser.parse_args()
    try:
        version = check_release(args.root, args.tag)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(f"core release manifests consistent: {version}")


if __name__ == "__main__":
    main()
