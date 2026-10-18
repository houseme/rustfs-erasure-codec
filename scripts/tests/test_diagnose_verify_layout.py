"""Fixtures validate completeness and failure behavior, never performance."""
import json
import subprocess
import tempfile
from unittest import mock
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_verify_layout as diagnostic
import release_sampling as sampling


def records():
    result = []
    grid = [(a,b) for a in diagnostic.OFFSETS for b in diagnostic.OFFSETS]
    for length in diagnostic.LENGTHS:
        for order in (0,1):
            for a,b in (grid if order == 0 else reversed(grid)):
                for index in range(sampling.PROTOCOL['warmups']+sampling.PROTOCOL['samples']):
                    result.append({'len':length, 'pass':order, 'source_offset':a, 'scratch_offset':b,
                                   'kind':'warmup' if index < sampling.PROTOCOL['warmups'] else 'sample',
                                   'index':index, 'iterations':100, 'wall_ns':10000, 'ns_per_iter':100.0,
                                   'entry':'slice_views', 'fixture_lifetime':'same_backing_per_length',
                                   'input_page_offsets':[a]*6, 'scratch_page_offsets':[b]*2})
    return result


class LayoutDiagnosticTests(unittest.TestCase):
    def test_interrupted_final_console_fragment_does_not_erase_completed_records(self):
        row=records()[0]
        output='build log\n'+diagnostic.PREFIX+json.dumps(row)+'\n'+diagnostic.PREFIX+'{"kind":'
        self.assertEqual(diagnostic.parse_output(output),[row])
        with self.assertRaises(json.JSONDecodeError): diagnostic.parse_output(output+'\n')

    def test_failed_capture_preserves_completed_numeric_rows_and_removes_its_input(self):
        with tempfile.TemporaryDirectory() as directory:
            root=pathlib.Path(directory);(root/'tests').mkdir();output=root/'capture'
            source={'commit':'a'*40}
            context={'isolation':{'cpu_affinity':[2,3]}}
            def run(command, **kwargs):
                if '--no-run' in command: return
                self.assertEqual(command[:3],['taskset','--cpu-list','2'])
                kwargs['stdout'].write(diagnostic.PREFIX+json.dumps(records()[0])+'\n')
                kwargs['stdout'].flush()
                raise subprocess.CalledProcessError(1,command)
            with mock.patch.object(diagnostic.evidence,'source_info',return_value=source), mock.patch.object(diagnostic.evidence,'environment_context',return_value=context), mock.patch.object(diagnostic.subprocess,'run',side_effect=run):
                with self.assertRaises(subprocess.CalledProcessError):
                    diagnostic.capture(root,output,root/'group',100)
            self.assertEqual(json.loads((output/'samples.partial.json').read_text()),records()[:1])
            self.assertFalse((root/'tests/rse_verify_layout_diagnostic.rs').exists())
            self.assertFalse((output/'diagnosis.json').exists())

    def test_complete_grid_retains_all_cases_without_release_attribution(self):
        report = diagnostic.analyze(records(),100)
        self.assertEqual(report['case_count'],64)
        self.assertEqual(report['formal_samples'],576)
        self.assertEqual(len(report['repeat_checks']),32)
        self.assertIsNone(report['candidate_attribution'])
        self.assertFalse(report['release_approval'])

    def test_missing_reordered_or_replaced_observations_reject(self):
        changes = [lambda rows:rows.pop(), lambda rows:rows.reverse(),
                   lambda rows:rows[0].update(index=True), lambda rows:rows[0].update(kind='sample'),
                   lambda rows:rows[0].update(entry='Vec'), lambda rows:rows[0].update(iterations=1),
                   lambda rows:rows[0].update(wall_ns=0), lambda rows:rows[0].update(ns_per_iter=float('nan')),
                   lambda rows:rows[0]['input_page_offsets'].pop(),
                   lambda rows:rows[0]['scratch_page_offsets'].__setitem__(0,4096),
                   lambda rows:rows[0]['scratch_page_offsets'].__setitem__(0,16)]
        for change in changes:
            rows = records();change(rows)
            with self.assertRaises(ValueError): diagnostic.analyze(rows,100)

    def test_a_reallocated_reverse_fixture_cannot_look_comparable(self):
        rows = records()
        for row in rows:
            if row['pass'] == 1:
                row['input_page_offsets'] = [p+64 for p in row['input_page_offsets']]
        with self.assertRaisesRegex(ValueError,'reuse'): diagnostic.analyze(rows,100)

    def test_failures_are_reported_without_removing_wide_or_drifting_cells(self):
        rows = records()
        for row in rows:
            if row['pass'] == 1 and row['source_offset'] == 16 and row['scratch_offset'] == 32:
                row['wall_ns'] *= 2; row['ns_per_iter'] *= 2
            if row['index'] in (3,4):
                row['wall_ns'] = int(row['wall_ns']*.8);row['ns_per_iter'] *= .8
            if row['index'] in (10,11):
                row['wall_ns'] = int(row['wall_ns']*1.2);row['ns_per_iter'] *= 1.2
        report = diagnostic.analyze(rows,100)
        self.assertEqual(len(report['wide_cases']),64)
        self.assertEqual(len(report['repeat_failures']),2)
        self.assertFalse(report['release_approval'])


if __name__ == '__main__': unittest.main()
