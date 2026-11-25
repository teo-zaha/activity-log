"""Check workspace versions and prepare a release with git-cliff."""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    raise SystemExit(
        "Release commands require Python 3.11 or later. Use nix develop or install a newer Python."
    ) from None


MANIFEST = Path("Cargo.toml")
CHANGELOG = Path("CHANGELOG.md")
VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
GITHUB_CHANGELOG = re.compile(
    r"^### New contributors$|^\[Full changelog\]\(https://github\.com/"
    r"|^-.* \*\([0-9a-f]{7}\)\*(?: by @\S+| in \[#\d+\]\(https://github\.com/)",
    re.MULTILINE,
)


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
    return subprocess.check_output(command, encoding="utf-8").strip()


def cliff(*args, context=None, github_token=None, github_context=False):
    online = bool(github_token) or github_context
    command = ["git-cliff", "--use-branch-tags"]
    env = os.environ.copy()
    if github_token:
        env["GITHUB_TOKEN"] = github_token
    else:
        env.pop("GITHUB_TOKEN", None)
    if online:
        env["GIT_CLIFF_OFFLINE"] = "false"
    else:
        command.append("--offline")
    try:
        return subprocess.run(
            [*command, *args],
            check=True,
            encoding="utf-8",
            stdout=subprocess.PIPE,
            input=context,
            env=env,
        ).stdout.strip()
    except subprocess.CalledProcessError as error:
        mode = "with GitHub metadata" if online else "offline"
        raise SystemExit(f"git-cliff failed {mode} (exit {error.returncode})") from None


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
    subprocess.run(["cargo", "update", "--workspace"], check=True, stdout=sys.stderr)
    check_versions()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check").add_argument("--require-git-cliff", action="store_true")
    commands.add_parser("changelog")
    commands.add_parser("preview").add_argument("--version")
    prepare = commands.add_parser("prepare")
    prepare.add_argument("--version")
    prepare.add_argument("--notes", type=Path, required=True)
    args = parser.parse_args()

    # Keep the token out of version selection, template rendering, and Cargo.
    token = os.environ.pop("GITHUB_TOKEN", None)
    if not os.environ.get("GITHUB_REPO"):
        token = None
    if args.command == "prepare" and os.environ.get("GITHUB_ACTIONS") == "true" and not token:
        raise SystemExit("Release preparation requires GITHUB_TOKEN and GITHUB_REPO in GitHub Actions")

    if args.command == "changelog":
        if token:
            context = cliff("--context", github_token=token)
            cliff(
                "--from-context", "-", "--output", str(CHANGELOG),
                context=context, github_context=True,
            )
        else:
            if CHANGELOG.exists() and GITHUB_CHANGELOG.search(CHANGELOG.read_text(encoding="utf-8")):
                raise SystemExit(
                    "CHANGELOG.md contains GitHub data. Set GITHUB_REPO (owner/repo) and "
                    "GITHUB_TOKEN, then rerun pnpm release:changelog to preserve it."
                )
            cliff("--output", str(CHANGELOG))
        return

    current = check_versions()
    if args.command == "check":
        env = os.environ.copy()
        if args.require_git_cliff:
            env["ACTIVITY_LOG_REQUIRE_GIT_CLIFF"] = "1"
        return subprocess.run(
            [sys.executable, "-B", str(Path(__file__).with_name("release_test.py"))],
            env=env,
        ).returncode

    version = next_version(current, args.version)
    if args.command == "preview":
        print(f"Next release: v{version}" if version else "No pending release")
        if token:
            context = cliff("--unreleased", "--context", github_token=token)
            preview = cliff(
                "--from-context", "-", "--strip", "header",
                context=context, github_context=True,
            )
        else:
            preview = cliff("--unreleased", "--strip", "header")
        print(preview)
        return
    if not version:
        return

    tag = f"v{version}"
    # Share one metadata fetch and one release context between both outputs.
    context = cliff("--tag", tag, "--context", github_token=token)
    releases = json.loads(context)
    if not releases or releases[0]["version"] != tag:
        raise SystemExit(f"git-cliff found no unreleased changes for {tag}")
    notes = cliff(
        "--from-context", "-", "--strip", "all",
        context=json.dumps(releases[:1]), github_context=bool(token),
    )
    cliff(
        "--from-context", "-", "--output", str(CHANGELOG),
        context=context, github_context=bool(token),
    )
    args.notes.write_text(
        "Windows and macOS builds are unsigned. Windows SmartScreen and macOS "
        "Gatekeeper may warn when you open them. The macOS build is for Apple "
        "silicon (arm64).\n\n"
        f"{notes}\n",
        encoding="utf-8",
    )
    if version != current:
        write_version(version)
    print(tag)


if __name__ == "__main__":
    sys.exit(main())
