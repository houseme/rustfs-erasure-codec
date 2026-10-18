"""Validate bounded Slot diagnosis; synthetic timings never establish performance."""
import copy
import json
import pathlib
import sys
import tempfile
import subprocess
from unittest import mock
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_shard_slot as diagnostic
import release_sampling as sampling


def trace():
    buffers = [{'len': 65536, 'capacity': 65536, 'offset_64': 16,
                'offset_4096': 16, 'offset_page': 16} for _ in range(6)]
    return {'entry': 'Vec<ShardSlot<Vec<u8>>>', 'source_lifetime': 'observation',
            'clone_lifetime': 'iteration', 'page_size': 4096, 'iterations': 4096,
            'source': buffers, 'transitions': 1, 'overflow': 0,
            'layouts': [{'count': 4096, 'buffers': copy.deepcopy(buffers)}]}


class ShardSlotDiagnosticTests(unittest.TestCase):
    def test_original_arm_remains_byte_identical_to_the_formal_renderer(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        self.assertEqual(diagnostic.render(root, False), sampling.render(root))
        before, after = sampling.render(root), diagnostic.render(root, True)
        changed = {name for name in before if before[name] != after[name]}
        self.assertEqual(changed, {'root.rs', 'small_file.rs'})
        self.assertEqual(set(after)-set(before), {'slot_trace.rs'})
        self.assertIn('let mut shards = shards_to_slots(&original);', after['small_file.rs'])
        self.assertEqual(after['small_file.rs'].count('trace.before(&shards);'), 1)
        self.assertEqual(after['small_file.rs'].count('trace.after(&shards, &original);'), 1)

    def test_changed_original_workload_rejects_instead_of_silently_adapting(self):
        from unittest import mock
        root = pathlib.Path(__file__).resolve().parents[2]
        files = sampling.render(root)
        files['small_file.rs'] = files['small_file.rs'].replace('mark_missing_slots(&mut shards, &[0, case.data_shards]);', 'changed();')
        with mock.patch.object(diagnostic.sampling, 'render', return_value=files):
            with self.assertRaisesRegex(ValueError, 'workload changed'):
                diagnostic.render(root, True)

    def test_all_iterations_and_observations_required_with_page_bound_layouts(self):
        records = [trace() for _ in range(12)]
        result = diagnostic.layouts(records, 4096, 4096)
        self.assertTrue(result['histogram_complete'])
        for mutate in (lambda r: r.pop(), lambda r: r[0].update(iterations=True),
                       lambda r: r[0].update(page_size=16384),
                       lambda r: r[0].update(transitions=4096),
                       lambda r: r[0]['layouts'][0].update(count=4095),
                       lambda r: r[0]['layouts'][0]['buffers'].pop(),
                       lambda r: r[0]['source'][0].update(offset_page=4096),
                       lambda r: r[0].update(clone_lifetime='worker')):
            changed = copy.deepcopy(records); mutate(changed)
            with self.assertRaises(ValueError): diagnostic.layouts(changed, 4096, 4096)

    def test_histogram_overflow_is_preserved_and_never_reported_complete(self):
        records = [trace() for _ in range(12)]
        records[0]['layouts'][0]['count'] -= 1
        records[0]['overflow'] = 1
        self.assertFalse(diagnostic.layouts(records, 4096, 4096)['histogram_complete'])

    def test_failed_worker_preserves_completed_records_and_removes_owned_overlay(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); (root/'tests').mkdir()
            tool = pathlib.Path(__file__).resolve().parents[2]
            context = {'runtime': {'page_size': 4096}, 'isolation': {'cpu_affinity': [2, 3]}}
            row = {'completed': True}
            def run(command, **kwargs):
                if '--no-run' in command: return
                self.assertEqual(command[:3], ['taskset', '--cpu-list', '2'])
                kwargs['stdout'].write('RSE_SAMPLE '+json.dumps(row)+'\n')
                kwargs['stdout'].write('RSE_TIMING '+json.dumps(row)+'\n')
                kwargs['stdout'].flush()
                raise subprocess.CalledProcessError(1, command)
            with mock.patch.object(diagnostic, 'render', return_value={'root.rs': 'root', 'stub.rs': 'stub'}), mock.patch.object(diagnostic.evidence, 'source_info', return_value={'commit': diagnostic.evidence.BASELINE_COMMIT}), mock.patch.object(diagnostic.evidence, 'environment_context', return_value=context), mock.patch.object(diagnostic.subprocess, 'run', side_effect=run):
                with self.assertRaises(subprocess.CalledProcessError):
                    diagnostic.capture(tool, root, root/'output', root/'group', 4096)
            for kind in ('samples', 'timing'):
                path = root/'output'/('plain-0-calibrate-'+kind+'.partial.json')
                self.assertEqual(json.loads(path.read_text()), [row])
            self.assertFalse((root/'tests/rse_release_sampling.rs').exists())
            self.assertFalse((root/'tests/rse_release_sampling').exists())
            self.assertFalse((root/'output/diagnosis.json').exists())

    def test_unexpected_generated_inputs_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); (root/'tests').mkdir()
            with mock.patch.object(diagnostic, 'render', return_value={'root.rs': 'root', 'stub.rs': 'stub'}):
                with self.assertRaisesRegex(ValueError, 'preserve'):
                    with diagnostic.overlay(root, False):
                        (root/'tests/rse_release_sampling/foreign.rs').write_text('unrelated')
            self.assertEqual((root/'tests/rse_release_sampling/foreign.rs').read_text(), 'unrelated')
            self.assertTrue((root/'tests/rse_release_sampling.rs').exists())

    def test_partial_journal_keeps_completed_numeric_rows(self):
        record = trace()
        text = diagnostic.PREFIX+json.dumps(record)+'\n'+diagnostic.PREFIX+'{"entry":'
        self.assertEqual(sampling.parse_output(text, diagnostic.PREFIX), [record])
        with self.assertRaises(json.JSONDecodeError): sampling.parse_output(text+'\n', diagnostic.PREFIX)


if __name__ == '__main__': unittest.main()
