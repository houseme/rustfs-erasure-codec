"""Generate and validate the same external sampling overlay on unmodified codecs."""
import hashlib
import json
import math
import os
import pathlib
import re
import time

TEMPLATES = pathlib.Path(__file__).resolve().parent / "release-sampling"


def service_window_contract():
    """Use the same frozen constants in the native worker and offline verifier."""
    text=(TEMPLATES/'service_window.rs').read_text()
    fields=('target_wall_ns','target_timed_ns','minimum_wall_ns','minimum_timed_ns')
    result={}
    for field in fields:
        matches=re.findall(r'^pub\(super\) const '+field.upper()+r': u64 = ([0-9_]+);$',text,re.M)
        if len(matches)!=1:raise ValueError('service-window declaration changed')
        result[field]=int(matches[0].replace('_',''))
    if not 0<result['minimum_timed_ns']<=result['target_timed_ns']<=result['target_wall_ns'] or not 0<result['minimum_wall_ns']<=result['target_wall_ns']:
        raise ValueError('invalid service-window bounds')
    return result


PROTOCOL = {"name": "shared-external-duration-sampler", "version": 15, "samples": 9, "warmups": 3, "case_isolation": "fresh-process-per-count", "case_order": "lexicographic", "max_calibration_batches": 12, "max_calibration_records": 144,
            **service_window_contract(),
            "service_window_policy": "uniform-duration-before-calibration; all-126-cases-and-paired-layouts; no-count-floor-or-case-exceptions",
            "central_spread_limit": .10,
            "observer_buffer_policy": "bounded-stack-procfs-no-rust-heap",
            "fault_counter_scope": "guest-process-including-workers-excluding-children; no IDs; outside inner timers",
            "result_metadata_policy": "bounded-inline-hotspot-patterns-deferred-formatting",
            "stream_output_policy": "independent-capacity-per-writer-per-iteration",
            "workload_contract": {"scope":"all-126-cases", "codec_lifetime":"worker", "hotspot_input":"worker-seeded-SmallRng-u32-low-byte", "state_witness":"generated-byte-count-and-next-two-u64-words", "geometry_scope":"unchanged-caller-only; dynamic destinations and internal views untracked"},
            "thread_placement": {"measuring_cpu":"minimum-reserved-cpu", "pin":"thread-only-after-codec-and-pool-preparation", "pool_affinity":"complete-reservation", "observation_checks":"actual-mask-before-and-after-timer"},
            "fixture_layout": {"scope": "small_file:all;stream:all;caller-storage-only", "codec_lifetime": "worker", "dynamic_outputs": "original per-iteration policy; offsets explicitly untracked", "entry": "Vec<Vec<u8>>", "lifetime": "worker", "construction": "caller-buffers-before-codec", "offset_moduli": [64, 4096, "os_base_page_size"], "page_size_source": "controller-sysconf", "untracked_cases": "original allocation contract; no layout guarantee"},
            "timing_scope": {"smoke":"original per-iteration workload", "small_file":"operation loops excluding one-time fixture preparation", "stream":"operation loops excluding one-time fixture preparation", "hotspot":"original inner recovery intervals"}}
HOTSPOT_BLOCKS = ("struct ReconstructionHotspotBenchResult", "fn benchmark_test_iterations", "macro_rules! make_random_shards",
    "pub fn fill_random", "fn shards_to_option_shards", "fn option_shards_with_missing", "fn shard_index_pattern",
    "fn assert_required_reconstruction_matches", "fn bench_reconstruct_data_hotspot", "fn bench_reconstruct_some_hotspot",
    "fn write_reconstruction_hotspot_bench_results")


def block(text, name):
    start = text.index(name)
    return text[start:text.index("\n}", start) + 2]


def implementation_digest():
    files = [pathlib.Path(__file__).resolve(), *sorted(TEMPLATES.glob("*.rs"))]
    return hashlib.sha256(b"".join(p.name.encode() + b"\0" + p.read_bytes() for p in files)).hexdigest()


def exclude_once_only_setup(text, name, loop_count):
    original = block(text, name)
    # Operate only on the known rustfmt layout, and prove no non-timing statement
    # changes. Per-iteration reconstruction clones and stream adapters remain timed.
    starts = list(re.finditer(r"(?m)^            for _ in 0\.\.iterations \{", original))
    if len(starts) != loop_count:
        raise ValueError("benchmark loop layout changed; review the timing adapter")
    updated = original
    for start in reversed(starts):
        close = re.search(r"(?m)^            }$", original[start.end():])
        if close is None: raise ValueError("benchmark loop closing scope changed")
        end = start.end() + close.end()
        updated = updated[:end] + "\n            measured += start.elapsed();" + updated[end:]
        updated = updated[:start.start()] + "            let start = Instant::now();\n" + updated[start.start():]
    updated = updated.replace("    let start = Instant::now();", "    let mut measured = std::time::Duration::ZERO;", 1)
    updated = updated.replace("    let elapsed = start.elapsed();", "    let elapsed = measured;", 1)
    restored = updated.replace("            let start = Instant::now();\n", "").replace("\n            measured += start.elapsed();", "")
    restored = restored.replace("    let mut measured = std::time::Duration::ZERO;", "    let start = Instant::now();", 1).replace("    let elapsed = measured;", "    let elapsed = start.elapsed();", 1)
    if restored != original: raise ValueError("timing adapter changed a non-timing statement")
    return text.replace(original, updated, 1)


def reserve_stream_outputs(text):
    """Correct one declared fixture constructor on both untouched source trees."""
    original = "let mut writers = vec![Vec::with_capacity(case.shard_size); case.parity_shards];"
    replacement = "let mut writers = reserved_stream_outputs(case.parity_shards, case.shard_size);"
    if text.count(original) != 1:
        raise ValueError("stream output construction changed; review the shared fixture adapter")
    return text.replace(original, replacement, 1)


