#!/usr/bin/env python3
"""Compile the real sampler with a separate, test-only allocation counter."""
import os
import pathlib
import subprocess
from contextlib import contextmanager

ROOT = pathlib.Path(__file__).resolve().parents[1]


@contextmanager
def probe(root):
    target = root/'tests/rse_sampler_telemetry_allocations.rs'
    if target.exists(): raise ValueError('refuse to overwrite an allocation probe')
    modules = ''.join(f'#[path="{root}/scripts/release-sampling/{name}.rs"] mod {name};\n'
                      for name in ('engine', 'layout', 'process')).replace('mod engine;', 'mod sampling;')
    text = (root/'scripts/diagnostics/telemetry_allocations.rs').read_text()+'\n'+modules
    with target.open('x') as file: file.write(text)
    try:
        yield
    finally:
        if target.read_text() != text: raise ValueError('changed allocation probe; preserve it')
        target.unlink()


if __name__ == '__main__':
    with probe(ROOT):
        subprocess.run(['cargo', 'test', '--locked', '--release', '--features', 'std simd-accel',
                        '--test', 'rse_sampler_telemetry_allocations', '--', '--nocapture'],
                       cwd=ROOT, env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'), check=True)
