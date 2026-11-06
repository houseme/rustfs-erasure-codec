import hashlib, json, os, pathlib, signal, subprocess, time
root = pathlib.Path(os.environ['RSE_BENCH_ROOT'])
out = root / 'native-balanced-v5-avx512-large-20261106-01'
out.mkdir(exist_ok=False)
ticks = os.sysconf('SC_CLK_TCK')
cpu = 8
cases = {
    'gf16_slot': ('binaries-v5-native/packed_recovery', r'small_leopard_recovery/slots/gf16/10x4/1048576/missing4$'),
    'gf16_option': ('binaries-v5-native/packed_recovery', r'small_leopard_recovery/option/gf16/10x4/1048576/missing4$'),
}
backends = ('rust-gfni-avx512',)
inputs = [root / path / role for path, _ in cases.values() for role in ('baseline', 'candidate')]
receipts = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
(out / 'build-receipts.json').write_text(json.dumps(receipts, indent=2))

def snapshot(exclude):
    seen = {}
    for p in pathlib.Path('/proc').iterdir():
        if not p.name.isdigit() or int(p.name) in exclude:
            continue
        try:
            f = (p / 'stat').read_text().rsplit(') ', 1)[1].split()
            seen[int(p.name)] = (f[19], int(f[11]) + int(f[12]))
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
    return seen

def interrupt(signum, frame):
    raise InterruptedError('signal ' + str(signum))

for sig in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
    signal.signal(sig, interrupt)
child = None
try:
    for case, (binary_dir, pattern) in cases.items():
        for backend in backends:
            for block in range(4):
                # Reverse both within-block order and control/candidate order.
                roles = ('A', 'B', 'B', 'A') if block % 2 == 0 else ('B', 'A', 'A', 'B')
                modes = ('same', 'candidate') if block % 2 == 0 else ('candidate', 'same')
                for mode in modes:
                    for phase, role in enumerate(roles):
                        chosen = 'baseline' if role == 'A' or mode == 'same' else 'candidate'
                        stage = out / case / backend / mode / ('block' + str(block)) / ('phase' + str(phase) + '-' + role)
                        stage.mkdir(parents=True, exist_ok=False)
                        env = os.environ.copy()
                        env.update(RSE_BACKEND_OVERRIDE=backend, RSE_EXPECT_BACKEND=backend,
                                   RAYON_NUM_THREADS='1', CRITERION_HOME=str(stage / 'criterion'))
                        argv = [str(root / binary_dir / chosen), '--bench', '--noplot', '--sample-size', '40',
                                '--warm-up-time', '1.0', '--measurement-time', '3.0', pattern]
                        def pin():
                            os.setsid()
                            os.sched_setaffinity(0, {cpu})
                        with (stage / 'stdout.log').open('w') as stdout, (stage / 'stderr.log').open('w') as stderr, (stage / 'monitor.jsonl').open('w') as journal:
                            child = subprocess.Popen(argv, cwd=root, env=env, stdout=stdout, stderr=stderr, preexec_fn=pin)
                            previous = time.monotonic()
                            old = snapshot({os.getpid(), child.pid})
                            while child.poll() is None:
                                time.sleep(1)
                                now = time.monotonic()
                                current = snapshot({os.getpid(), child.pid})
                                busy = [pid for pid, (start, value) in current.items()
                                        if pid in old and old[pid][0] == start and value - old[pid][1] > ticks * (now - previous) * 0.5]
                                journal.write(json.dumps({'elapsed': round(now - previous, 3), 'competing_count': len(busy)}) + '\n')
                                journal.flush()
                                if busy:
                                    raise RuntimeError('competing CPU-intensive process detected')
                                previous, old = now, current
                            if child.returncode:
                                raise RuntimeError('benchmark failed: ' + str(child.returncode))
                            child = None
                print('DONE', case, backend, 'block', block, flush=True)
    for path, expected in receipts.items():
        assert hashlib.sha256((root / path).read_bytes()).hexdigest() == expected
    (out / 'COMPLETE').write_text('completed all phases and verified binary receipts\n')
    print('COMPLETE', flush=True)
finally:
    if child is not None and child.poll() is None:
        os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