# The loop body remains authoritative in the original benchmark source. These
# descriptors define only the explicit caller-fixture and API contracts.
SMALL_KERNELS = (
    ("SmallFileOp::Standard(Operation::Encode)", "encode"),
    ("SmallFileOp::Standard(Operation::Verify)", "verify"),
    ("SmallFileOp::VerifyWithBuffer", "verify_with_buffer"),
    ("SmallFileOp::Standard(Operation::Reconstruct)", "reconstruct"),
    ("SmallFileOp::ReconstructOpt", "reconstruct_opt"),
    ("SmallFileOp::ReconstructShardSlot", "reconstruct_shard_slot"),
    ("SmallFileOp::ReconstructSomeDataOnly", "reconstruct_some_data_only"),
    ("SmallFileOp::Standard(Operation::ReconstructData)", "reconstruct_data"),
)
STREAM_KERNELS = tuple(("StreamOperation::"+name, name.lower()+"_stream")
                       for name in ("Encode", "Verify", "Reconstruct"))


BASE_SEED = 0x00EC_5EED_2026_0524
HOTSPOT_CASES = (
    (10,4,"reconstruct_data_missing_1_data",(0,),None),
    (10,4,"reconstruct_data_missing_2_data",(0,2),None),
    (10,4,"reconstruct_data_missing_data_plus_parity",(1,10),None),
    (32,16,"reconstruct_data_32x16_missing_2_data",(0,2),None),
    (10,4,"reconstruct_some_required_1_of_2_missing_data",(0,2),(2,)),
    (10,4,"reconstruct_some_required_2_of_3_missing_data",(0,2,4),(0,4)),
    (10,4,"reconstruct_some_required_data_and_skip_parity",(1,10),(1,)),
    (32,16,"reconstruct_some_32x16_required_2_of_4_missing_data",(0,2,4,6),(2,6)),
)


def case_seed(operation, data, parity, size):
    tag = {"encode":0x11,"verify":0x22,"reconstruct":0x33,"reconstruct_data":0x44,"update":0x55}[operation]
    return BASE_SEED ^ tag ^ (data << 48) ^ (parity << 32) ^ size


def hotspot_case(case):
    try: return next(row for row in HOTSPOT_CASES if row[2] == case)
    except StopIteration: raise ValueError('unknown hotspot workload') from None


def workload_contract(profile, case):
    if profile == 'hotspot':
        data, parity, name, missing, required = hotspot_case(case)
        name_hash = 0xcbf29ce484222325
        for value in name.encode(): name_hash = ((name_hash ^ value)*0x100000001b3) & (2**64-1)
        seed = case_seed('reconstruct_data',data,parity,1048576) ^ name_hash
        contract = {'call_entry':'Vec<Option<Vec<u8>>>',
                    'output_policy':'iteration-random-source-cloned-baseline-and-candidate-missing-outputs-untracked',
                    'missing_mask':sum(1 << i for i in missing),
                    'required_mask':sum(1 << i for i in (required if required is not None else range(data))),
                    'changed_mask':0}
        lifetime, construction, generator = 'iteration', 'codec-before-iteration-source', 'worker-seeded-SmallRng-u32-low-byte'
    else:
        parts = case.split(':')
        if len(parts) != (5 if profile == 'stream' else 4): raise ValueError('invalid workload case')
        operation, data, parity, size = parts[:4]
        data, parity, size = int(data), int(parity), int(size)
        if profile == 'smoke':
            policies = {
                'encode':('Vec<Vec<u8>>','iteration-generated-source-parity-in-place',0,0),
                'update':('Vec<&Vec<u8>>+Vec<Option<&Vec<u8>>>->Vec<&mut Vec<u8>>','iteration-generated-source-cloned-old-updated-and-parity',0,0),
                'verify':('Vec<Vec<u8>>','iteration-generated-source-internal-scratch-untracked',0,0),
                'reconstruct':('Vec<Option<Vec<u8>>>','iteration-generated-source-moved-options-drop-missing-new-output-untracked',1 | (1 << data),(1 << (data+parity))-1),
                'reconstruct_data':('Vec<Option<Vec<u8>>>','iteration-generated-source-moved-options-drop-missing-new-output-untracked',3,(1 << data)-1),
            }
            if operation not in policies: raise ValueError('unknown smoke workload')
            contract = dict(zip(('call_entry','output_policy','missing_mask','required_mask'),policies[operation]))
            contract['changed_mask'] = int(operation == 'update')
            seed_op = operation
            lifetime, construction, generator = 'iteration','codec-before-iteration-source','per-iteration-seeded-SmallRng-random-u8'
        elif profile in ('small_file','stream'):
            contract = dict(fixture_contract(profile,case), changed_mask=0)
            seed_op = operation.removesuffix('_stream')
            seed_op = {'verify_with_buffer':'verify','reconstruct_opt':'reconstruct','reconstruct_shard_slot':'reconstruct','reconstruct_some_data_only':'reconstruct_data'}.get(seed_op,seed_op)
            lifetime, construction, generator = 'worker','source-before-codec','worker-seeded-SmallRng-random-u8'
        else: raise ValueError('unknown workload profile')
        seed = case_seed(seed_op,data,parity,size)
    return dict(contract, codec_lifetime='worker', source_lifetime=lifetime, construction=construction, generator=generator, seed=seed)


