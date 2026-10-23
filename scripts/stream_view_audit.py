"""Diagnostic decorators for real stream buffer views; never formal release timing."""
import hashlib
import pathlib
import native_tools
import release_sampling as sampling

PREFIX = 'RSE_STREAM_VIEWS '
ENTRIES = native_tools.request('stream-views', request=dict(operation='entries'))


def native_template():
    return native_tools.native_template('views')


def implementation_digest():
    return hashlib.sha256(pathlib.Path(__file__).read_bytes()+native_template().encode()+native_tools.implementation_bytes()).hexdigest()


def render(root):
    return native_tools.diagnostic_render(root, 'views')


def underlying_records(records):
    return native_tools.request('stream-views', request=dict(operation='underlying', records=records))


def validate(view, case, iterations, page_size, observation, source_fixture=None, *, require_counters=False):
    native_tools.request('stream-views', request=dict(operation='validate', observations=[dict(
        view=view, case=case, iterations=iterations, page_size=page_size,
        observation=observation, source_fixture=source_fixture, require_counters=require_counters)]))


def validate_samples(views, samples, iterations, page_size, *, require_counters=False, summary=False, inventory=False):
    if len(views) != len(samples): raise ValueError('missing stream-view observations')
    result=[];signatures=[]
    # Four bounded ledgers fit one native frame, including full u64 counters.
    for offset in range(0,len(views),4):
        observations=[dict(view=view, case=row['case'], iterations=iterations, page_size=page_size,
                           observation=row['index'], source_fixture=row['fixture']['source'],
                           require_counters=require_counters)
                      for view,row in zip(views[offset:offset+4],samples[offset:offset+4])]
        request=dict(operation='inventory' if inventory else 'validate', observations=observations)
        if not inventory:request['summary']=summary
        reply=native_tools.request('stream-views', request=request)
        if inventory:
            result.extend(reply['lifecycle']);signatures.extend(reply['signatures'])
        else:result.extend(reply)
    return dict(lifecycle=result,signatures=signatures) if inventory else result


def summarize_windows(view):
    return native_tools.request('stream-views', request=dict(operation='summarize',
                                view=dict(calls=view['calls'], windows=view['windows'])))


def check(repository, baseline_ref=None):
    """Compile both untouched codecs and exercise every stream case structurally."""
    import os
    import subprocess
    import tempfile
    from diagnostic_overlay import overlay
    import release_evidence as evidence
    def source(root):
        settings=evidence.capture_environment()
        with overlay(root,render(root)):
            subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel',
                            '--test','rse_release_sampling','stream_views','--','--nocapture'],
                           cwd=root,env=sampling.build_environment(root,settings),check=True)
            result=[]
            for case in sorted(evidence.expected_cases('stream')):
                env=sampling.worker_environment(dict(settings,RSE_SAMPLER_PROFILE='stream',
                    RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='1',
                    RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(sampling.base_page_size())))
                proc=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel',
                    '--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],
                    cwd=root,env=sampling.build_environment(root,env),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
                if proc.returncode:print(proc.stdout,end='');proc.check_returncode()
                rows=sampling.parse_output(proc.stdout);views=sampling.parse_output(proc.stdout,PREFIX)
                if len(rows)!=12 or len(views)!=12:raise ValueError('incomplete native view worker')
                sampling.validate_workload_sequence(underlying_records(rows),'stream')
                validate_samples(views,rows,1,sampling.base_page_size(),require_counters=hasattr(os,'sched_getaffinity'))
                for index,(row,view) in enumerate(zip(rows,views)):
                    if row['case']!=case or row['index']!=index:raise ValueError('native view worker case/order mismatch')
                    if hasattr(os,'sched_getaffinity'):
                        sampling.validate_placement(row,os.sched_getaffinity(0))
                        sampling.validate_telemetry(row,os.sched_getaffinity(0))
                result.append(case)
            print('All 15 stream view workers executed; structural/semantic checks only, no performance approval')
            return result
    current=source(repository)
    if baseline_ref:
        with tempfile.TemporaryDirectory(prefix='rse-stream-view-baseline-') as directory:
            baseline=pathlib.Path(directory)/'baseline'
            sampling.checkout_baseline(repository,baseline,baseline_ref)
            if current!=source(baseline):raise ValueError('stream view catalogs differ between sources')
