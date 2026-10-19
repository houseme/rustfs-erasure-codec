"""Evidence contract tests use synthetic fixtures, never performance acceptance."""
import copy
import json
import pathlib
import subprocess
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as evidence
import release_sampling as sampling
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from layout_fixture import write_campaign


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.manifest = self.root / "manifest.json"
        self.sources = {
            "baseline": {"commit": evidence.BASELINE_COMMIT, "tree": "a" * 40, "lock_sha256": "a" * 64},
            "candidate": {"commit": "b" * 40, "tree": "c" * 40, "lock_sha256": "b" * 64},
        }
        context = {"compiler": "rustc fixture", "cpu_model": "fixture CPU", "settings": evidence.SAMPLING_ENV,
                   "runtime": {"libc": {"name": "fixture", "version": "1"}, "allocator_environment": {}, "page_size":4096},
                   "isolation": {"scope": "exclusive-vm-cpu-partition", "reserved_cpu_count": 8, "cpu_affinity": list(range(8)), "physical_host_exclusivity_proven": False}}
        self.doc = {"schema_version": evidence.SCHEMA_VERSION, "baseline_release": "9.0.0", "phase_order": list(evidence.PHASES),
                    "sources": self.sources, "harness_sha256": "d" * 64,
                    "contexts": {p: copy.deepcopy(context) for p in (*evidence.PHASES, "after")}, "files": {}}
        for phase in evidence.PHASES:
            (self.root / phase).mkdir()
            self.doc["files"][phase] = {}
            for profile in evidence.FILES:
                self.write_sampled_rows(phase, profile, self.fixture_rows(phase, profile))
        self.doc['sampling'] = dict(sampling.PROTOCOL, implementation_sha256='e'*64, overlay_sha256='f'*64,
                                    common_iterations={p:{k:1 for k in evidence.expected_cases(p)} for p in evidence.FILES})
        self.doc['calibration'] = {}
        for role in ('baseline', 'candidate'):
            folder = 'calibration-' + role
            (self.root / folder).mkdir()
            self.doc['calibration'][role] = {}
            for profile in evidence.FILES:
                records = []
                for row in self.fixture_rows('A1' if role == 'baseline' else 'B1', profile):
                    kinds = ['calibration_warmup'] * sampling.PROTOCOL['warmups'] + ['calibration'] * sampling.PROTOCOL['samples']
                    for index, kind in enumerate(kinds):
                        record = self.record(row, profile, role, kind, index)
                        record['complete'] = index == len(kinds)-1
                        record['calibration_batch'] = 1
                        records.append(record)
                self.doc['calibration'][role][profile] = self.write_samples(folder, profile, records)
        for name, value in (('implementation_digest', 'e'*64), ('overlay_digest', 'f'*64)):
            patch = mock.patch.object(sampling, name, return_value=value)
            patch.start()
            self.addCleanup(patch.stop)
        for name, value in (("source_info", self.sources["candidate"]), ("source_at", self.sources["baseline"]),
                            ("harness_digest", "d" * 64), ("command", "rustc fixture")):
            patch = mock.patch.object(evidence, name, return_value=value)
            patch.start()
            self.addCleanup(patch.stop)
        self.context = context
        self.write_layout()

    def write_layout(self, sources=None, context=None, iterations=1, provenance=None):
        layout_root = self.root/evidence.LAYOUT_DIRECTORY
        if layout_root.exists(): shutil.rmtree(layout_root)
        write_campaign(layout_root, sources=self.sources if sources is None else sources,
                       tool=self.sources['candidate'], context=self.context if context is None else context,
                       iterations=iterations, provenance=provenance or {'backend':'fixture', 'backend_id':'fixture', 'backend_kind':'fixture'})
        self.bind_layout()

    def bind_layout(self):
        path = self.root/evidence.LAYOUT_DIRECTORY/'manifest.json'
        self.doc['layout_supplement'] = {'path': evidence.LAYOUT_DIRECTORY+'/manifest.json', 'sha256': evidence.digest(path.read_bytes())}

    def rewrite_layout_samples(self, phase, mutate):
        from diagnose_verify_layout import PREFIX
        folder = self.root/evidence.LAYOUT_DIRECTORY
        path = folder/phase/'samples.json'
        rows = json.loads(path.read_text())
        mutate(rows)
        path.write_text(json.dumps(rows))
        (folder/phase/'measurement.log').write_text(''.join(PREFIX+json.dumps(row)+'\n' for row in rows))
        document = json.loads((folder/'manifest.json').read_text())
        for name in ('samples.json', 'measurement.log'):
            document['files'][phase][name]['sha256'] = evidence.digest((folder/phase/name).read_bytes())
        (folder/'manifest.json').write_text(json.dumps(document))
        self.bind_layout()

    def fixture_rows(self, phase, profile):
        result = []
        revision = self.sources["baseline" if phase.startswith("A") else "candidate"]["commit"]
        for key in sorted(evidence.expected_cases(profile)):
            row = {"schema_version": 1, "artifact_kind": evidence.KINDS[profile], "benchmark_metrics_enabled": False}
            if profile == "hotspot":
                row.update(scenario=key, data_shards=10, parity_shards=4, shard_size=1048576, useful_shards=1,
                           missing_pattern="0", required_pattern="0", baseline_operation="reconstruct", candidate_operation="reconstruct_data",
                           baseline_mb_s=100.0, candidate_mb_s=100.0, speedup=1.0)
            else:
                operation, data, parity, size, *block = key.split(":")
                row.update(operation=operation, data_shards=int(data), parity_shards=int(parity), shard_size=int(size),
                           throughput_mb_s=100.0, ns_per_iter=10_000_000.0, git_revision=revision[:12],
                           features="std|simd-accel", target_triple="x86_64-linux-unknown", backend="fixture",
                           backend_id="fixture", backend_kind="fixture", backend_override="auto", override_honored=True,
                           iterations=1)
                if profile != "smoke": row["profile"] = "fast"
                if block:
                    row.update(stream_block_size=int(block[0]), stream_io_backend="fixture", stream_io_mode="auto", ns_per_block=10_000_000.0)
            result.append(row)
        return result

    def write_rows(self, phase, profile, rows):
        name = f"{phase}/{evidence.FILES[profile]}"
        path = self.root / name
        path.write_text(json.dumps(rows) + "\n")
        self.doc["files"][phase].setdefault(profile, {}).update(path=name, sha256=evidence.digest(path.read_bytes()))

    def record(self, row, profile, role, kind, index):
        key = row.get('scenario') or ':'.join(str(row[f]) for f in ('operation','data_shards','parity_shards','shard_size','stream_block_size') if f in row)
        fixture = None
        if profile in ('small_file', 'stream'):
            position = {'len':row['shard_size'], 'capacity':row['shard_size'], 'offset_64':16, 'offset_4096':16, 'offset_page':16}
            fixture = {'entry':'Vec<Vec<u8>>', 'lifetime':'worker', 'page_size':4096,
                       'source':[copy.deepcopy(position) for _ in range(row['data_shards'] if row['operation']=='encode_stream' else row['data_shards']+row['parity_shards'])],
                       'scratch':[copy.deepcopy(position) for _ in range(row['parity_shards'] if row['operation']=='verify_with_buffer' else 0)], **sampling.fixture_contract(profile,key)}
        return {'fixture':fixture, 'kind':kind, 'case':key, 'revision':self.sources[role]['commit'], 'index':index, 'iterations':1,
                'wall_ns':120_000_000, 'timed_ns':10_000_000.0, 'complete':True, 'target_met':True, 'isolated_case':True, 'calibration_batch':0, 'rows':[copy.deepcopy(row)],
                'before':{'cpu':0, 'run_ns':index*100, 'wait_ns':index, 'switches':index},
                'after':{'cpu':0, 'run_ns':index*100+99, 'wait_ns':index+1, 'switches':index+1}}

    def write_samples(self, folder, profile, records):
        name = folder + '/' + profile + '-samples.json'
        path = self.root / name
        path.write_text(json.dumps(records))
        return {'path':name, 'sha256':evidence.digest(path.read_bytes())}

    def write_sampled_rows(self, phase, profile, rows):
        self.write_rows(phase, profile, rows)
        records = []
        for row in rows:
            records.extend(self.record(row, profile, 'baseline' if phase.startswith('A') else 'candidate', kind, i)
                           for i, kind in enumerate(['warmup'] * sampling.PROTOCOL['warmups'] + ['sample'] * sampling.PROTOCOL['samples']))
        self.doc['files'][phase][profile]['samples'] = self.write_samples(phase, profile, records)

    def verify(self):
        self.manifest.write_text(json.dumps(self.doc))
        return evidence.verify(self.manifest, self.root)

    def test_missing_paired_layout_is_not_complete_release_evidence(self):
        self.doc.pop('layout_supplement', None)
        with self.assertRaisesRegex(ValueError, 'layout'):
            self.verify()

    def test_complete_campaign_runs_existing_regression_gates(self):
        _, _, report = self.verify()
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["case_counts"], {"smoke": 15, "small_file": 88, "hotspot": 8, "stream": 15})
        self.assertEqual(report["drift_checks"], 252)
        self.assertEqual(report["sample_distribution_checks"], 504)
        self.assertEqual(report['layout_distribution_checks'], 256)
        self.assertEqual(report['layout_repeat_checks'], 256)
        self.assertEqual(report['layout_comparisons'], 256)
        self.assertEqual(report['layout_manifest_sha256'], self.doc['layout_supplement']['sha256'])

    def test_layout_paths_digests_and_missing_manifest_reject_before_gate_programs(self):
        original = copy.deepcopy(self.doc)
        for change in (lambda d:d['layout_supplement'].update(path='../manifest.json'),
                       lambda d:d['layout_supplement'].update(path='A1/manifest.json'),
                       lambda d:d['layout_supplement'].update(sha256='0'*64),
                       lambda d:d['layout_supplement'].update(sha256=True)):
            self.doc = copy.deepcopy(original); change(self.doc)
            with mock.patch.object(evidence.subprocess, 'run') as gate:
                with self.assertRaises(ValueError): self.verify()
                gate.assert_not_called()
        self.doc = copy.deepcopy(original)
        (self.root/evidence.LAYOUT_DIRECTORY/'manifest.json').unlink()
        with mock.patch.object(evidence.subprocess, 'run') as gate:
            with self.assertRaises(ValueError): self.verify()
            gate.assert_not_called()

    def test_layout_directory_and_manifest_symlinks_reject(self):
        directory = self.root/evidence.LAYOUT_DIRECTORY
        saved = self.root/'saved-layout'; directory.rename(saved); directory.symlink_to(saved, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlinked'): self.verify()
        directory.unlink(); saved.rename(directory)
        path = directory/'manifest.json'; path.rename(directory/'saved.json'); path.symlink_to(directory/'saved.json')
        with self.assertRaises(ValueError): self.verify()

    def test_independently_valid_layout_must_match_full_campaign_sources_and_context(self):
        sources = copy.deepcopy(self.sources)
        sources['candidate']['commit'] = 'd'*40  # Same tree/lock, different captured source identity.
        self.write_layout(sources=sources)
        with self.assertRaisesRegex(ValueError, 'source or resource context'): self.verify()
        for field in ('cpu_model', 'runtime', 'isolation'):
            context = copy.deepcopy(self.context)
            if field == 'cpu_model': context[field] = 'different controlled CPU'
            elif field == 'runtime': context[field]['page_size'] = 16384
            else: context[field]['cpu_affinity'] = [1,2]; context[field]['reserved_cpu_count'] = 2
            self.write_layout(context=context)
            with self.assertRaisesRegex(ValueError, 'source or resource context'): self.verify()

        # Independently valid external-tool capture must still belong to the
        # frozen candidate checkout used by the eventual tag verifier.
        tool = dict(self.sources['candidate'], commit='e'*40, tree='f'*40)
        folder = self.root/evidence.LAYOUT_DIRECTORY; shutil.rmtree(folder)
        write_campaign(folder, sources=self.sources, tool=tool, context=self.context, iterations=1,
                       provenance={'backend':'fixture', 'backend_id':'fixture', 'backend_kind':'fixture'})
        self.bind_layout()
        with mock.patch.object(evidence, 'source_info', side_effect=lambda root:self.sources['candidate'] if root == self.root else tool):
            with self.assertRaisesRegex(ValueError, 'tool does not match'): self.verify()

    def test_layout_uses_frozen_count_and_matching_original_backend_and_target(self):
        self.write_layout(iterations=2)
        with self.assertRaisesRegex(ValueError, 'frozen'): self.verify()
        for changes in ({'backend':'different'}, {'backend_id':'different'}, {'backend_kind':'different'}, {'target_arch':'aarch64'}):
            self.write_layout(provenance={'backend':'fixture', 'backend_id':'fixture', 'backend_kind':'fixture', **changes})
            with self.assertRaisesRegex(ValueError, 'backend/features/target'): self.verify()

    def test_failed_layout_drift_or_single_cell_regression_withholds_full_approval(self):
        for regression in (False, True):
            self.write_layout()
            def change(rows):
                for row in rows:
                    if row['len'] == 1024 and row['source_offset'] == row['scratch_offset'] == 0:
                        row['wall_ns'] = 113_000_000 if regression else 106_000_000
                        row['ns_per_iter'] = float(row['wall_ns'])  # One frozen iteration in this fixture.
            if regression: self.rewrite_layout_samples('B1', change)
            self.rewrite_layout_samples('B2', change)
            with self.assertRaises(evidence.EvidenceFailure) as raised: self.verify()
            self.assertIsNone(raised.exception.details['candidate_attribution'])
            self.assertFalse(raised.exception.details['layout']['release_approval'])
            self.assertFalse(raised.exception.details['layout']['layout_gate_passed'])
            if regression: self.assertEqual(len(raised.exception.details['layout']['regression_failures']), 8)
            else: self.assertTrue(raised.exception.details['layout']['repeat_failures'])

    def test_incomplete_layout_and_changed_numeric_rows_are_not_accepted_as_supplements(self):
        def remove(rows): rows.pop()
        self.rewrite_layout_samples('A2', remove)
        with self.assertRaisesRegex(ValueError, 'complete forward/reverse'): self.verify()
        self.write_layout()
        path = self.root/evidence.LAYOUT_DIRECTORY/'A1/samples.json'
        path.write_text('[]')
        with self.assertRaisesRegex(ValueError, 'changed'): self.verify()

    def test_materialize_never_stages_inputs_when_layout_quality_fails(self):
        def short(rows):
            for row in rows:
                row['wall_ns'] = 1000; row['ns_per_iter'] = 1000.
        self.rewrite_layout_samples('B2', short)
        self.manifest.write_text(json.dumps(self.doc))
        with self.assertRaises(evidence.EvidenceFailure):
            evidence.materialize(self.manifest, self.root, self.root/'inputs.env')
        self.assertFalse((self.root/'inputs.env').exists())
        self.assertFalse((self.root/'target/release-preflight/inputs').exists())

    def test_old_sampled_envelope_cannot_omit_new_required_layout_acceptance(self):
        self.doc['schema_version'] = 2
        self.doc.pop('layout_supplement')
        with mock.patch.object(evidence.subprocess, 'run') as gate:
            with self.assertRaisesRegex(ValueError, 'sampled schema 3'): self.verify()
            gate.assert_not_called()

    def test_layout_count_is_predeclared_from_both_calibrated_lengths(self):
        counts = {evidence.LAYOUT_CASES[0]:3, evidence.LAYOUT_CASES[1]:7}
        self.assertEqual(evidence.layout_iterations({'common_iterations':{'small_file':counts}}), 7)
        for bad in (None, True, 0, 1_000_000_001):
            with self.assertRaises(ValueError):
                evidence.layout_iterations({'common_iterations':{'small_file':dict(counts, **{evidence.LAYOUT_CASES[0]:bad})}})

    def test_real_cli_missing_layout_cannot_report_success_or_stage_inputs(self):
        self.doc.pop('layout_supplement')
        self.manifest.write_text(json.dumps(self.doc))
        script = pathlib.Path(evidence.__file__).resolve()
        report = self.root/'cli-report.json'
        for mode, option, output in (('verify', '--report', report), ('materialize', '--env-file', self.root/'inputs.env')):
            result = subprocess.run([sys.executable, str(script), mode, '--manifest', str(self.manifest),
                                     '--repository', str(self.root), option, str(output)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('paired-layout supplement', result.stderr)
        self.assertEqual(json.loads(report.read_text())['status'], 'failed')
        self.assertFalse((self.root/'inputs.env').exists())
        self.assertFalse((self.root/'target/release-preflight/inputs').exists())

    def test_wrong_source_lock_harness_baseline_and_self_comparison(self):
        original = copy.deepcopy(self.doc)
        mutations = (("baseline", "commit", "f" * 40), ("candidate", "commit", evidence.BASELINE_COMMIT),
                     ("candidate", "tree", "f" * 40), ("candidate", "lock_sha256", "f" * 64),
                     ("baseline", "tree", "c" * 40))
        for role, key, value in mutations:
            with self.subTest(role=role, key=key):
                self.doc = copy.deepcopy(original)
                self.doc["sources"][role][key] = value
                with self.assertRaises(ValueError): self.verify()
        self.doc = copy.deepcopy(original)
        self.doc["harness_sha256"] = "f" * 64
        with self.assertRaises(ValueError): self.verify()

    def test_rebase_with_identical_tree_is_accepted(self):
        current = dict(self.sources["candidate"], commit="e" * 40)
        with mock.patch.object(evidence, "source_info", return_value=current), mock.patch.object(evidence.subprocess, "run") as gate:
            report = self.verify()[2]
        self.assertEqual(report["candidate_commit"], "e" * 40)
        self.assertEqual(report["measured_candidate_commit"], "b" * 40)
        self.assertEqual(gate.call_count, 16)

    def test_empty_duplicate_missing_extra_wrong_revision_and_nonfinite(self):
        for mode in ("empty", "duplicate", "missing", "extra", "revision", "nonfinite", "zero", "features"):
            with self.subTest(mode=mode):
                rows = self.fixture_rows("A1", "small_file")
                if mode == "empty": rows = []
                elif mode == "duplicate": rows.append(rows[0])
                elif mode == "missing": rows.pop()
                elif mode == "extra": rows.append(dict(rows[0], shard_size=3))
                elif mode == "revision": rows[0]["git_revision"] = "badface"
                elif mode == "nonfinite": rows[0]["ns_per_iter"] = float("nan")
                elif mode == "zero": rows[0]["ns_per_iter"] = 0
                else: rows[0]["features"] = "std"
                self.write_rows("A1", "small_file", rows)
                with self.assertRaises(ValueError): self.verify()

    def test_original_smoke_export_uses_exact_catalog_iterations_and_manifest_profile(self):
        self.assertNotIn("profile", self.fixture_rows("A1", "smoke")[0])
        for field, value in (("iterations", 2), ("profile", "quick")):
            rows = self.fixture_rows("A1", "smoke")
            rows[0][field] = value
            self.write_rows("A1", "smoke", rows)
            with self.assertRaises(ValueError): self.verify()

    def test_every_phase_and_profile_required(self):
        original = copy.deepcopy(self.doc)
        for phase in evidence.PHASES:
            for profile in evidence.FILES:
                self.doc = copy.deepcopy(original)
                del self.doc["files"][phase][profile]
                with self.assertRaises(ValueError): self.verify()

    def test_tampered_bytes_path_and_symlinks(self):
        self.doc["files"]["A1"]["smoke"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "digest"): self.verify()
        for name in ("../escape", "/tmp/file", "A1/../smoke-results.json", "unknown/smoke-results.json"):
            with self.assertRaises(ValueError): evidence.safe_file(self.root, name)
        target = self.root / "A1" / evidence.FILES["smoke"]
        target.unlink()
        target.symlink_to(self.root / "B1" / evidence.FILES["smoke"])
        with self.assertRaises(ValueError): evidence.safe_file(self.root, "A1/" + evidence.FILES["smoke"])

    def test_environment_changes_including_after_reject(self):
        original = copy.deepcopy(self.doc)
        for phase in (*evidence.PHASES[1:], "after"):
            for key in ("compiler", "cpu_model", "settings", "isolation", "runtime"):
                self.doc = copy.deepcopy(original)
                self.doc["contexts"][phase][key] = "changed"
                with self.assertRaises(ValueError): self.verify()

    def test_allocator_runtime_records_controls_and_excludes_unrelated_environment(self):
        env = {'MALLOC_TRIM_THRESHOLD_':'100', 'GLIBC_TUNABLES':'glibc.malloc.top_pad=100', 'LD_PRELOAD':'fixture.so', 'LD_LIBRARY_PATH':'fixture-libs', 'UNRELATED':'private'}
        with mock.patch.object(evidence.platform, 'libc_ver', return_value=('glibc','2.39')):
            runtime = evidence.runtime_context(env)
        self.assertEqual(runtime['libc'], {'name':'glibc','version':'2.39'})
        self.assertEqual(set(runtime['allocator_environment']), {'MALLOC_TRIM_THRESHOLD_','GLIBC_TUNABLES','LD_PRELOAD','LD_LIBRARY_PATH'})
        with mock.patch.object(evidence.platform, 'libc_ver', return_value=('','')):
            with self.assertRaisesRegex(ValueError, 'libc identity'):
                evidence.runtime_context({})

    def test_missing_malformed_or_changed_allocator_provenance_rejects(self):
        original = copy.deepcopy(self.doc)
        for runtime in (None, {}, {'libc':{}, 'allocator_environment':{}},
                        {'libc':{'name':'glibc','version':'1'}, 'allocator_environment':{'UNRELATED':'value'}}):
            self.doc = copy.deepcopy(original)
            for context in self.doc['contexts'].values():
                context['runtime'] = runtime
            with self.assertRaisesRegex(ValueError, 'libc|allocator'):
                self.verify()
        self.doc = copy.deepcopy(original)
        self.doc['contexts']['A2']['runtime']['allocator_environment']['GLIBC_TUNABLES'] = 'glibc.malloc.top_pad=100'
        with self.assertRaisesRegex(ValueError, 'environment changed'):
            self.verify()

    def test_either_side_drift_withholds_comparison(self):
        for phase in ("A2", "B2"):
            original = self.fixture_rows(phase, "smoke")
            rows = copy.deepcopy(original)
            rows[0]["throughput_mb_s"] *= 1.06
            self.write_sampled_rows(phase, "smoke", rows)
            with mock.patch.object(evidence.subprocess, "run") as gate:
                with self.assertRaisesRegex(ValueError, "drift"): self.verify()
                gate.assert_not_called()
            self.write_sampled_rows(phase, "smoke", original)

    def test_exact_five_percent_repeat_boundary_is_inclusive_on_both_sides(self):
        for phase in ('A2', 'B2'):
            original = self.fixture_rows(phase, 'smoke')
            for factor in (.95, 1.05):
                rows = copy.deepcopy(original)
                rows[0]['throughput_mb_s'] *= factor
                self.write_sampled_rows(phase, 'smoke', rows)
                self.assertEqual(self.verify()[2]['status'], 'passed')
            for factor in (.94999999, 1.05000001):
                rows = copy.deepcopy(original)
                rows[0]['throughput_mb_s'] *= factor
                self.write_sampled_rows(phase, 'smoke', rows)
                with self.assertRaises(evidence.EvidenceFailure): self.verify()
            self.write_sampled_rows(phase, 'smoke', original)

    def test_stable_regression_rejected_by_existing_gate(self):
        for phase in ("B1", "B2"):
            rows = self.fixture_rows(phase, "smoke")
            rows[0]["throughput_mb_s"] = 50.0
            self.write_sampled_rows(phase, "smoke", rows)
        with self.assertRaises(subprocess.CalledProcessError): self.verify()

    def test_materialize_uses_verified_raw_files(self):
        self.manifest.write_text(json.dumps(self.doc))
        with mock.patch.object(evidence.subprocess, "run"):
            evidence.materialize(self.manifest, self.root, self.root / "inputs.env")
        lines = dict(line.split("=", 1) for line in (self.root / "inputs.env").read_text().splitlines())
        self.assertEqual(set(lines), set(evidence.VARIABLES.values()))
        for profile, filename in evidence.FILES.items():
            self.assertEqual(pathlib.Path(lines[evidence.VARIABLES[profile]]).read_bytes(), (self.root / "A1" / filename).read_bytes())
            self.assertEqual((self.root / "target/benchmark-smoke" / filename).read_bytes(), (self.root / "B2" / filename).read_bytes())

    def test_legacy_schema_cannot_bypass_sampling_or_materialize_release_inputs(self):
        self.doc['schema_version'] = 1
        self.doc.pop('sampling')
        self.doc.pop('calibration')
        for context in self.doc['contexts'].values():
            context['settings'] = evidence.BASE_ENV
        with mock.patch.object(evidence.subprocess, 'run') as gate:
            with self.assertRaisesRegex(ValueError, 'sampled schema 3'):
                self.verify()
            with self.assertRaisesRegex(ValueError, 'sampled schema 3'):
                evidence.materialize(self.manifest, self.root, self.root/'inputs.env')
            gate.assert_not_called()
        self.assertFalse((self.root/'inputs.env').exists())
        self.assertFalse((self.root/'target/release-preflight/inputs').exists())

    def test_capture_refuses_to_reuse_any_existing_output(self):
        report = self.root / "report.json"
        report.write_text("old observation")
        with self.assertRaisesRegex(ValueError, "new evidence directory"):
            evidence.capture(self.root, self.root, self.root, self.root)
        self.assertEqual(report.read_text(), "old observation")


if __name__ == "__main__": unittest.main()