def validate_workload(record, profile):
    expected = workload_contract(profile,record['case'])
    workload = record.get('workload')
    if not isinstance(workload,dict) or set(workload) != set(expected) | {'rng'} or any(type(workload.get(k)) is not type(v) or workload[k] != v for k,v in expected.items()):
        raise ValueError('missing or incompatible API/seed/preparation workload contract')
    if profile != 'hotspot':
        if workload['rng'] is not None: raise ValueError('unexpected dynamic random stream')
        if record.get('rows') and any(type(row.get('seed')) is not int or row['seed'] != expected['seed'] for row in record['rows']):
            raise ValueError('raw row seed differs from the actual workload')
        return
    if record.get('rows'):
        data, parity, name, missing, required = hotspot_case(record['case'])
        useful = tuple(i for i in missing if i < data) if required is None else required
        pattern = lambda values:'|'.join(('d'+str(i)) if i<data else ('p'+str(i-data)) for i in values)
        wanted = {'data_shards':data,'parity_shards':parity,'shard_size':1048576,'useful_shards':len(useful),
                  'baseline_operation':'reconstruct' if required is None else 'reconstruct_data',
                  'candidate_operation':'reconstruct_data' if required is None else 'reconstruct_some',
                  'missing_pattern':pattern(missing),'required_pattern':pattern(useful)}
        if any(type(row.get(k)) is not type(v) or row[k] != v for row in record['rows'] for k,v in wanted.items()):
            raise ValueError('hotspot raw row differs from the actual API/mask workload')
    count, index = record.get('iterations'), record.get('index')
    if type(count) is not int or count <= 0 or type(index) is not int or not 0 <= index < 12:
        raise ValueError('invalid random stream observation')
    data, parity, *_ = hotspot_case(record['case'])
    step = count*(data+parity)*1048576
    trace = workload['rng']
    if not isinstance(trace,dict) or set(trace) != {'bytes_before','bytes_after','next_before','next_after'}:
        raise ValueError('missing random stream state witness')
    if any(type(trace.get(k)) is not int or trace[k] != value for k,value in (('bytes_before',index*step),('bytes_after',(index+1)*step))):
        raise ValueError('random input consumption differs from the original iteration workload')
    for key in ('next_before','next_after'):
        if not isinstance(trace[key],list) or len(trace[key]) != 2 or any(type(v) is not int or not 0 <= v < 2**64 for v in trace[key]):
            raise ValueError('invalid random stream preview')


def validate_workload_sequence(records, profile):
    for record in records: validate_workload(record,profile)
    if profile == 'hotspot':
        for before,after in zip(records,records[1:]):
            if before['workload']['rng']['next_after'] != after['workload']['rng']['next_before']:
                raise ValueError('random stream was reset or changed inside a worker')


def workload_catalog(records, *, calibration=False):
    selected = {}
    for record in records:
        if record['index'] == 0: selected[record['case']] = record
    return {case:{k:v for k,v in row['workload'].items() if k != 'rng'} | {'initial_rng':row['workload']['rng']['next_before'] if row['workload']['rng'] else None, 'placement':row['placement']}
            for case,row in selected.items()}


def workload_traces(records, *, calibration=False):
    if calibration:
        last = {r['case']:r['calibration_batch'] for r in records}
        records = [r for r in records if r['calibration_batch'] == last[r['case']]]
    return {case:[r['workload']['rng'] for r in records if r['case'] == case]
            for case in {r['case'] for r in records} if any(r['case'] == case and r['workload']['rng'] is not None for r in records)}


def fixture_contract(profile, case):
    if profile not in ("small_file", "stream"): return None
    parts = case.split(":")
    if len(parts) != (4 if profile == "small_file" else 5):
        raise ValueError("invalid caller-fixture case")
    operation, data, parity, size = parts[:4]
    data, parity, size = int(data), int(parity), int(size)
    if not 1 <= data < data+parity <= 32 or size <= 0:
        raise ValueError("invalid caller-fixture shape")
    all_mask = (1 << (data+parity))-1
    contracts = {
        "encode": ("Vec<Vec<u8>>", "caller-parity-in-place", 0, 0),
        "verify": ("Vec<Vec<u8>>", "internal-scratch-untracked", 0, 0),
        "verify_with_buffer": ("Vec<Vec<u8>>", "retained-caller-scratch", 0, 0),
        "reconstruct": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), all_mask),
        "reconstruct_opt": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), all_mask),
        "reconstruct_shard_slot": ("Vec<ShardSlot<Vec<u8>>>", "iteration-clones-retain-missing-storage-untracked", 1 | (1 << data), all_mask),
        "reconstruct_some_data_only": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 1 | (1 << data), 1),
        "reconstruct_data": ("Vec<Option<Vec<u8>>>", "iteration-clones-drop-missing-new-output-untracked", 3, (1 << data)-1),
        "encode_stream": ("Vec<&[u8]> -> Vec<Vec<u8>>", "iteration-reserved-writers-untracked", 0, 0),
        "verify_stream": ("Vec<&[u8]>", "iteration-readers-internal-scratch-untracked", 0, 0),
        "reconstruct_stream": ("Vec<Cursor<Vec<u8>>>", "iteration-cloned-cursors-empty-missing-untracked", 1 | (1 << data), all_mask),
    }
    allowed = dict(SMALL_KERNELS if profile == "small_file" else STREAM_KERNELS).values()
    if operation not in allowed: raise ValueError("unexpected caller-fixture operation")
    return dict(zip(("call_entry", "output_policy", "missing_mask", "required_mask"), contracts[operation]))


def original_loop(text, pattern):
    """Extract exactly one rustfmt-shaped loop, never copy one-time setup."""
    marker = "        "+pattern+" => {"
    if text.count(marker) != 1: raise ValueError("benchmark operation arm changed")
    arm = text[text.index(marker):]
    arm = arm[:re.search(r"(?m)^        }", arm).start()]
    match = re.search(r"(?ms)^            for _ in 0\.\.iterations \{.*?^            }", arm)
    if match is None or arm.count("for _ in 0..iterations {") != 1:
        raise ValueError("benchmark operation loop changed")
    return match.group()


