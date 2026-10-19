"""Real policy parsing/refusal fixtures; no synthetic performance acceptance."""
import copy
import ctypes
import errno
import pathlib
import sys
import tempfile
import unittest
from unittest import mock
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import release_memory_policy as policy


def fixture():
    return {'kernel_release':'fixture-linux','process_thp_disable':0,'thp':{'enabled':'madvise','defrag':'defer+madvise','shmem_enabled':None,'sizes_kib':{'16':{'anonymous':'always','shmem':None},'2048':{'anonymous':'inherit','shmem':None}}}}


class MemoryPolicyTests(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.directory.cleanup);self.root=pathlib.Path(self.directory.name)
        (self.root/'enabled').write_text('always [madvise] never\n')
        (self.root/'defrag').write_text('always defer [defer+madvise] madvise never\n')
        for size,choice in ((2048,'inherit'),(16,'always')):
            p=self.root/f'hugepages-{size}kB';p.mkdir();(p/'enabled').write_text(' '.join('['+v+']' if v==choice else v for v in ('always','inherit','madvise','never')))
        self.stack=mock.patch.object(policy.platform,'system',return_value='Linux');self.stack.start();self.addCleanup(self.stack.stop)

    def capture(self,mode=0):
        with mock.patch.object(policy.platform,'release',return_value='fixture-linux'),mock.patch.object(policy,'_process_mode',return_value=mode):return policy.capture(self.root)

    def test_read_only_global_and_per_size_choices_and_full_process_modes(self):
        before={p.relative_to(self.root):p.read_bytes() for p in self.root.rglob('*') if p.is_file()}
        for mode in (0,1,3):
            result=self.capture(mode);expected=fixture();expected['process_thp_disable']=mode
            self.assertEqual(result,expected);self.assertEqual(list(result['thp']['sizes_kib']),['16','2048']);policy.validate(result)
        self.assertEqual(before,{p.relative_to(self.root):p.read_bytes() for p in self.root.rglob('*') if p.is_file()})

    def test_legacy_global_only_kernel_is_explicit_not_an_inferred_size_policy(self):
        for p in self.root.glob('hugepages-*'):(p/'enabled').unlink();p.rmdir()
        self.assertEqual(self.capture()['thp']['sizes_kib'],{})

    def test_shmem_only_sizes_have_explicit_absent_anonymous_interface(self):
        p=self.root/'hugepages-8kB';p.mkdir();(p/'shmem_enabled').write_text('always inherit within_size advise [never]')
        result=self.capture()
        self.assertEqual(result['thp']['sizes_kib']['8'],{'anonymous':None,'shmem':'never'})
        (p/'shmem_enabled').write_text('[unknown]')
        with self.assertRaises(ValueError):self.capture()

    def test_missing_unreadable_or_malformed_policy_never_becomes_disabled(self):
        with self.assertRaisesRegex(ValueError,'observable THP'):policy.capture(self.root/'absent')
        p=self.root/'hugepages-16kB/enabled';original=p.read_text();p.unlink()
        with self.assertRaisesRegex(ValueError,"interfaces"):self.capture()
        p.write_text(original)
        with mock.patch.object(policy,'_text',side_effect=PermissionError('denied')):
            with self.assertRaises(PermissionError):self.capture()
        for text in ('always madvise never','[always] [madvise] never','always [unknown] never','always [madvise] never extra','always always [madvise]'):
            p.write_text(text)
            with self.assertRaises(ValueError):self.capture()
        p.write_bytes(b'\xff')
        with self.assertRaisesRegex(ValueError,'ASCII'):self.capture()
        p.write_text('a'*4097)
        with self.assertRaisesRegex(ValueError,'oversized'):self.capture()

    def test_unknown_or_non_power_of_two_sizes_are_rejected(self):
        for name in ('hugepages-24kB','hugepages-16MB'):
            p=self.root/name;p.mkdir();(p/'enabled').write_text('[never]')
            with self.assertRaises(ValueError):self.capture()
            (p/'enabled').unlink();p.rmdir()

    def test_malformed_complete_contexts_are_refused_without_mutation(self):
        original=fixture()
        for change in (lambda d:d.pop('kernel_release'),lambda d:d.update(kernel_release='\n'),lambda d:d.update(process_thp_disable=False),lambda d:d.update(process_thp_disable=2),lambda d:d['thp'].pop('sizes_kib'),lambda d:d['thp'].update(enabled=None),lambda d:d['thp'].update(defrag=[]),lambda d:d['thp']['sizes_kib'].update({'24':'always'}),lambda d:d['thp']['sizes_kib'].update({'16':False}),lambda d:d['thp'].update(unknown='never')):
            value=copy.deepcopy(original);change(value);before=copy.deepcopy(value)
            with self.assertRaises(ValueError):policy.validate(value)
            self.assertEqual(value,before)

    def test_unavailable_or_invalid_process_prctl_does_not_default_to_enabled(self):
        for failure in (OSError(errno.EPERM,'denied'),OSError(errno.EINVAL,'unsupported')):
            with mock.patch.object(policy,'_process_mode',side_effect=failure):
                with self.assertRaises(OSError):policy.capture(self.root)
        with self.assertRaises(ValueError):self.capture(2)

    def test_prctl_is_read_only_and_preserves_except_advised_bits(self):
        for mode in (0,1,3):
            function=mock.Mock(return_value=mode);lib=mock.Mock(prctl=function)
            with mock.patch.object(policy.ctypes,'CDLL',return_value=lib):self.assertEqual(policy._process_mode(),mode)
            function.assert_called_once_with(42,0,0,0,0)
            self.assertEqual(function.argtypes,[ctypes.c_int,*([ctypes.c_ulong]*4)])
        lib=mock.Mock(prctl=mock.Mock(return_value=-1))
        with mock.patch.object(policy.ctypes,'CDLL',return_value=lib),mock.patch.object(policy.ctypes,'get_errno',return_value=errno.EPERM):
            with self.assertRaises(OSError) as error:policy._process_mode()
        self.assertEqual(error.exception.errno,errno.EPERM)

    def test_size_must_exceed_the_recorded_base_page_and_inheritance_has_a_parent(self):
        value=fixture()
        with self.assertRaisesRegex(ValueError,'mTHP size'):policy.validate(value,16384)
        value['thp']['sizes_kib']['16']['shmem']='inherit'
        with self.assertRaisesRegex(ValueError,'global control'):policy.validate(value,4096)

    def test_unbounded_size_inventory_is_rejected(self):
        with mock.patch.object(policy,'MAX_SIZES',1):
            with self.assertRaisesRegex(ValueError,'too many'):self.capture()

    def test_non_linux_cannot_claim_kernel_policy(self):
        with mock.patch.object(policy.platform,'system',return_value='Darwin'):
            with self.assertRaisesRegex(ValueError,'Linux'):policy.capture(self.root)
