"""Placement contracts use synthetic samples, not performance evidence."""
import copy
import json
import pathlib
import signal
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import diagnose_cpu_placement as diagnostic
import release_sampling as sampling


def records(sequence=(2, 2), iterations=384):
    size = 524288
    pos = {'len': size, 'capacity': size, 'offset_64': 16, 'offset_4096': 16, 'offset_page': 16}
    fixture = {'entry': 'Vec<Vec<u8>>', 'lifetime': 'worker', 'page_size': 4096,
               'source': [dict(pos) for _ in range(14)], 'scratch': [],
               **sampling.fixture_contract('small_file', diagnostic.CASE)}
    rows = []
    for epoch, cpu in enumerate(sequence):
        for index in range(12):
            rows.append({'epoch': epoch, 'cpu': cpu, 'index': index, 'iterations': iterations,
                         'kind': 'warmup' if index < 3 else 'sample', 'case': diagnostic.CASE,
                         'call_entry': 'Vec<Vec<u8>>', 'backend': 'fixture', 'fixture': copy.deepcopy(fixture),
                         'ns_per_iter': 400000., 'timed_ns': 400000.*iterations, 'wall_ns': 160000000,
                         'before': {'cpu': cpu, 'run_ns': 1, 'wait_ns': 1, 'switches': 1, 'process_minor_faults':0, 'process_major_faults':0},
                         'after': {'cpu': cpu, 'run_ns': 1000, 'wait_ns': 2, 'switches': 3, 'process_minor_faults':0, 'process_major_faults':0},
                         'faults_before': {'cpu': cpu, 'minor': 1, 'major': 0},
                         'faults_after': {'cpu': cpu, 'minor': 3, 'major': 0}})
    return rows