def prepared_kernel(text, profile):
    arms = []
    descriptor = []
    kernels = SMALL_KERNELS if profile == "small_file" else STREAM_KERNELS
    for pattern, operation in kernels:
        loop = original_loop(text, pattern)
        replacements = {}
        if profile == "small_file":
            if operation == "encode": replacements = {"rs.encode(&mut shards)": "rs.encode(&mut *original)"}
            elif operation == "verify": replacements = {"rs.verify(&shards)": "rs.verify(&*original)"}
            elif operation == "verify_with_buffer": replacements = {"rs.verify_with_buffer(&shards, &mut buffer)": "rs.verify_with_buffer(&*original, &mut *scratch)"}
            elif operation == "reconstruct_shard_slot": replacements = {"shards_to_slots(&original)": "shards_to_slots(original)"}
            elif operation == "reconstruct_some_data_only": replacements = {"&required)": "required)"}
        else:
            if operation == "encode_stream": replacements = {"data.iter()": "original.iter()"}
            else: replacements = {"shards.iter()": "original.iter()"}
            if operation == "reconstruct_stream": replacements["&shards[0]"] = "&original[0]"
        for before, after in replacements.items():
            if loop.count(before) != 1: raise ValueError("benchmark operand layout changed")
            loop = loop.replace(before, after, 1)
        # Existing buffered verify observes its boolean result. Apply the same
        # explicit correctness oracle to ordinary verify; no call is removed.
        if operation in ("verify", "verify_with_buffer"):
            old = "rs."+operation+"(&*original"+("" if operation == "verify" else ", &mut *scratch")+").unwrap();"
            if loop.count(old) != 1: raise ValueError("benchmark verification oracle changed")
            loop = loop.replace(old, "assert!(std::hint::black_box("+old[:-1]+"));")
        arms.append("        "+pattern+" => {\n"+loop+"\n        }")
        contract = fixture_contract(profile, operation+":4:2:1024"+(":1024" if profile == "stream" else ""))
        missing = "0" if contract["missing_mask"] == 0 else ("3" if operation == "reconstruct_data" else "1 | (1 << case.data_shards)")
        required = "0" if contract["required_mask"] == 0 else ("1" if operation == "reconstruct_some_data_only" else "(1 << case.data_shards)-1" if operation == "reconstruct_data" else "(1 << (case.data_shards+case.parity_shards))-1")
        descriptor.append(f'        {pattern} => ("{contract["call_entry"]}", "{contract["output_policy"]}", {missing}, {required}),')
    if profile == "small_file":
        signature = "prepared: &mut PreparedSmallFile, iterations: usize"
        bindings = "    let case = prepared.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let scratch = &mut prepared.scratch; let required = &prepared.required;\n"
        arms.append('        _ => panic!("unsupported small-file release operation"),')
        descriptor.append('        _ => panic!("unsupported small-file release operation"),')
        contract_signature = "case: BenchCase, operation: SmallFileOp"
    else:
        signature = "prepared: &mut PreparedStream, iterations: usize"
        bindings = "    let case = prepared.stream_case.case; let operation = prepared.operation; let rs = &prepared.codec; let original = &mut prepared.source; let options = &prepared.options;\n"
        # The original code already takes &options; the generated parameter is
        # a reference, so remove this extra borrow explicitly.
        arms = [arm.replace("&options", "options") for arm in arms]
        contract_signature = "case: BenchCase, operation: StreamOperation"
    return "\nfn execute_release_kernel("+signature+") -> std::time::Duration {\n"+bindings+"    let start = Instant::now();\n    match operation {\n"+"\n".join(arms)+"\n    }\n    start.elapsed()\n}\n"+"fn release_contract("+contract_signature+") -> (&'static str, &'static str, u64, u64) {\n    match operation {\n"+"\n".join(descriptor)+"\n    }\n}\n"


def smoke_kernel(text):
    original = block(text,'fn run_operation')
    signature = 'fn run_operation(case: BenchCase, operation: Operation, iterations: usize) -> SmokeResult'
    constructor = '    let rs = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();\n'
    if original.count(signature) != 1 or original.count(constructor) != 1: raise ValueError('smoke preparation changed')
    kernel = original.replace(signature, 'fn execute_smoke_kernel(case: BenchCase, operation: Operation, iterations: usize, rs: &ReedSolomon) -> SmokeResult',1).replace(constructor,'',1)
    kernel = kernel.replace('rs.verify(&shards).unwrap();','assert!(std::hint::black_box(rs.verify(&shards).unwrap()));',1)
    restored = kernel.replace('fn execute_smoke_kernel(case: BenchCase, operation: Operation, iterations: usize, rs: &ReedSolomon) -> SmokeResult',signature,1).replace('    let bytes =',constructor+'    let bytes =',1).replace('assert!(std::hint::black_box(rs.verify(&shards).unwrap()));','rs.verify(&shards).unwrap();',1)
    if restored != original: raise ValueError('smoke adapter changed original timed statements')
    wrapper = signature+' {\n'+constructor+'    execute_smoke_kernel(case, operation, iterations, &rs)\n}'
    arms=[]
    for op in ('encode','update','verify','reconstruct','reconstruct_data'):
        contract=workload_contract('smoke',op+':4:2:1024')
        missing='1 | (1 << case.data_shards)' if op=='reconstruct' else '3' if op=='reconstruct_data' else '0'
        required='(1 << (case.data_shards+case.parity_shards))-1' if op=='reconstruct' else '(1 << case.data_shards)-1' if op=='reconstruct_data' else '0'
        variant={'reconstruct_data':'ReconstructData'}.get(op,op.title())
        arms.append(f'Operation::{variant} => ("{contract["call_entry"]}","{contract["output_policy"]}",{missing},{required}),')
    contract='fn smoke_contract(case: BenchCase, operation: Operation) -> (&\'static str,&\'static str,u64,u64) { match operation {'+''.join(arms)+'_ => panic!("unsupported smoke release case"), } }'
    return text.replace(original,wrapper,1)+'\n'+kernel+'\n'+contract


