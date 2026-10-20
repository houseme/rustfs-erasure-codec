"""Portable, guest-reported CPU identity; never a claim of host exclusivity."""
import copy
import pathlib
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import release_evidence as evidence


def arm_record(cpu, part='0x000'):
    return {'processor': str(cpu), 'CPU implementer': '0x61',
            'CPU architecture': '8', 'CPU variant': '0x0',
            'CPU part': part, 'CPU revision': '0', 'Features': 'fp asimd aes'}


def cpuinfo(records):
    return '\n\n'.join('\n'.join(f'{k}\t: {v}' for k, v in r.items()) for r in records)


class CpuIdentityTests(unittest.TestCase):
    def test_arm_environment_accepts_actual_proc_fields_without_model_name(self):
        text = cpuinfo([arm_record(0), arm_record(1)])
        isolation = {'cpu_affinity': [0, 1]}
        with mock.patch.object(evidence, 'isolation_context', return_value=isolation), \
             mock.patch.object(evidence.pathlib.Path, 'read_text', return_value=text), \
             mock.patch.object(evidence.platform, 'machine', return_value='aarch64'), \
             mock.patch.object(evidence, 'command', return_value='rustc fixture'), \
             mock.patch.object(evidence, 'runtime_context', return_value={}):
            result = evidence.environment_context(pathlib.Path('.'), evidence.SAMPLING_ENV, pathlib.Path('/unused'))
        self.assertTrue(result['cpu_model'].startswith('aarch64 guest CPU identity: '))
        self.assertEqual(result['isolation'], isolation)

    def test_named_cpu_model_keeps_existing_x86_context_string(self):
        self.assertEqual(evidence.cpu_identity('processor: 0\nmodel name: fixture x86 CPU\n', 'x86_64', [0]), 'fixture x86 CPU')

    def test_arm_identity_is_canonical_and_bound_to_reserved_cpus(self):
        first = arm_record(0)
        second = arm_record(1, '0x001')
        before = evidence.cpu_identity(cpuinfo([first, second]), 'aarch64', [0, 1])
        reordered = [dict(reversed(list(second.items()))), dict(reversed(list(first.items())))]
        reordered[0]['Features'] = 'aes fp asimd'
        self.assertEqual(before, evidence.cpu_identity(cpuinfo(reordered), 'aarch64', [1, 0]))
        self.assertNotEqual(before, evidence.cpu_identity(cpuinfo([first, second]), 'aarch64', [0]))
        for field, value in [('CPU part', '0x002'), ('CPU variant', '0x1'), ('CPU revision', '1'), ('Features', 'fp asimd')]:
            changed = copy.deepcopy(second)
            changed[field] = value
            with self.subTest(field=field):
                self.assertNotEqual(before, evidence.cpu_identity(cpuinfo([first, changed]), 'aarch64', [0, 1]))
                self.assertEqual(evidence.cpu_identity(cpuinfo([first, second]), 'aarch64', [0]), evidence.cpu_identity(cpuinfo([first, changed]), 'aarch64', [0]))

    def test_incomplete_or_malformed_arm_identity_is_refused(self):
        original = arm_record(0)
        for field in original:
            changed = dict(original)
            del changed[field]
            with self.subTest(missing=field), self.assertRaises(ValueError):
                evidence.cpu_identity(cpuinfo([changed]), 'aarch64', [0])
        for field in ['processor', 'CPU implementer', 'CPU architecture', 'CPU variant', 'CPU part', 'CPU revision']:
            changed = dict(original)
            changed[field] = 'invalid'
            with self.subTest(malformed=field), self.assertRaises(ValueError):
                evidence.cpu_identity(cpuinfo([changed]), 'aarch64', [0])
        for records in [[], [original, original]]:
            with self.assertRaises(ValueError):
                evidence.cpu_identity(cpuinfo(records), 'aarch64', [0])
        with self.assertRaises(ValueError):
            evidence.cpu_identity(cpuinfo([original]), 'aarch64', [0, 1])

    def test_arm_fields_do_not_invent_a_model_for_an_unknown_architecture(self):
        with self.assertRaises(ValueError):
            evidence.cpu_identity(cpuinfo([arm_record(0)]), 'unknown', [0])
