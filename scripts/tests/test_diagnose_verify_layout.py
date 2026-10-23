"""Fixtures validate completeness and failure behavior, never performance."""
import json
import os
import signal
import time
import subprocess
import tempfile
from unittest import mock
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import diagnose_verify_layout as diagnostic


from layout_fixture import (records, controlled_context, paired_records, write_phase,
                            write_campaign, BASELINE, CANDIDATE)

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
            context=controlled_context()
            def run(command, **kwargs):
                if '--no-run' in command: return
                self.assertEqual(command[:3],['taskset','--cpu-list','2'])
                kwargs['stdout'].write(diagnostic.PREFIX+json.dumps(records()[0])+'\n')
                kwargs['stdout'].flush()
                raise subprocess.CalledProcessError(1,command)
            with mock.patch.object(diagnostic.evidence,'source_info',return_value=source), mock.patch.object(diagnostic.evidence,'environment_context',return_value=context), mock.patch.object(diagnostic,'run_checked',side_effect=run):
                with self.assertRaises(subprocess.CalledProcessError):
                    diagnostic.capture(root,output,root/'group',100)
            self.assertEqual(json.loads((output/'samples.partial.json').read_text()),records()[:1])
            self.assertFalse((root/'tests/rse_release_sampling.rs').exists())
            self.assertFalse((root/'tests/rse_release_sampling').exists())
            self.assertFalse((output/'diagnosis.json').exists())