def hotspot_kernel(text):
    macro = block(text,'macro_rules! make_random_shards')
    updated = macro.replace('($per_shard:expr, $size:expr)', '($rng:expr, $per_shard:expr, $size:expr)').replace('fill_random(s);','fill_random(&mut *$rng, s);')
    if updated == macro: raise ValueError('hotspot random allocation constructor changed')
    text = text.replace(macro,updated,1)
    fill = block(text,'pub fn fill_random')
    text = text.replace(fill,'fn fill_random(rng: &mut super::workload::InputRng, arr: &mut [u8]) { rng.fill(arr); }',1)
    for name in ('data','some'):
        fn = 'fn bench_reconstruct_'+name+'_hotspot'
        original=block(text,fn)
        original_loop_text = re.search(r'(?ms)^    for _ in 0\.\.iterations \{.*?^    }',original).group()
        body=original[original.index('    let r = '):]
        ctor='    let r = ReedSolomon::new(data_shards, parity_shards).unwrap();'
        body=body.replace(ctor, '    let case = prepared.case; let data_shards=case.data; let parity_shards=case.parity; let shard_size=prepared.shard_size; let scenario=case.name; let missing_indices=case.missing; '+('let required_indices=case.required.unwrap_or(&[]); ' if name=='some' else '')+'let r=&prepared.codec; let rng=&mut prepared.rng;',1)
        body=body.replace('    let iterations = benchmark_test_iterations();\n','',1)
        if name=='data':
            start=body.index('    let missing_data_indices:');end=body.index('    let useful_shards',start)
            body=body[:start]+'    let missing_data_indices=&prepared.missing_data;\n'+body[end:]
            body=body.replace('for &index in &missing_data_indices','for &index in missing_data_indices').replace('shard_index_pattern(data_shards, &missing_data_indices)','shard_index_pattern(data_shards, missing_data_indices)')
        else:
            start=body.index('    let mut required =');end=body.index('    let mut baseline_total',start)
            body=body[:start]+'    let required=&prepared.required;\n\n'+body[end:]
            body=body.replace('&required)', 'required)')
        old='make_random_shards!(shard_size, data_shards + parity_shards)'
        if body.count(old)!=1: raise ValueError('hotspot per-iteration preparation changed')
        body=body.replace(old,'make_random_shards!(rng, shard_size, data_shards + parity_shards)',1)
        loop_text = re.search(r'(?ms)^    for _ in 0\.\.iterations \{.*?^    }',body).group()
        restored = loop_text.replace('make_random_shards!(rng, ','make_random_shards!(').replace('for &index in missing_data_indices','for &index in &missing_data_indices').replace('reconstruct_some(&mut candidate, required)','reconstruct_some(&mut candidate, &required)')
        if restored != original_loop_text: raise ValueError('hotspot adapter changed original timed iteration statements')
        text=text.replace(original,fn+'(prepared: &mut PreparedHotspot, iterations: usize) -> ReconstructionHotspotBenchResult {\n'+body,1)
    return text


def render_hotspot_cases():
    def indices(values): return '&['+','.join(map(str,values))+']'
    return ',\n'.join('HotspotCase {data:'+str(data)+',parity:'+str(parity)+',name:"'+name+'",missing:'+indices(missing)+',required:'+('None' if required is None else 'Some('+indices(required)+')')+'}' for data,parity,name,missing,required in HOTSPOT_CASES)


def render(root):
    rendered = {name: (TEMPLATES / name).read_text() for name in ("engine.rs", "process.rs", "layout.rs", "pattern.rs", "workload.rs", "service_window.rs")}
    sources = {"smoke": "benchmark_smoke.rs", "small_file": "benchmark_small_files.rs", "stream": "benchmark_stream_paths.rs"}
    for profile, filename in sources.items():
        text = (root / "tests" / filename).read_text()
        shared_modules = '#[path = "../benches/common/mod.rs"]\nmod bench_common;\nmod common;'
        if text.count(shared_modules) != 1:
            raise ValueError("benchmark common-module layout changed")
        text = text.replace(shared_modules, 'use super::{bench_common, common};', 1)
        name = {"smoke": "fn smoke_iterations", "small_file": "fn small_file_iterations", "stream": "fn stream_iterations"}[profile]
        text = text.replace(block(text, name), name + "() -> usize { super::sampling::iterations() }")
        if profile in ("small_file", "stream"):
            text = exclude_once_only_setup(text, "fn run_operation" if profile == "small_file" else "fn run_stream_operation", 11 if profile == "small_file" else 3)
        if profile == "stream": text = reserve_stream_outputs(text)
        if profile == "smoke": text = smoke_kernel(text)
        rendered[profile + ".rs"] = text + "\n" + (TEMPLATES / (profile + ".rs")).read_text()
        if profile in ("small_file", "stream"): rendered[profile+".rs"] += prepared_kernel(text, profile)
    original = (root / "src/tests/mod.rs").read_text()
    parts = [block(original, name) for name in HOTSPOT_BLOCKS]
    text = '\n'.join(parts)
    if text.count(": String,") != 2:
        raise ValueError("hotspot result metadata changed; review the bounded adapter")
    text = text.replace(": String,", ": super::pattern::ShardPattern,")
    original_pattern = block(text, "fn shard_index_pattern")
    text = text.replace(original_pattern, "fn shard_index_pattern(data_shards: usize, indices: &[usize]) -> super::pattern::ShardPattern { super::pattern::ShardPattern::new(data_shards, indices) }", 1)
    text = text.replace(block(text, "fn benchmark_test_iterations"), 'fn benchmark_test_iterations() -> usize { super::sampling::iterations() }')
    text = hotspot_kernel(text)
    prefix = 'use std::{fs, path::PathBuf, time::Instant};\nuse rustfs_erasure_codec::galois_8::ReedSolomon;\nfn benchmark_metrics_enabled() -> bool { cfg!(feature="benchmark-metrics") }\nconst BENCHMARK_ARTIFACT_SCHEMA_VERSION: u32 = 1;\n'
    rendered["hotspot.rs"] = prefix + text + "\n" + (TEMPLATES / "hotspot.rs").read_text().replace("__HOTSPOT_CASES__",render_hotspot_cases())
    root_source = f'#![allow(dead_code)]\n#[path="{root}/benches/common/mod.rs"] mod bench_common;\n#[path="{root}/tests/common/mod.rs"] mod common;\n' + '\n'.join(f'#[path="rse_release_sampling/{name}.rs"] mod {name};' for name in ("engine", "process", "layout", "pattern", "workload", *sources, "hotspot"))
    root_source = root_source.replace('mod engine;', 'mod sampling;')
    root_source += '\n#[test]\n#[ignore]\nfn release_controlled_profile() { match std::env::var("RSE_SAMPLER_PROFILE").unwrap().as_str() {'
    root_source += ','.join(f'"{name}" => {name}::release_profile()' for name in (*sources, "hotspot"))
    root_source += ', _ => panic!("unknown release profile") } }\n'
    rendered["root.rs"] = root_source
    return rendered


def overlay_digest(root):
    return hashlib.sha256(b"".join(name.encode() + b"\0" + text.replace(str(root), "<SOURCE>").encode()
                                  for name, text in sorted(render(root).items()))).hexdigest()