class CpuPlacementTests(unittest.TestCase):
    def test_complete_counterbalanced_plan_never_selects_only_stable_cpus(self):
        plan = diagnostic.worker_plan([2, 3, 4])
        self.assertEqual(plan[:2], [('same-buffer-forward-reverse', [2, 3, 4, 4, 3, 2]),
                                  ('same-buffer-reverse-forward', [4, 3, 2, 2, 3, 4])])
        self.assertEqual([s for _, s in plan[2:]], [[2], [3], [4], [4], [3], [2]])

    def test_formal_overlay_is_untouched_and_counter_is_check_only(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        before, after = sampling.render(root), diagnostic.render(root)
        self.assertEqual({name for name in before if before[name] != after[name]}, {'engine.rs', 'small_file.rs', 'root.rs'})
        self.assertEqual(after['engine.rs'].replace('pub(super) fn read_bounded', 'fn read_bounded'), before['engine.rs'])
        self.assertTrue(after['small_file.rs'].startswith(before['small_file.rs']))
        self.assertNotIn('#[global_allocator]', '\n'.join(after.values()))
        self.assertIn('#[global_allocator]', diagnostic.render(root, allocation_check=True)['root.rs'])

    def test_complete_samples_retain_faults_and_repeats_without_approval(self):
        report = diagnostic.analyze(records(), [2, 2], 384, 4096)
        self.assertEqual(len(report['groups']), 2)
        self.assertEqual(report['groups'][0]['measured_minor_faults'], 18)
        self.assertEqual(report['same_worker_repeats'], [{'cpu': 2, 'absolute_drift': 0.}])
        self.assertFalse(report['release_approval'])
        self.assertIsNone(report['candidate_attribution'])

    def test_quality_and_repeat_failures_remain_visible(self):
        rows = records()
        for row in rows[12:]:
            row['ns_per_iter'] *= 1.2; row['timed_ns'] *= 1.2; row['wall_ns'] = 1
        rows[-2]['ns_per_iter'] *= 1.3; rows[-2]['timed_ns'] *= 1.3
        rows[-3]['ns_per_iter'] *= 1.3; rows[-3]['timed_ns'] *= 1.3
        report = diagnostic.analyze(rows, [2, 2], 384, 4096)
        self.assertEqual(report['groups'][1]['short_samples'], 9)
        self.assertGreater(report['groups'][1]['central_spread'], .1)
        self.assertGreater(report['same_worker_repeats'][0]['absolute_drift'], .05)
        self.assertFalse(report['release_approval'])

    def test_missing_samples_escaped_cpus_bad_timing_and_changed_storage_reject(self):
        for change in (lambda r: r.pop(), lambda r: r[0].update(index=True),
                       lambda r: r[0]['after'].update(cpu=3),
                       lambda r: r[0]['faults_after'].update(minor=0),
                       lambda r: r[0].update(timed_ns=1),
                       lambda r: r[-1]['fixture']['source'][0].update(offset_64=32, offset_4096=32, offset_page=32)):
            rows = records(); change(rows)
            with self.assertRaises(ValueError): diagnostic.analyze(rows, [2, 2], 384, 4096)

    def fake_worker(self, directory, foreign=False, bad_budget=False, cancel=False):
        ready = {'epoch': 0, 'cpu': 2, 'tid': 501, 'budget_before': 1 if bad_budget else 2,
                 'budget_after': 2, 'hypothetical_parallel_policy': {'use_parallel': True, 'jobs': 2, 'chunk_len': 32768}}
        end = {'epoch': 0, 'cpu': 2, 'cached_policy_unchanged': True, 'available_parallelism_while_pinned': 1}
        stream = 'RSE_CPU_READY '+json.dumps(ready)+'\n'
        stream += ''.join('RSE_CPU_SAMPLE '+json.dumps(row)+'\n' for row in records([2]))
        stream += 'RSE_CPU_END '+json.dumps(end)+'\n'
        process = SimpleNamespace(pid=500, stdin=mock.Mock(), stdout=SimpleNamespace(fileno=lambda: 10, close=lambda: None), wait=lambda **_: 0)
        state = {'cpus': {2, 3}}
        chunks = [stream.encode(), b'']
        if cancel:
            parts = stream.splitlines(keepends=True)
            chunks = [parts[0].encode(), ''.join(parts[1:4]).encode()]
        reads = 0
        def read_chunk(*_):
            nonlocal reads
            reads += 1
            if cancel and reads == 2: signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)
            return chunks.pop(0)
        selector = mock.MagicMock(); selector.__enter__.return_value = selector; selector.select.return_value = [(1, 1)]
        path = pathlib.Path(directory)/'worker.log'
        with mock.patch.object(diagnostic.subprocess, 'Popen', return_value=process), mock.patch.object(diagnostic.selectors, 'DefaultSelector', return_value=selector), mock.patch.object(diagnostic.os, 'read', side_effect=read_chunk), mock.patch.object(diagnostic.os, 'getpgid', return_value=999 if foreign else 500), mock.patch.object(diagnostic.os, 'sched_getaffinity', create=True, side_effect=lambda _: state['cpus']), mock.patch.object(diagnostic.os, 'sched_setaffinity', create=True, side_effect=lambda _, cpus: state.update(cpus=cpus)) as pin, mock.patch.object(diagnostic.os, 'killpg') as kill:
            if cancel:
                with self.assertRaises(InterruptedError): diagnostic.run_worker(path.parent, {}, [2], 384, path, [2, 3])
                kill.assert_called_once_with(500, signal.SIGTERM)
                self.assertEqual(len(json.loads(path.with_suffix('.samples.partial.json').read_text())), 3)
            elif foreign or bad_budget:
                with self.assertRaises(ValueError): diagnostic.run_worker(path.parent, {}, [2], 384, path, [2, 3])
                pin.assert_not_called(); kill.assert_called_once()
                self.assertEqual(json.loads(path.with_suffix('.samples.partial.json').read_text()), [])
            else:
                rows, gates = diagnostic.run_worker(path.parent, {}, [2], 384, path, [2, 3])
                self.assertEqual(len(rows), 12)
                self.assertEqual(gates[0]['actual_affinity'], [2])
                self.assertNotIn('tid', gates[0])
                pin.assert_called_once_with(501, {2}); kill.assert_not_called()
                self.assertEqual(process.stdin.write.call_args_list, [mock.call(b'G'), mock.call(b'E')])

    def test_parent_pins_only_after_budget_and_owned_thread_are_verified(self):
        with tempfile.TemporaryDirectory() as directory: self.fake_worker(directory)

    def test_termination_cleans_owned_session_preserves_samples_and_restores_handlers(self):
        before = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
        with tempfile.TemporaryDirectory() as directory: self.fake_worker(directory, cancel=True)
        self.assertEqual(before, {sig: signal.getsignal(sig) for sig in before})

    def test_foreign_thread_or_single_cpu_construction_is_rejected_before_pin(self):
        for flags in ({'foreign': True}, {'bad_budget': True}):
            with tempfile.TemporaryDirectory() as directory: self.fake_worker(directory, **flags)


if __name__ == '__main__': unittest.main()
