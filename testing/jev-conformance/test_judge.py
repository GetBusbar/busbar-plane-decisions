"""Focused regressions for subject composition, failure isolation and resource ownership."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("judge", Path(__file__).with_name("judge.py"))
judge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(judge)


class ComposeTests(unittest.TestCase):
    def test_staged_patch_is_shared_with_nested_locked_metadata(self):
        # No compilation or network: a tiny local git dependency reproduces the subject graph.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = dict(os.environ, CARGO_HOME=str(root / "cargo-home"),
                       CARGO_NET_OFFLINE="true", CARGO_NET_GIT_FETCH_WITH_CLI="true",
                       GIT_ALLOW_PROTOCOL="file")

            def run(*args, cwd=root):
                result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return result.stdout

            package = '[package]\nname="busbar-plane-decisions"\nversion="1.0.0"\nedition="2021"\n'
            for name in ("upstream", "plane"):
                crate = root / name
                (crate / "src").mkdir(parents=True)
                (crate / "Cargo.toml").write_text(package)
                (crate / "src/lib.rs").write_text('pub use plane_door::door;\n')
            upstream = root / "upstream"
            run("git", "init", "-q", str(upstream))
            run("git", "-C", str(upstream), "add", ".")
            run("git", "-C", str(upstream), "-c", "user.name=Fixture", "-c",
                "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                "-c", "core.hooksPath=/dev/null", "commit", "-qm", "Fixture")
            checkout = root / "checkout"
            distribution = checkout / "dist/default"
            (distribution / "src").mkdir(parents=True)
            (distribution / "src/lib.rs").write_text("")
            manifest = distribution / "Cargo.toml"
            manifest.write_text(
                '[package]\nname="subject"\nversion="1.0.0"\nedition="2021"\n'
                '[package.metadata.busbar.linked-entry]\nplane-decisions="busbar_plane_decisions"\n'
                '[workspace]\n[workspace.dependencies]\n'
                f'busbar-plane-decisions={{git={json.dumps(upstream.as_uri())}}}\n'
                '[dependencies]\nbusbar-plane-decisions={workspace=true}\n')
            # Seed only this synthetic file:// repository; Git forbids every network protocol.
            # Cargo's offline mode also forbids uncached local Git, so override it for seeding.
            run("cargo", "metadata", "--config", "net.offline=false",
                "--format-version=1", cwd=distribution)
            original_manifest = manifest.read_bytes()
            original_lock = (distribution / "Cargo.lock").read_bytes()
            staged = Path(run("bash", str(Path(__file__).with_name("compose.sh")),
                              str(checkout), str(root / "plane"), str(root / "staged")).strip())
            # The outer build resolves its patch, then the build script uses --locked with no
            # command-line overrides. Both must name the candidate, and the lock must stay fixed.
            outer = json.loads(run("cargo", "metadata", "--offline", "--format-version=1",
                                   "--manifest-path", str(staged)))
            lock = staged.with_name("Cargo.lock").read_bytes()
            nested = json.loads(run("cargo", "metadata", "--offline", "--locked",
                                    "--all-features", "--format-version=1", cwd=staged.parent))
            for metadata in (outer, nested):
                plane = next(p for p in metadata["packages"] if p["name"] == "busbar-plane-decisions")
                self.assertEqual(Path(plane["manifest_path"]), root / "plane/Cargo.toml")
                self.assertIsNone(plane["source"])
            self.assertEqual(staged.with_name("Cargo.lock").read_bytes(), lock)
            self.assertEqual(manifest.read_bytes(), original_manifest)
            self.assertEqual((distribution / "Cargo.lock").read_bytes(), original_lock)


class JudgeTests(unittest.TestCase):
    def test_admission_quota_refusal_preserves_all_gating_checks(self):
        for code, hops, spend, requests, passes in (
                ("rate_limit", 0, (1, 100), 1, True),
                ("unsupported_operation", 0, (1, 100), 1, False),
                ("rate_limit", 1, (1, 100), 1, False),
                ("rate_limit", 0, (2, 200), 1, False),
                ("rate_limit", 0, (1, 100), 2, False)):
            with self.subTest(code=code, hops=hops, spend=spend, requests=requests):
                target = Mock()
                target.mint.return_value = ("Bearer fixture", "key")
                target.call.side_effect = [judge.Call(200, judge.SUCCESS, 1),
                    judge.Call(429, json.dumps({"error": {"code": code, "message": "quota"}}).encode(), hops)]
                usage = iter((1, requests))
                target.admin_get.side_effect = lambda path: (
                    json.dumps({"requests": next(usage)}).encode() if path.endswith("/usage") else b"{}")
                with patch.object(judge, "ledger_spend", side_effect=[(0, 0), (1, 100), spend]), \
                        patch.object(judge, "settled", side_effect=lambda read, done: read()), \
                        patch.object(judge, "run_checks", side_effect=lambda pairs: dict(pairs)["h2-admit-refusal"]()):
                    if passes:
                        judge.busbar_checks(target)
                    else:
                        with self.assertRaises(judge.Red):
                            judge.busbar_checks(target)

    def test_failed_check_does_not_skip_later_checks(self):
        later = Mock()
        failed = Mock(side_effect=ConnectionResetError("reset"))
        results = judge.run_checks([("failed", failed), ("later", later)])
        self.assertIn("ConnectionResetError", results[0][1])
        self.assertEqual(results[1], ("later", None))
        later.assert_called_once_with()

    def test_nanojev_floor_waits_for_previous_ledger_row(self):
        target = Mock(admin=True)
        # The preceding served check's row lands after the first baseline read.
        totals = [(0, 0), (1, 42), (1, 42), (2, 42 + judge.FLOOR_UNITS)]
        with patch.object(judge, "spend_of", side_effect=totals), \
                patch.object(judge.time, "sleep"), \
                patch.object(judge, "settled", side_effect=lambda read, done: read()), \
                patch.object(judge, "served_once"), \
                patch.object(judge, "run_checks", side_effect=lambda pairs: pairs[2][1]()):
            judge.wire_nanojev(target)

    def test_negative_control_does_not_count_an_unread_target_as_caught(self):
        # A break whose relay merely fails to answer has not been judged by its check: red only
        # because the target could not be read is a missed break, not a caught one.
        def unread(_target):
            return [(check, judge.UNREAD_FINDING + ": ConnectionResetError: reset")
                    for check in judge.reference_relay.BREAKS.values()]
        with tempfile.TemporaryDirectory() as out, \
                patch.object(judge, "run_wire", side_effect=unread), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.main(["judge.py", "negative", out]), 1)

    def test_negative_control_counts_a_judged_red_as_caught(self):
        def red(_target):
            return [(check, "a judged finding") for check in judge.reference_relay.BREAKS.values()]
        with tempfile.TemporaryDirectory() as out, \
                patch.object(judge, "run_wire", side_effect=red), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.main(["judge.py", "negative", out]), 0)

    def test_incomplete_command_is_usage_error(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(judge.main(["judge.py", "subject"]), 2)

    def test_failed_bind_closes_socket(self):
        sock = Mock()
        sock.bind.side_effect = OSError("bind failed")
        with patch.object(judge.socket, "socket", return_value=sock):
            with self.assertRaises(OSError):
                judge.free_port()
        sock.close.assert_called_once_with()

    def test_relay_spawn_failure_closes_far_end(self):
        far = Mock(port=1234)
        with tempfile.TemporaryDirectory() as out, \
                patch.object(judge, "FarEnd", return_value=far), \
                patch.object(judge, "free_port", return_value=1235), \
                patch.object(judge.subprocess, "Popen", side_effect=OSError("spawn failed")):
            with self.assertRaises(OSError):
                judge.relay_target("one", out)
        far.close.assert_called_once_with()

    def test_key_generation_failure_closes_far_end(self):
        far = Mock(port=1234)
        with tempfile.TemporaryDirectory() as out, \
                patch.object(judge, "FarEnd", return_value=far), \
                patch.object(judge, "free_ports", return_value=[1235, 1236]), \
                patch.object(judge.subprocess, "run", side_effect=OSError("binary missing")):
            with self.assertRaises(OSError):
                judge.busbar_target("one", out, "missing")
        far.close.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
