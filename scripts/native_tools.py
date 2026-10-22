"""Legacy capture bindings to repository-only native Rust tools.

Compile a native host tool and own one framed worker for repeated requests.
Native tools never invoke the Python interpreter.
"""
import functools
import atexit
import hashlib
import json
import os
import pathlib
import subprocess
import threading
from subprocess import Popen as _Popen, PIPE as _PIPE

ROOT = pathlib.Path(__file__).resolve().parents[1]
STREAM_CORE_MODULE = '__rse_stream_core'
LEOPARD_FILES = ('src/core/leopard_gf8/work.rs', 'src/core/leopard_gf8/decode.rs',
                 'src/core/leopard_gf16/work.rs', 'src/core/leopard_gf16/decode.rs')
_contract = json.loads((ROOT/'xtask/contracts/benchmark.json').read_text())
PROVENANCE_FIELDS = tuple(_contract['provenance_fields'])
CASE_FIELDS = tuple(_contract['hotspot_case_fields'])
_provenance_keys = frozenset(PROVENANCE_FIELDS).union(
    key for aliases in _contract['provenance_aliases'].values() for key in aliases)
_comparison_keys = _provenance_keys.union(CASE_FIELDS)
_pure_requests = frozenset(('benchmark-rows','hotspot-rows','benchmark-provenance','provenance-field',
                            'compatible-rows','observation-contracts','cpu-identity','memory-policy-validate','sampling','stream-views','diagnostic-ledger'))
_worker = None
_worker_owner = None
_worker_lock = threading.Lock()
_request_limit = 32*1024*1024 + 64*1024  # 32 MiB artifact plus the request envelope.


def _run(command, text=None, cwd=None):
    with _Popen(command, cwd=cwd, env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'),
                stdin=_PIPE if text is not None else None, stdout=_PIPE, stderr=_PIPE, text=True) as child:
        try:
            output, error = child.communicate(text)
        except BaseException:
            child.kill(); child.wait(); raise
        if child.returncode:
            raise ValueError(error.strip() or 'native repository tool failed')
        return output


@functools.lru_cache(maxsize=1)
def _executable():
    host = next(line[len('host: '):] for line in _run(['rustc', '-vV'], cwd=ROOT).splitlines() if line.startswith('host: '))
    output = _run(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path', str(ROOT/'xtask/Cargo.toml'),
                   '--package', 'xtask', '--target', host, '--message-format=json'], cwd=ROOT)
    executables = [row['executable'] for line in output.splitlines() for row in [json.loads(line)]
                   if row.get('reason') == 'compiler-artifact' and row.get('target', {}).get('name') == 'xtask' and row.get('executable')]
    if len(executables) != 1: raise ValueError('expected one native repository tool')
    return executables[0]


def _discard_worker():
    global _worker, _worker_owner
    child, owner = _worker, _worker_owner
    _worker = _worker_owner = None
    if child is None:
        return
    try:
        child.stdin.close()
    except (OSError, ValueError):
        pass
    if owner == os.getpid():
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.terminate()
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
    child.stdout.close()


def close_worker():
    with _worker_lock:
        _discard_worker()


atexit.register(close_worker)


def _request_payload(payload):
    global _worker, _worker_owner
    if len(payload.encode())+1 > _request_limit:
        raise ValueError('native request exceeds artifact transport byte budget')
    with _worker_lock:
        if _worker_owner != os.getpid():
            _discard_worker()
        try:
            if _worker is None:
                _worker_owner = os.getpid()
                _worker = _Popen([_executable(), 'native-helper', '--serve'],
                                env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'),
                                stdin=_PIPE, stdout=_PIPE, text=True, bufsize=1)
            _worker.stdin.write(payload+'\n')
            _worker.stdin.flush()
            line = _worker.stdout.readline(32*1024*1024+1)
            if not line.endswith('\n'):
                raise ValueError('native worker stopped or exceeded its response limit')
            reply = json.loads(line)
            valid = (isinstance(reply, dict) and
                     ((reply.get('ok') is True and set(reply) == {'ok','value'}) or
                      (reply.get('ok') is False and set(reply) == {'ok','error'} and isinstance(reply['error'], str))))
            if not valid:
                raise ValueError('invalid native worker response')
        except BaseException:
            _discard_worker()
            raise
        if not reply['ok']:
            raise ValueError(reply['error'])
        return reply['value']


def _payload(values):
    try:
        return json.dumps(values, sort_keys=True, allow_nan=False, separators=(',',':'))
    except ValueError as error:
        raise ValueError('native request requires finite JSON numbers') from error