class PairedLayoutTests(unittest.TestCase):
    def verify(self, output, **kwargs):
        with mock.patch.object(diagnostic.evidence, 'source_info', return_value=CANDIDATE), mock.patch.object(diagnostic.evidence, 'source_at', side_effect=lambda root, rev: BASELINE if rev == BASELINE['commit'] else CANDIDATE):
            return diagnostic.verify_paired(output, output, compiler=False, **kwargs)

    def test_diagnostic_replay_recomputes_all_unqualified_cells_without_approval(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            document = write_campaign(output)
            document['diagnostic_only'] = True
            rows = paired_records()
            for row in rows:
                row['ns_per_iter'] *= 1.2
                row['wall_ns'] = round(row['wall_ns'] * 1.2)
            diagnostic.save(output/'B1', 'samples.json', rows)
            (output/'B1/measurement.log').write_text(''.join(
                diagnostic.PREFIX+json.dumps(row)+'\n' for row in rows))
            for name in ('samples.json', 'measurement.log'):
                document['files']['B1'][name]['sha256'] = diagnostic.evidence.digest(
                    (output/'B1'/name).read_bytes())
            diagnostic.save(output, 'diagnostic-manifest.json', document)
            # Saved summaries cannot override raw, source-bound observations.
            diagnostic.save(output, 'pairing.json', {'layout_gate_passed': True})
            report = self.verify(output, diagnostic=True)
            self.assertFalse(report['layout_gate_passed'])
            self.assertFalse(report['release_approval'])
            self.assertIsNone(report['candidate_attribution'])
            self.assertEqual(len(report['unqualified_layout_comparisons']), 256)
            self.assertEqual(len(report['unqualified_regression_failures']), 128)
            (output/'manifest.json').write_text(json.dumps(document))
            with self.assertRaisesRegex(ValueError, 'modes must match'):
                self.verify(output)

    def test_immutable_replay_uses_raw_rows_instead_of_any_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            write_campaign(output)
            diagnostic.save(output, 'pairing.json', {'layout_gate_passed': False})
            report = self.verify(output)
            self.assertTrue(report['layout_gate_passed'])
            self.assertFalse(report['release_approval'])

    def test_directory_replay_uses_one_small_frame_without_python_member_parsing(self):
        with tempfile.TemporaryDirectory() as directory:
            output=pathlib.Path(directory);write_campaign(output)
            paths=[p for p in output.rglob('*') if p.is_file()]
            before={p.relative_to(output).as_posix():p.read_bytes() for p in paths}
            native=diagnostic.native_tools
            with mock.patch.object(diagnostic.evidence,'json_data',side_effect=AssertionError('Python JSON replay')):
                with mock.patch.object(diagnostic,'parse_output',side_effect=AssertionError('Python journal replay')):
                    with mock.patch.object(native,'_request_payload',wraps=native._request_payload) as frames:
                        report=self.verify(output)
            self.assertTrue(report['layout_gate_passed']);self.assertEqual(frames.call_count,1)
            self.assertLess(len(frames.call_args.args[0].encode()),8192)
            self.assertEqual(before,{p.relative_to(output).as_posix():p.read_bytes() for p in paths})

    def test_explicit_manifest_digest_is_required_to_be_a_matching_string(self):
        with tempfile.TemporaryDirectory() as directory:
            output=pathlib.Path(directory);write_campaign(output)
            sha=diagnostic.evidence.digest((output/'manifest.json').read_bytes())
            self.assertTrue(self.verify(output,manifest_sha256=sha)['layout_gate_passed'])
            for value in (None,False,0,'0'*64):
                with self.assertRaises(ValueError):self.verify(output,manifest_sha256=value)

    def test_rebase_accepts_identical_full_tree_and_lock_but_rejects_changed_tool(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory); write_campaign(output)
            rebased = dict(CANDIDATE, commit='e'*40)
            with mock.patch.object(diagnostic.evidence, 'source_info', return_value=rebased), mock.patch.object(diagnostic.evidence, 'source_at', return_value=BASELINE):
                self.assertTrue(diagnostic.verify_paired(output, output, compiler=False)['layout_gate_passed'])
            changed = dict(CANDIDATE, tree='f'*40)
            with mock.patch.object(diagnostic.evidence, 'source_info', side_effect=[CANDIDATE, changed]), mock.patch.object(diagnostic.evidence, 'source_at', return_value=BASELINE):
                with self.assertRaisesRegex(ValueError, 'tool source'): diagnostic.verify_paired(output, output, compiler=False)

    def test_missing_tampered_replaced_and_unsafe_members_reject(self):
        changes = [lambda o,d:(o/'A1/samples.json').unlink(),
                   lambda o,d:(o/'A1/samples.json').write_text('[]'),
                   lambda o,d:d['files']['A1']['samples.json'].update(path='../samples.json'),
                   lambda o,d:d['sources']['candidate'].update(tree='c'*40),
                   lambda o,d:d['contract'].update(version=0),
                   lambda o,d:d['phase_order'].reverse(),
                   lambda o,d:d['files'].pop('B2'),
                   lambda o,d:d.update(release_approval=True)]
        for change in changes:
            with tempfile.TemporaryDirectory() as directory:
                output = pathlib.Path(directory); document = write_campaign(output)
                change(output, document); diagnostic.save(output, 'manifest.json', document)
                with self.assertRaises(ValueError): self.verify(output)

    def test_rehashed_but_incompatible_provenance_and_journal_reject(self):
        changes = [lambda m:m.update(source=CANDIDATE),
                   lambda m:m['context'].update(compiler='different'),
                   lambda m:m['context']['runtime'].update(page_size=16384),
                   lambda m:m['context']['isolation'].update(physical_host_exclusivity_proven=True),
                   lambda m:m['context']['settings'].update(RUSTFLAGS='-Ctarget-cpu=native'),
                   lambda m:m.update(measuring_cpu=3), lambda m:m.update(iterations=2),
                   lambda m:m.update(template_sha256='c'*64)]
        for change in changes:
            with tempfile.TemporaryDirectory() as directory:
                output = pathlib.Path(directory); document = write_campaign(output)
                path = output/'A2/provenance.json'; metadata = json.loads(path.read_text())
                change(metadata); diagnostic.save(output/'A2', 'provenance.json', metadata)
                document['files']['A2']['provenance.json']['sha256'] = diagnostic.evidence.digest(path.read_bytes())
                diagnostic.save(output, 'manifest.json', document)
                with self.assertRaises(ValueError): self.verify(output)
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory); document = write_campaign(output)
            path = output/'B1/samples.json'; rows = json.loads(path.read_text()); rows[0]['wall_ns'] += 1
            diagnostic.save(output/'B1', 'samples.json', rows)
            document['files']['B1']['samples.json']['sha256'] = diagnostic.evidence.digest(path.read_bytes())
            diagnostic.save(output, 'manifest.json', document)
            with self.assertRaisesRegex(ValueError, 'numeric journal'): self.verify(output)

    def test_symlinked_phase_or_member_rejects(self):
        for phase_link in (True, False):
            with tempfile.TemporaryDirectory() as directory:
                output = pathlib.Path(directory); write_campaign(output)
                source = output/'A1' if phase_link else output/'A1/samples.json'
                target = output/'saved'; source.rename(target); source.symlink_to(target, target_is_directory=phase_link)
                with self.assertRaisesRegex(ValueError, 'symlinked'): self.verify(output)

    def test_successful_capture_runs_all_phases_and_replays_saved_members(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); baseline = root/'baseline'; candidate = root/'candidate'
            output = root/'capture'; order = []
            def source(path): return BASELINE if path == baseline else CANDIDATE
            def capture(path, destination, group, count):
                order.append((path, destination.name, count))
                return write_phase(destination, source(path))
            with mock.patch.object(diagnostic.evidence, 'source_info', side_effect=source), mock.patch.object(diagnostic.evidence, 'source_at', side_effect=lambda root, rev: BASELINE if rev == BASELINE['commit'] else CANDIDATE), mock.patch.object(diagnostic.evidence, 'command', return_value='rustc controlled'), mock.patch.object(diagnostic, 'capture', side_effect=capture):
                report = diagnostic.capture_paired(baseline, candidate, output, root/'group', 1_000_000)
            self.assertEqual(order, [(baseline,'A1',1_000_000),(candidate,'B1',1_000_000),(candidate,'B2',1_000_000),(baseline,'A2',1_000_000)])
            self.assertTrue(report['layout_gate_passed'])
            self.assertFalse((output/'failure.json').exists())
            self.assertTrue((output/'pairing.json').exists())

    def test_capture_stops_after_complete_failed_phase_and_keeps_all_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); baseline = root/'baseline'; candidate = root/'candidate'
            output = root/'capture'
            def source(path): return BASELINE if path == baseline else CANDIDATE
            def capture(path, destination, group, count):
                rows = paired_records()
                for row in rows:
                    if row['index'] in (10,11):
                        row['wall_ns'] *= 2; row['ns_per_iter'] *= 2
                return write_phase(destination, source(path), rows)
            with mock.patch.object(diagnostic.evidence, 'source_info', side_effect=source), mock.patch.object(diagnostic, 'capture', side_effect=capture) as run:
                with self.assertRaisesRegex(ValueError, 'stop later phases'):
                    diagnostic.capture_paired(baseline, candidate, output, root/'group', 1_000_000)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(len(json.loads((output/'A1/samples.json').read_text())), 768)
            self.assertFalse((output/'manifest.json').exists())
            self.assertFalse((output/'B1').exists())
            self.assertFalse(json.loads((output/'failure.json').read_text())['release_approval'])

    def test_cleanup_preserves_foreign_modifications_and_prevents_completed_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); (root/'tests').mkdir(); output = root/'capture'
            def run(command, **kwargs):
                if '--no-run' in command: return
                kwargs['stdout'].write(''.join(diagnostic.PREFIX+json.dumps(r)+'\n' for r in paired_records()))
                kwargs['stdout'].flush()
                (root/'tests/rse_release_sampling.rs').write_text('foreign change')
            with mock.patch.object(diagnostic.evidence, 'source_info', return_value=CANDIDATE), mock.patch.object(diagnostic.evidence, 'environment_context', return_value=controlled_context()), mock.patch.object(diagnostic, 'run_checked', side_effect=run):
                with self.assertRaisesRegex(ValueError, 'preserve'):
                    diagnostic.capture(root, output, root/'group', 1_000_000)
            self.assertEqual((root/'tests/rse_release_sampling.rs').read_text(), 'foreign change')
            self.assertEqual(len(json.loads((output/'samples.json').read_text())), 768)
            self.assertFalse((output/'diagnosis.json').exists())

    def test_owned_worker_cancellation_reaps_child_and_restores_handlers(self):
        if not hasattr(os, 'killpg'): self.skipTest('POSIX process groups required')
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); ready = root/'ready'; done = root/'done'
            script = root/'controller.py'
            script.write_text("import sys,signal,json;sys.path.insert(0,"+repr(str(pathlib.Path(diagnostic.__file__).parent))+");import diagnose_verify_layout as d\n"
                              "old=signal.getsignal(signal.SIGTERM)\ntry:\n d.run_checked([sys.executable,'-c',"+
                              repr("import os,time;open("+repr(str(ready))+",'w').write(str(os.getpid()));time.sleep(60)")+ "])\nexcept KeyboardInterrupt: pass\n"
                              "open("+repr(str(done))+",'w').write(str(signal.getsignal(signal.SIGTERM)==old))\n")
            controller = subprocess.Popen([sys.executable, str(script)])
            try:
                deadline = time.monotonic()+5
                while not ready.exists() and time.monotonic() < deadline: time.sleep(.01)
                self.assertTrue(ready.exists())
                child = int(ready.read_text())
                controller.send_signal(signal.SIGTERM)
                self.assertEqual(controller.wait(timeout=5), 0)
                with self.assertRaises(ProcessLookupError): os.kill(child, 0)
                self.assertEqual(done.read_text(), 'True')
            finally:
                if controller.poll() is None: controller.kill(); controller.wait()

    def test_successful_worker_with_malformed_log_does_not_emit_completed_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); (root/'tests').mkdir(); output = root/'capture'
            def run(command, **kwargs):
                if '--no-run' not in command:
                    kwargs['stdout'].write(diagnostic.PREFIX+'bad json\n'); kwargs['stdout'].flush()
            with mock.patch.object(diagnostic.evidence, 'source_info', return_value=CANDIDATE), mock.patch.object(diagnostic.evidence, 'environment_context', return_value=controlled_context()), mock.patch.object(diagnostic, 'run_checked', side_effect=run):
                with self.assertRaisesRegex(ValueError, 'invalid numeric journal'):
                    diagnostic.capture(root, output, root/'group', 1_000_000)
            self.assertTrue((output/'extraction-error.txt').is_file())
            self.assertFalse((root/'tests/rse_release_sampling.rs').exists())
            self.assertFalse((output/'diagnosis.json').exists())


if __name__ == '__main__': unittest.main()
