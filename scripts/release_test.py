import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import release


GITHUB_ENV = {"GITHUB_TOKEN": "test-token", "GITHUB_REPO": "example/project"}


class CliffTest(unittest.TestCase):
    def test_version_bump_stays_offline_with_github_configured(self):
        with (
            patch.dict(os.environ, GITHUB_ENV, clear=True),
            patch("release.output", return_value="v1.0.0"),
            patch("release.subprocess.run") as run,
        ):
            run.return_value.stdout = "v1.0.1\n"
            self.assertEqual(release.next_version("1.0.0", None), "1.0.1")
            self.assertIn("--offline", run.call_args.args[0])
            self.assertNotIn("GITHUB_TOKEN", run.call_args.kwargs["env"])

    def test_authenticated_failure_stops_the_release(self):
        with (
            patch("release.subprocess.run", side_effect=subprocess.CalledProcessError(1, "git-cliff")),
            self.assertRaisesRegex(SystemExit, "git-cliff failed with GitHub metadata"),
        ):
            release.cliff("--context", github_token="test-token")


class CommandTest(unittest.TestCase):
    def setUp(self):
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.enterContext(contextlib.chdir(directory))
        self.enterContext(patch.dict(os.environ, {}, clear=True))
        self.enterContext(patch("sys.stdout", new_callable=io.StringIO))
        self.enterContext(patch("release.check_versions", return_value="1.0.0"))
        self.enterContext(patch("release.next_version", return_value="1.1.0"))

    def command(self, *args):
        with patch("sys.argv", ["release.py", *args]):
            release.main()

    def test_github_fetch_requires_both_environment_variables(self):
        for env in ({}, {"GITHUB_TOKEN": "test-token"}, {"GITHUB_REPO": "example/project"}, GITHUB_ENV):
            with (
                self.subTest(variables=tuple(env)),
                patch.dict(os.environ, env, clear=True),
                patch("release.subprocess.run") as run,
            ):
                run.return_value.stdout = "[]"
                self.command("preview")
                first, *renders = run.call_args_list
                if env == GITHUB_ENV:
                    self.assertIn("--context", first.args[0])
                    self.assertEqual(first.kwargs["env"]["GITHUB_TOKEN"], "test-token")
                    render, = renders
                    self.assertIn("--from-context", render.args[0])
                    self.assertNotIn("GITHUB_TOKEN", render.kwargs["env"])
                else:
                    self.assertIn("--offline", first.args[0])
                    self.assertNotIn("GITHUB_TOKEN", first.kwargs["env"])
                    self.assertEqual(renders, [])

    def test_prepare_requires_token_in_github_actions(self):
        with (
            patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "GITHUB_REPO": "example/project"}),
            patch("release.cliff") as cliff,
            self.assertRaisesRegex(SystemExit, "requires GITHUB_TOKEN and GITHUB_REPO"),
        ):
            self.command("prepare", "--notes", "notes.md")
        cliff.assert_not_called()

    def test_context_fetch_failure_leaves_release_files_untouched(self):
        paths = (Path("notes.md"), release.CHANGELOG, release.MANIFEST)
        for path in paths:
            path.write_text(path.name, encoding="utf-8")
        with (
            patch.dict(os.environ, GITHUB_ENV),
            patch("release.cliff", side_effect=SystemExit("git-cliff failed with GitHub metadata")),
            self.assertRaisesRegex(SystemExit, "git-cliff failed with GitHub metadata"),
        ):
            self.command("prepare", "--notes", "notes.md")
        for path in paths:
            self.assertEqual(path.read_text(encoding="utf-8"), path.name)

    def test_prepare_renders_both_outputs_from_one_context(self):
        releases = [
            {"version": "v1.1.0", "commits": [{"remote": {"username": "octocat"}}]},
            {"version": "v1.0.0", "commits": []},
        ]
        with (
            patch.dict(os.environ, GITHUB_ENV),
            patch("release.cliff", side_effect=[json.dumps(releases), "new notes", ""]) as cliff,
            patch("release.write_version"),
        ):
            self.command("prepare", "--notes", "notes.md")
        fetch, notes, changelog = cliff.call_args_list
        self.assertEqual(fetch.args, ("--tag", "v1.1.0", "--context"))
        self.assertEqual(fetch.kwargs["github_token"], "test-token")
        self.assertEqual(notes.args, ("--from-context", "-", "--strip", "all"))
        self.assertEqual(changelog.args, ("--from-context", "-", "--output", "CHANGELOG.md"))
        self.assertEqual(json.loads(notes.kwargs["context"]), releases[:1])
        self.assertEqual(json.loads(changelog.kwargs["context"]), releases)
        self.assertTrue(Path("notes.md").read_text(encoding="utf-8").endswith("\n\nnew notes\n"))

    def test_offline_changelog_preserves_existing_github_data(self):
        for data in (
            "- Add an endpoint *(abc1234)* by @octocat\n",
            "- Add an endpoint *(abc1234)* in [#12](https://github.com/example/project/pull/12)\n",
            "### New contributors\n",
            "[Full changelog](https://github.com/example/project/compare/v1.0.0...v1.1.0)\n",
        ):
            with self.subTest(data=data), patch("release.cliff") as cliff:
                release.CHANGELOG.write_text(data, encoding="utf-8")
                with self.assertRaisesRegex(SystemExit, "Set GITHUB_REPO .* and GITHUB_TOKEN"):
                    self.command("changelog")
                cliff.assert_not_called()
                self.assertEqual(release.CHANGELOG.read_text(encoding="utf-8"), data)


class TemplateTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which("git-cliff"):
            message = "git-cliff is required for release template tests. Use nix develop."
            required_in = ("ACTIVITY_LOG_REQUIRE_GIT_CLIFF", "CI", "GITHUB_ACTIONS", "IN_NIX_SHELL")
            if any(os.environ.get(key) for key in required_in):
                raise RuntimeError(message)
            raise unittest.SkipTest(message)

    def test_rendering_with_and_without_github_data(self):
        services = ("github", "gitlab", "gitea", "bitbucket", "azure_devops")
        contributor = {"username": "octocat", "pr_number": 12, "pr_labels": [], "is_first_time": True}
        previous = {
            "version": "v1.0.0",
            "timestamp": 1735689600,
            "commits": [],
            "submodule_commits": {},
            **{service: {"contributors": []} for service in services},
        }
        commit = {
            "id": "abc1234",
            "message": "add an endpoint",
            "raw_message": "feat: add an endpoint",
            "group": "<!-- 1 -->✨ Features",
            "conventional": True,
            "merge_commit": False,
            "links": [],
            "author": {"timestamp": 0},
            "committer": {"timestamp": 0},
            **{service: {"pr_labels": [], "is_first_time": False} for service in services},
        }
        current = {**previous, "version": "v1.1.0", "previous": previous, "commits": [commit]}
        config = Path(release.__file__).resolve().parents[1] / "cliff.toml"
        args = ("--config", str(config), "--from-context", "-", "--strip", "all")
        with patch.dict(os.environ, {"GITHUB_REPO": "example/project"}):
            offline = release.cliff(*args, context=json.dumps([current, previous]))
            commit["remote"] = contributor
            current["github"] = {"contributors": [
                {**contributor, "username": "example-automation[bot]"},
                contributor,
                {**contributor, "username": "example-returner", "is_first_time": False},
            ]}
            online = release.cliff(*args, context=json.dumps([current, previous]), github_context=True)
            current["github"]["contributors"].remove(contributor)
            no_newcomers = release.cliff(
                *args, context=json.dumps([current, previous]), github_context=True,
            )
        for expected in ("by @octocat", "### New contributors", "/compare/v1.0.0...v1.1.0"):
            self.assertIn(expected, online)
        self.assertIn("[#12](https://github.com/example/project/pull/12)\n\n[Full changelog]", online)
        self.assertIn("v1.0.0...v1.1.0)\n\n## [1.0.0]", online)
        for unwanted in ("example-automation[bot]", "example-returner"):
            self.assertNotIn(unwanted, online)
        self.assertNotIn("New contributors", no_newcomers)
        for unwanted in ("by @", "New contributors", "/compare/", "/pull/"):
            self.assertNotIn(unwanted, offline)
        self.assertIn("- Add an endpoint *(abc1234)*", offline)


if __name__ == "__main__":
    unittest.main()
