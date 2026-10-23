"""Own exactly one generated diagnostic overlay; preserve unexpected inputs."""
from contextlib import contextmanager
import native_tools


@contextmanager
def source_overlay(root, names, module, variant):
    """Install one native plan and restore only its unchanged owned inputs."""
    if any((root/n).is_symlink() or not (root/n).is_file() for n in names):
        raise ValueError('require regular source-copy inputs')
    added=root/('src/'+module+'.rs')
    if added.exists() or added.is_symlink():raise ValueError('preserve an existing source observer')
    original={n:(root/n).read_text() for n in names}
    plan=native_tools.source_plan(variant, original);changes=plan['files']
    try:
        for name,text in changes.items():(root/name).write_text(text)
        yield plan['bindings']
    finally:
        if any((root/n).is_symlink() or not (root/n).is_file() or (root/n).read_text()!=text for n,text in changes.items()):
            raise ValueError('unexpected diagnostic source change; copy preserved')
        for name,text in original.items():(root/name).write_text(text)
        added.unlink()


@contextmanager
def overlay(repository, files):
    entry, directory = repository/'tests/rse_release_sampling.rs', repository/'tests/rse_release_sampling'
    if entry.exists() or directory.exists():
        raise ValueError('refuse to overwrite a sampling overlay')
    directory.mkdir()
    try:
        for name, value in files.items():
            with (entry if name == 'root.rs' else directory/name).open('x') as target:
                target.write(value)
        yield files
    finally:
        if {p.name for p in directory.iterdir()} != set(files)-{'root.rs'} or any((entry if name == 'root.rs' else directory/name).read_text() != value for name, value in files.items()):
            raise ValueError('unexpected generated input; preserve it for inspection')
        for path in directory.iterdir():
            path.unlink()
        directory.rmdir()
        entry.unlink()
