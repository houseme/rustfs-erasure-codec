import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_release_manifest.py"


class ReleaseManifestTests(unittest.TestCase):
    def check(self, tag=None, package="9.1.0", workspace="9.1.0", lock="9.1.0"):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text(
                f'[package]\nname = "rustfs-erasure-codec"\nversion = "{package}"\n'
                f'[workspace.dependencies]\nrustfs-erasure-codec = {{ version = "{workspace}", path = "." }}\n'
            )
            (root / "Cargo.lock").write_text(
                f'[[package]]\nname = "rustfs-erasure-codec"\nversion = "{lock}"\n'
            )
            arguments = [sys.executable, str(SCRIPT), "--root", str(root)]
            if tag is not None:
                arguments += ["--tag", tag]
            return subprocess.run(arguments, capture_output=True, text=True, check=False)

    def test_canonical_bare_and_v_tags(self):
        for tag in (None, "9.1.0", "v9.1.0"):
            result = self.check(tag)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_prerelease_tags_match_exactly(self):
        result = self.check("v9.1.0-rc.1", package="9.1.0-rc.1", workspace="9.1.0-rc.1", lock="9.1.0-rc.1")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_other_tags_and_partial_versions_fail(self):
        for tag in ("main", "v9.0.0", "9.1", "v9.1.0-extra", "prefix9.1.0", ""):
            with self.subTest(tag=tag):
                self.assertNotEqual(self.check(tag).returncode, 0)

    def test_workspace_and_lock_must_match_package(self):
        for changes in ({"workspace": "9.0.0"}, {"lock": "9.0.0"}):
            result = self.check(**changes)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("versions differ", result.stderr)

    def test_missing_manifests_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, str(SCRIPT), "--root", directory], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
