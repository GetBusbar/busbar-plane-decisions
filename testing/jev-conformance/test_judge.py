"""Focused regressions for failure isolation and target resource ownership."""
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("judge", Path(__file__).with_name("judge.py"))
judge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(judge)


class JudgeTests(unittest.TestCase):
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