def generate(root):
    directory = root / "tests/rse_release_sampling"
    entry = root / "tests/rse_release_sampling.rs"
    if directory.exists() or entry.exists():
        raise ValueError("refuse to replace an existing sampling overlay")
    directory.mkdir()
    for name, text in render(root).items():
        (entry if name == "root.rs" else directory / name).write_text(text)


def parse_output(output, prefix="RSE_SAMPLE "):
    """Keep completed journal records; ignore only one interrupted final fragment."""
    records = []
    lines = output.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if not line.startswith(prefix): continue
        try:
            records.append(json.loads(line.removeprefix(prefix)))
        except json.JSONDecodeError:
            if index != len(lines)-1 or line.endswith('\n'): raise
    return records


def valid_page_size(value):
    return type(value) is int and 1024 <= value <= 1048576 and value & (value-1) == 0


def base_page_size():
    value = os.sysconf('SC_PAGE_SIZE')
    if not valid_page_size(value): raise ValueError("invalid OS base page size")
    return value


def parse_log(path):
    return parse_output(path.read_text())


def validate_placement(record, allowed_cpus=None):
    placement = record.get('placement')
    if not isinstance(placement,dict) or set(placement) != {'cpu','constructor_budget','constructor_affinity','pool_threads','affinity_checked'}:
        raise ValueError('actual measuring-thread placement is required')
    if any(type(placement.get(k)) is not int or placement[k] < (0 if k == 'cpu' else 1) for k in ('cpu','constructor_budget','pool_threads')) or placement.get('pool_threads') != 4 or placement.get('affinity_checked') is not True:
        raise ValueError('invalid prepared codec/pool placement')
    mask = placement.get('constructor_affinity')
    if not isinstance(mask,list) or not 1 <= len(mask) <= 65536 or any(type(cpu) is not int or cpu < 0 for cpu in mask) or mask != sorted(set(mask)) or placement['cpu'] not in mask or placement['constructor_budget'] > len(mask):
        raise ValueError('invalid actual constructor/pool CPU mask')
    if allowed_cpus is not None and (placement['cpu'] != min(allowed_cpus) or mask != sorted(allowed_cpus)):
        raise ValueError('measuring-thread placement differs from the reservation')
    if any(not isinstance(record.get(k),dict) or record[k].get('cpu') != placement['cpu'] for k in ('before','after')):
        raise ValueError('sample CPU differs from the fixed measuring thread')


def worker_environment(environment):
    result = dict(environment)
    if hasattr(os,'sched_getaffinity'):
        result['RSE_SAMPLER_CPU'] = str(min(os.sched_getaffinity(0)))
    return result


def validate_telemetry(record, allowed_cpus=None):
    before, after = record.get("before"), record.get("after")
    for point in (before, after):
        if not isinstance(point, dict) or set(point) != {"cpu", "run_ns", "wait_ns", "switches", "process_minor_faults", "process_major_faults"}:
            raise ValueError("per-sample scheduling telemetry is required")
        if any(type(value) is not int or value < 0 for value in point.values()):
            raise ValueError("invalid scheduling counters")
        if allowed_cpus is not None and point["cpu"] not in allowed_cpus:
            raise ValueError("sample escaped the reserved CPU set")
    if any(after[key] < before[key] for key in ("run_ns", "wait_ns", "switches", "process_minor_faults", "process_major_faults")):
        raise ValueError("scheduling counters moved backwards")


def validate_fixture(record, profile, expected_page_size=None):
    """Require the complete retained caller contract; dynamic outputs stay explicit."""
    fixture = record.get("fixture")
    contract = fixture_contract(profile, record["case"])
    tracked = contract is not None
    if not tracked:
        if fixture is not None: raise ValueError("unexpected fixture layout declaration")
        return
    if not isinstance(fixture, dict) or set(fixture) != {"entry", "lifetime", "page_size", "source", "scratch", *contract} or fixture.get("entry") != "Vec<Vec<u8>>" or fixture.get("lifetime") != "worker":
        raise ValueError("caller storage requires a retained Vec fixture layout")
    if any(type(fixture.get(key)) is not type(value) or fixture[key] != value for key, value in contract.items()):
        raise ValueError("API entry or destination allocation contract differs")
    page_size = fixture.get("page_size")
    if not valid_page_size(page_size) or expected_page_size is not None and page_size != expected_page_size:
        raise ValueError("fixture page size differs from the recorded runtime")
    operation, data, parity, size = record["case"].split(":")[:4]
    data, parity, size = int(data), int(parity), int(size)
    source_count = data if operation == "encode_stream" else data+parity
    scratch_count = parity if operation == "verify_with_buffer" else 0
    for role, count in (("source", source_count), ("scratch", scratch_count)):
        positions = fixture.get(role)
        if not isinstance(positions, list) or len(positions) != count:
            raise ValueError("incomplete fixture buffer layout")
        for position in positions:
            if not isinstance(position, dict) or set(position) != {"len", "capacity", "offset_64", "offset_4096", "offset_page"} or any(type(v) is not int for v in position.values()):
                raise ValueError("invalid fixture buffer position")
            if position["len"] != size or not size <= position["capacity"] <= 2**64 - 1 or not 0 <= position["offset_4096"] < 4096 or not 0 <= position["offset_64"] < 64 or position["offset_4096"] % 64 != position["offset_64"] or not 0 <= position["offset_page"] < page_size or position["offset_page"] % min(page_size, 4096) != position["offset_4096"] % min(page_size, 4096):
                raise ValueError("invalid fixture shape or offset")


def fixture_catalog(records, *, calibration=False):
    """Extract already-validated fixed layouts, never infer them from timing."""
    return {r["case"]: r["fixture"] for r in records
            if r.get("fixture") is not None and
            (r.get("complete") is True if calibration else r.get("kind") == "sample")}


