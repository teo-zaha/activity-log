"""Check workspace versions and prepare a release with git-cliff."""

import argparse
import json
import re
import subprocess
import tomllib
from pathlib import Path


MANIFEST = Path("Cargo.toml")
VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


def version_parts(value):
    match = re.fullmatch(VERSION, value)
    if not match:
        raise SystemExit(f"Expected a version like 1.2.3, got {value!r}")
    return tuple(map(int, match.groups()))


def check_versions():
    workspace = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))["workspace"]
    version = workspace["package"]["version"]
    version_parts(version)
    lock = tomllib.loads(Path("Cargo.lock").read_text(encoding="utf-8"))
    local_versions = {
        package["name"]: package["version"]
        for package in lock["package"]
        if "source" not in package
    }
    for member in workspace["members"]:
        path = Path(member) / "Cargo.toml"
        package = tomllib.loads(path.read_text(encoding="utf-8"))["package"]
        inheritance = package.get("version")
        if not isinstance(inheritance, dict) or inheritance.get("workspace") is not True:
            raise SystemExit(f"{path} must inherit the workspace version")
        if local_versions.get(package["name"]) != version:
            raise SystemExit(f"Cargo.lock must record {package['name']} {version}")

    for path in Path("desktop").glob("tauri*.conf.json"):
        if "version" in json.loads(path.read_text(encoding="utf-8")):
            raise SystemExit(f"{path} must take its version from Cargo")
    for directory in (".", "dashboard", "desktop"):
        path = Path(directory) / "package.json"
        package = json.loads(path.read_text(encoding="utf-8"))
        if "version" in package or package.get("private") is not True:
            raise SystemExit(f"{path} must be private and unversioned")

    return version


def output(*command):
    return subprocess.check_output(command, text=True).strip()


def cliff(*args):
    return output("git-cliff", "--use-branch-tags", *args)


def next_version(current, override):
    tags = {
        tag[1:]
        for tag in output("git", "tag", "--list", "v*").splitlines()
        if re.fullmatch("v" + VERSION, tag)
    }
    latest = max(map(version_parts, tags), default=(-1, -1, -1))
    current_parts = version_parts(current)
    if override:
        requested = version_parts(override)
        if requested <= latest or requested < current_parts:
            raise SystemExit(
                "The override must be newer than every release tag and at least the Cargo version"
            )
        return override
    if current_parts > latest:
        return current

    bumped_tag = cliff("--bumped-version")
    if not bumped_tag:
        return None
    if not bumped_tag.startswith("v"):
        raise SystemExit(f"Expected a v-prefixed release tag, got {bumped_tag!r}")
    bumped = bumped_tag[1:]
    bumped_parts = version_parts(bumped)
    if bumped in tags and bumped_parts == latest:
        return None
    if bumped_parts <= latest:
        raise SystemExit(f"{bumped_tag} is older than an existing release tag")
    return bumped


def write_version(version):
    before, section, after = MANIFEST.read_text(encoding="utf-8").partition(
        "[workspace.package]\n"
    )
    workspace_package, following_section, rest = after.partition("\n[")
    workspace_package, changed = re.subn(
        r'^version\s*=\s*"[^"]+"',
        f'version = "{version}"',
        workspace_package,
        flags=re.MULTILINE,
    )
    if changed != 1:
        raise SystemExit("Expected one workspace version in Cargo.toml")
    MANIFEST.write_text(
        before + section + workspace_package + following_section + rest,
        encoding="utf-8",
    )
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    check_versions()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    commands.add_parser("changelog")
    commands.add_parser("preview").add_argument("--version")
    prepare = commands.add_parser("prepare")
    prepare.add_argument("--version")
    prepare.add_argument("--notes", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "changelog":
        cliff("--output", "CHANGELOG.md")
        return

    current = check_versions()
    if args.command == "check":
        return

    version = next_version(current, args.version)
    if args.command == "preview":
        print(f"Next release: v{version}" if version else "No pending release")
        print(cliff("--unreleased", "--strip", "header"))
        return
    if not version:
        return

    tag = f"v{version}"
    notes = cliff("--unreleased", "--tag", tag, "--strip", "all")
    if not notes.startswith(f"## [{version}]"):
        raise SystemExit(f"git-cliff found no unreleased changes for {tag}")
    args.notes.write_text(
        "Windows and macOS builds are unsigned. Windows SmartScreen and macOS "
        "Gatekeeper may warn when you open them. The macOS build is for Apple "
        "silicon (arm64).\n\n"
        f"{notes}\n",
        encoding="utf-8",
    )
    cliff("--tag", tag, "--output", "CHANGELOG.md")
    if version != current:
        write_version(version)
    print(tag)


if __name__ == "__main__":
    main()
