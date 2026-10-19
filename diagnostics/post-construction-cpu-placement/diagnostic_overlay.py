"""Own exactly one generated diagnostic overlay; preserve unexpected inputs."""
from contextlib import contextmanager


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