def validate_timing(record, profile, count):
    row = record["rows"][0]
    for field in ("wall_ns", "timed_ns"):
        value = record.get(field)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
            raise ValueError("invalid measured interval")
    if profile == "hotspot":
        expected_timed = row["useful_shards"] * row["shard_size"] * 1e9 * count / (1048576.0 * row["candidate_mb_s"])
        tolerance = expected_timed * .00001 + 1
    else:
        expected_timed = row["ns_per_iter"] * count
        tolerance = .006 * count + expected_timed * .00001
    if abs(expected_timed - record["timed_ns"]) > tolerance:
        raise ValueError("timed interval differs from the raw exported measurement")

def calibration_batch(records, count, batch):
    """Replay one complete worker schedule; return the next count, or None."""
    width = PROTOCOL["warmups"] + PROTOCOL["samples"]
    if type(count) is not int or not 1 <= count <= 1_000_000_000 or type(batch) is not int or not 1 <= batch <= PROTOCOL["max_calibration_batches"] or len(records) != width:
        raise ValueError("invalid bounded calibration batch")
    for index, record in enumerate(records):
        kind = "calibration_warmup" if index < PROTOCOL["warmups"] else "calibration"
        if record.get("kind") != kind or any(type(record.get(field)) is not int for field in ("index", "iterations", "calibration_batch")) or record.get("index") != index or record.get("iterations") != count or record.get("calibration_batch") != batch or record.get("isolated_case") is not True:
            raise ValueError("calibration requires the complete fixed warmup and rehearsal schedule")
        for field in ("wall_ns", "timed_ns"):
            value = record.get(field)
            if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
                raise ValueError("invalid calibration interval")
        met = record["wall_ns"] >= PROTOCOL["target_wall_ns"] and record["timed_ns"] >= PROTOCOL["target_timed_ns"]
        if record.get("target_met") is not met:
            raise ValueError("calibration target flag differs from measured intervals")
    if any(r.get("fixture") != records[0].get("fixture") for r in records):
        raise ValueError("fixture layout changed inside a calibration worker")
    probes = records[PROTOCOL["warmups"]:]
    complete = all(r["target_met"] for r in probes)
    if any(r.get("complete") is not (complete and index == width-1) for index, r in enumerate(records)):
        raise ValueError("calibration lacks the complete measurement rehearsal")
    if complete:
        return None
    desired = max(max(PROTOCOL["target_wall_ns"]/r["wall_ns"], PROTOCOL["target_timed_ns"]/r["timed_ns"]) for r in probes)*1.25
    following = count * math.ceil(min(64, max(2, desired)))
    if following > 1_000_000_000:
        raise ValueError("calibration iteration budget exceeded")
    return following


def build_environment(root, environment):
    """Keep generated build outputs with their source, even under a shared caller cache."""
    return dict(environment, CARGO_TARGET_DIR=str(root.resolve() / 'target'))


def run_profile(root, environment, profile, mode, log_path, counts_file=None, *, cases, journal=None):
    import subprocess
    ordered = sorted(cases)
    if mode not in ("calibrate", "sample") or not ordered or len(ordered) != len(set(ordered)):
        raise ValueError("a valid mode and nonempty unique case catalog are required")
    env = dict(build_environment(root, worker_environment(environment)), RSE_SAMPLER_PROFILE=profile, RSE_SAMPLER_MODE=mode, RSE_SAMPLER_PAGE_SIZE=str(base_page_size()))
    counts = {}
    if mode == "sample":
        if counts_file is None: raise ValueError("frozen counts are required")
        for line in counts_file.read_text().splitlines():
            name, value = line.split("\t")
            if name in counts: raise ValueError("duplicate frozen case count")
            count = int(value)
            if not 1 <= count <= 1_000_000_000: raise ValueError("invalid frozen case count")
            counts[name] = count
        if set(counts) != set(ordered): raise ValueError("complete frozen case counts are required")
    command = ["cargo", "test", "--locked", "--release", "--features", "std simd-accel", "--test", "rse_release_sampling", "release_controlled_profile", "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    records = []
    with log_path.open("w") as log:
        for case in ordered:
            count, batch = (1, 1) if mode == "calibrate" else (counts[case], 0)
            while True:
                worker = dict(env, RSE_SAMPLER_CASE=case, RSE_SAMPLER_ITERATIONS=str(count), RSE_SAMPLER_BATCH=str(batch))
                offset = log.tell()
                if journal:
                    identity = {'case': case, 'mode': mode, 'iterations': count, 'calibration_batch': batch}
                    journal(dict(identity, event='start', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns()))
                try:
                    subprocess.run(command, cwd=root, env=worker, stdout=log, stderr=subprocess.STDOUT, check=True)
                except BaseException as error:
                    if journal:
                        journal(dict(identity, event='end', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns(),
                                     exit_code=getattr(error, 'returncode', None), error=type(error).__name__))
                    raise
                else:
                    if journal:
                        journal(dict(identity, event='end', monotonic_ns=time.monotonic_ns(), utc_ns=time.time_ns(), exit_code=0))
                with log_path.open() as reader:
                    reader.seek(offset)
                    current = parse_output(reader.read())
                if not current or any(r.get("case") != case or r.get("isolated_case") is not True for r in current):
                    raise ValueError("case process did not emit exactly its selected workload")
                records.extend(current)
                if mode != "calibrate": break
                following = calibration_batch(current, count, batch)
                if following is None: break
                count, batch = following, batch+1
                if batch > PROTOCOL["max_calibration_batches"]:
                    raise ValueError("calibration batch budget exceeded; observations preserved")
    return records


def sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus=None, expected_page_size=None):
    groups = {}
    for record in records:
        if record.get("revision") != revision or record.get("case") not in expected:
            raise ValueError("sampling revision or case mismatch")
        groups.setdefault(record["case"], []).append(record)
    if set(groups) != expected:
        raise ValueError("complete sampling catalog is required")
    for case, group in groups.items():
        validate_workload_sequence(group, profile)
        if [r.get("kind") for r in group] != ["warmup"] * PROTOCOL["warmups"] + ["sample"] * PROTOCOL["samples"] or [r.get("index") for r in group] != list(range(PROTOCOL["samples"]+PROTOCOL["warmups"])):
            raise ValueError("every fixed warmup and sample is required")
        for record in group:
            validate_telemetry(record, allowed_cpus)
            validate_placement(record, allowed_cpus)
            validate_fixture(record, profile, expected_page_size)
            if record.get("fixture") != group[0].get("fixture"):
                raise ValueError("fixture layout changed inside a measurement worker")
            if any(type(record.get(field)) is not int for field in ("index", "iterations", "calibration_batch")) or record.get("calibration_batch") != 0 or record.get("isolated_case") is not True or record.get("iterations") != counts[case] or not isinstance(record.get("rows"), list) or len(record["rows"]) != 1:
                raise ValueError("sample count/iterations mismatch")
            row = record["rows"][0]
            if normalize(profile, row, revision) != case:
                raise ValueError("raw sample workload mismatch")
            validate_timing(record, profile, counts[case])
    return groups


