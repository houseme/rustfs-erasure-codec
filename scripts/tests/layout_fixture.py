"""Synthetic layout records for refusal tests; never performance evidence."""
import copy
import json
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


def controlled_context():
    return {'compiler': 'rustc controlled', 'cpu_model': 'controlled CPU',
            'settings': dict(diagnostic.evidence.SAMPLING_ENV),
            'runtime': {'page_size': 4096, 'libc': {'name': 'glibc', 'version': 'test'},
                        'allocator_environment': {}},
            'isolation': {'scope': 'exclusive-vm-cpu-partition', 'cpu_affinity': [2,3],
                          'reserved_cpu_count': 2, 'physical_host_exclusivity_proven': False}}


def paired_records(iterations=1_000_000, context=None, provenance=None):
    rows = records()
    for row in rows:
        row.update(iterations=1_000_000, wall_ns=2_000_000_000, ns_per_iter=2000.,
                   input_4096_offsets=[row['source_offset']]*6, scratch_4096_offsets=[row['scratch_offset']]*2,
                   page_size=4096, cpu=2, backend='rust-scalar', backend_id='RustScalar',
                   backend_kind='Scalar', features='simd-accel|std', target_arch='x86_64', target_os='linux')
    context = controlled_context() if context is None else context
    for row in rows:
        row.update(iterations=iterations, ns_per_iter=row['wall_ns']/iterations,
                   cpu=min(context['isolation']['cpu_affinity']), page_size=context['runtime']['page_size'])
        if provenance is not None: row.update(provenance)
    return rows


BASELINE = {'commit': diagnostic.evidence.BASELINE_COMMIT, 'tree': 'a'*40, 'lock_sha256': 'a'*64}
CANDIDATE = {'commit': 'b'*40, 'tree': 'b'*40, 'lock_sha256': 'b'*64}


def write_phase(output, source, rows=None, *, tool=None, context=None, iterations=1_000_000, provenance=None):
    output.mkdir(parents=True)
    context = copy.deepcopy(controlled_context() if context is None else context)
    tool = CANDIDATE if tool is None else tool
    rows = paired_records(iterations, context, provenance) if rows is None else rows
    metadata = {'schema_version': 'verify-layout-diagnostic-v2', 'source': source,
                'tool': tool, 'context': context, 'iterations': iterations,
                'measuring_cpu': min(context['isolation']['cpu_affinity']), 'release_approval': False, 'candidate_attribution': None,
                **diagnostic.tool_contract()}
    diagnostic.save(output, 'provenance.json', metadata)
    diagnostic.save(output, 'samples.json', rows)
    (output/'measurement.log').write_text(''.join(diagnostic.PREFIX+json.dumps(row)+'\n' for row in rows))
    return diagnostic.analyze(rows, iterations, context['runtime']['page_size'], min(context['isolation']['cpu_affinity']))


def write_campaign(output, *, sources=None, tool=None, context=None, iterations=1_000_000, provenance=None):
    sources = {'baseline': BASELINE, 'candidate': CANDIDATE} if sources is None else sources
    tool = CANDIDATE if tool is None else tool
    document = {'schema_version': 'verify-layout-paired-v1', 'phase_order': list(diagnostic.evidence.PHASES),
                'sources': sources, 'tool': tool,
                'contract': diagnostic.tool_contract(), 'release_approval': False, 'files': {}}
    for phase in diagnostic.evidence.PHASES:
        write_phase(output/phase, sources['baseline' if phase.startswith('A') else 'candidate'], tool=tool, context=context, iterations=iterations, provenance=provenance)
        document['files'][phase] = {name: {'path': phase+'/'+name, 'sha256': diagnostic.evidence.digest((output/phase/name).read_bytes())}
                                    for name in ('provenance.json', 'samples.json', 'measurement.log')}
    diagnostic.save(output, 'manifest.json', document)
    return document