def request(kind, **values):
    payload = _payload(dict(kind=kind, **values))
    if kind not in _pure_requests:
        # File adapters retain the caller's current directory. Live policy must
        # inherit its current process flags, rather than an older worker's state.
        return json.loads(_run([_executable(), 'native-helper'], payload))
    return _request_payload(payload)


def implementation_bytes():
    files = [pathlib.Path(__file__).resolve(), ROOT/'Cargo.toml', ROOT/'Cargo.lock', ROOT/'xtask/Cargo.toml',
             *sorted((ROOT/'xtask/src').rglob('*.rs')), *sorted((ROOT/'xtask/contracts').glob('*.json'))]
    return b''.join(str(path.relative_to(ROOT)).encode()+b'\0'+path.read_bytes() for path in files)


def implementation_digest():
    return hashlib.sha256(implementation_bytes()).hexdigest()


def stream_source_patch(text): return request('stream-source', text=text)
def verify_par_patch(text): return request('verify-par-source', text=text)
def native_template(kind): return request('template', variant=kind, tool=str(ROOT))


def diagnostic_render(root, variant, allocation_check=False):
    return request('diagnostic-render', root=str(root), tool=str(ROOT), variant=variant, allocation_check=allocation_check)


def capture_memory_policy():
    return request('memory-policy-capture')


@functools.lru_cache(maxsize=128)
def _validate_memory_policy(payload):
    _request_payload(payload)


def validate_memory_policy(value, base_page_size=None):
    # Cache successful, pure validation only. Re-serialize each current value so
    # later mutations cannot reuse a previous result; never cache live capture.
    payload = _payload(dict(kind='memory-policy-validate', value=value, base_page_size=base_page_size))
    _validate_memory_policy(payload)


@functools.lru_cache(maxsize=512)
def _normalized_rows(payload):
    # Cache bytes, not mutable returned dictionaries. Large batches bypass this
    # bounded compatibility cache; native operations already accept whole arrays.
    return json.dumps(_request_payload(payload), allow_nan=False)


def _rows(kind, **values):
    payload = _payload(dict(kind=kind, **values))
    return (json.loads(_normalized_rows(payload)) if len(payload)<65536 else _request_payload(payload))


def index_records(rows, source='artifact', strict_provenance=False):
    normalized = _rows('benchmark-rows', rows=rows, source=source, strict_provenance=strict_provenance)
    return {(row['operation'], row['data_shards'], row['parity_shards'], row['shard_size'], row['stream_block_size']): row
            for row in normalized}


def index_results(rows, metric):
    normalized = _rows('hotspot-rows', rows=rows, metric=metric)
    for original, row in zip(rows, normalized):
        original.update(row)
    return {row['scenario']: row for row in rows}


@functools.lru_cache(maxsize=512)
def _provenance_outcomes(payload):
    return _request_payload(payload)


def provenance_value(row, field):
    if field not in PROVENANCE_FIELDS:
        return request('provenance-field', row=row, field=field)
    # Timing/case fields cannot affect provenance. Include every current alias
    # so repeated observations share work while conflicting/mutated aliases fail.
    projected = {key: value for key, value in row.items() if key in _provenance_keys}
    payload = _payload(dict(kind='benchmark-provenance', row=projected))
    values = (_provenance_outcomes(payload) if len(payload)<65536 else _request_payload(payload))
    if field in values['errors']:
        raise ValueError(values['errors'][field])
    return values['values'][field]


def observation_contracts(profile, records, reference=None):
    def row(value):
        return {key: item for key, item in value.items() if key in _comparison_keys}
    contracts = dict(reference) if reference is not None else {}
    for offset in range(0, len(records), 64):
        group = records[offset:offset+64]
        projected = [dict(case=r['case'], kind=r.get('kind','observation'), index=r.get('index','?'),
                          calibration_batch=r.get('calibration_batch',0),
                          rows=[row(r['rows'][0])], placement=r['placement']) for r in group]
        first = request('observation-contracts', profile=profile, records=projected,
                        reference={case:(row(value),placement) for case,(value,placement) in contracts.items()},
                        collect=reference is None)
        for case,index in first:
            contracts[case] = (group[index]['rows'][0],group[index]['placement'])
    return contracts


def sampling_request(operation, **values):
    return request('sampling', request=dict(operation=operation, **values))


@functools.lru_cache(maxsize=1)
def _sampling_catalog():
    return _payload(sampling_request('catalog'))


def sampling_catalog():
    return json.loads(_sampling_catalog())


@functools.lru_cache(maxsize=512)
def _sampling_contract(operation, profile, case):
    return _payload(sampling_request(operation, profile=profile, case=case))


def sampling_contract(operation, profile, case):
    return json.loads(_sampling_contract(operation, profile, case))