def selected_row(records, profile, metric, revision, expected, counts, normalize, allowed_cpus=None, expected_page_size=None):
    groups = sample_groups(records, profile, revision, expected, counts, normalize, allowed_cpus, expected_page_size)
    selected, spread = [], []
    for case, group in groups.items():
        for record in group[PROTOCOL["warmups"]:]:
            for field, minimum in (("wall_ns", PROTOCOL["minimum_wall_ns"]), ("timed_ns", PROTOCOL["minimum_timed_ns"])):
                if record[field] < minimum: raise ValueError("sample measurement interval is insufficient")
        samples = group[PROTOCOL["warmups"]:]
        ordered = sorted(samples, key=lambda r: r["rows"][0][metric])
        median = ordered[len(ordered)//2]["rows"][0]
        width = (ordered[-2]["rows"][0][metric] - ordered[1]["rows"][0][metric]) / median[metric]
        spread.append({"case":case,"central_spread":width})
        selected.append(median)
    return selected, spread


def checkout_baseline(source, destination, revision):
    """Fetch the exact baseline tree even when source is a shallow CI checkout."""
    import subprocess
    subprocess.run(['git','init','--quiet',str(destination)],check=True)
    subprocess.run(['git','fetch','--quiet','--no-tags','--depth=1',str(source),revision],cwd=destination,check=True)
    subprocess.run(['git','checkout','--quiet','--detach',revision],cwd=destination,check=True)


def check(root, workers=False):
    import subprocess
    from diagnostic_overlay import overlay
    with overlay(root, render(root)):
        result = subprocess.run(["cargo","test","--locked","--release","--features","std simd-accel","--test","rse_release_sampling","--","--skip","release_controlled_profile","--nocapture"],
                                cwd=root,env=build_environment(root,dict(os.environ,RUSTUP_TOOLCHAIN="stable")),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        print(result.stdout, end='')
        result.check_returncode()
        actual = {}
        if workers:
            from release_evidence import SAMPLING_ENV, expected_cases
            for profile in ('smoke','hotspot'):
                for case in sorted(expected_cases(profile)):
                    env=dict(build_environment(root,worker_environment(dict(os.environ,**SAMPLING_ENV))),RSE_SAMPLER_PROFILE=profile,RSE_SAMPLER_MODE='sample',RSE_SAMPLER_CASE=case,RSE_SAMPLER_ITERATIONS='1',RSE_SAMPLER_BATCH='0',RSE_SAMPLER_PAGE_SIZE=str(base_page_size()))
                    process=subprocess.run(['cargo','test','--locked','--release','--features','std simd-accel','--test','rse_release_sampling','release_controlled_profile','--','--exact','--ignored','--nocapture','--test-threads=1'],cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
                    if process.returncode:
                        print(process.stdout,end='');process.check_returncode()
                    rows=parse_output(process.stdout)
                    if len(rows)!=12 or [r['index'] for r in rows]!=list(range(12)) or any(r['case'] != case or r['iterations'] != 1 for r in rows):
                        raise ValueError('complete one-iteration worker output is required')
                    validate_workload_sequence(rows,profile)
                    if hasattr(os,'sched_getaffinity'):
                        for row in rows:validate_placement(row,os.sched_getaffinity(0))
                    actual[case]=[{'workload':r['workload'],'placement':r['placement']} for r in rows]
            print('All 23 smoke/hotspot workers executed at one iteration; input/preparation correctness only')
    records = parse_output(result.stdout,'RSE_INPUT_REPLAY ')
    if len(records) != len(HOTSPOT_CASES) or {r['case'] for r in records} != {row[2] for row in HOTSPOT_CASES}:
        raise ValueError('complete native input replay witnesses are required')
    if workers and hasattr(os,'sched_getaffinity'):
        placements={json.dumps(row['placement'],sort_keys=True) for rows in actual.values() for row in rows}
        if len(placements) != 1: raise ValueError('native workers changed their constructor/pool placement')
        print('RSE_PLACEMENT_CHECK '+next(iter(placements)))
    return {'prefixes':sorted(records,key=lambda row:row['case']),'workers':actual}


if __name__ == "__main__":
    import argparse
    import subprocess
    import tempfile
    parser=argparse.ArgumentParser(description="Check actual shared workers and optional published-baseline input replay; never performance acceptance")
    parser.add_argument("--check",action="store_true",required=True)
    parser.add_argument("--check-workers",action="store_true",help="Execute the 23 changed workers at one iteration, never performance acceptance")
    parser.add_argument("--baseline-ref",help="Full published 9.0.0 source commit, already available locally")
    args=parser.parse_args()
    root=TEMPLATES.parent.parent
    current=check(root,args.check_workers)
    if args.baseline_ref:
        from release_evidence import BASELINE_COMMIT
        if args.baseline_ref != BASELINE_COMMIT: parser.error('baseline-ref must be the published 9.0.0 commit')
        with tempfile.TemporaryDirectory(prefix='rse-input-replay-') as temporary:
            baseline=pathlib.Path(temporary)/'baseline'
            checkout_baseline(root,baseline,BASELINE_COMMIT)
            previous=check(baseline,args.check_workers)
        if current != previous: raise ValueError('published and candidate input replay witnesses differ')
        print('Published 9.0.0/candidate input replay matches for all eight hotspot scenarios; correctness only')
