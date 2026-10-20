"""The allocation counter belongs only to the separate check executable."""
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import check_sampler_telemetry as checker
import release_sampling as sampling


class TelemetryProbeTests(unittest.TestCase):
    def root(self, directory):
        root = pathlib.Path(directory)
        (root/'tests').mkdir()
        (root/'scripts/diagnostics').mkdir(parents=True)
        (root/'scripts/diagnostics/telemetry_allocations.rs').write_text('test-only-counter')
        (root/'scripts/diagnostics/stream_view_allocations.rs').write_text('// stream-specific attachment fixture')
        (root/'scripts/diagnostics/verify_scratch_allocations.rs').write_text('// verify-specific attachment fixture')
        (root/'scripts/diagnostics/stream_core_allocations.rs').write_text('// core-specific attachment fixture')
        (root/'scripts/diagnostics/dynamic_buffer_allocations.rs').write_text('// dynamic-specific attachment fixture')
        (root/'src/tests').mkdir(parents=True)
        (root/'src/tests/mod.rs').write_text('fn shard_index_pattern() {\n}\n')
        return root

    def test_counter_is_never_included_in_the_formal_overlay(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        self.assertTrue(all('#[global_allocator]' not in value for value in sampling.render(root).values()))
        self.assertEqual(sampling.PROTOCOL['observer_buffer_policy'], 'bounded-stack-procfs-no-rust-heap')

    def test_owned_probe_is_removed_even_when_the_check_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.root(directory); target = root/'tests/rse_sampler_telemetry_allocations.rs'
            with self.assertRaises(RuntimeError):
                with checker.probe(root):
                    self.assertIn('mod sampling;', target.read_text())
                    raise RuntimeError('compiler failure')
            self.assertFalse(target.exists())

    def test_existing_or_changed_inputs_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.root(directory); target = root/'tests/rse_sampler_telemetry_allocations.rs'
            target.write_text('unrelated')
            with self.assertRaisesRegex(ValueError, 'overwrite'):
                with checker.probe(root): pass
            self.assertEqual(target.read_text(), 'unrelated'); target.unlink()
            with self.assertRaisesRegex(ValueError, 'preserve'):
                with checker.probe(root): target.write_text('changed')
            self.assertEqual(target.read_text(), 'changed')
