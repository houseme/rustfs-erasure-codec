"""Read-only Linux THP/mTHP provenance outside measured API windows.

A minor-fault event is not a count of base pages. Bind policy rather than trying
to infer page size, physical folios or fault cost from that event counter.
"""
import ctypes
import pathlib
import platform
import re
import stat

ROOT=pathlib.Path('/sys/kernel/mm/transparent_hugepage')
GLOBAL={'always','madvise','never'}
PER_SIZE=GLOBAL|{'inherit'}
DEFRAG=GLOBAL|{'defer','defer+madvise'}
SHMEM={'always','within_size','advise','never','deny','force'}
SHMEM_SIZE={'always','inherit','within_size','advise','never'}
SIZE=re.compile(r'hugepages-([1-9][0-9]{0,12})kB\Z')
MAX_SIZES=64


def _text(path):
    with path.open('rb') as stream:raw=stream.read(4097)
    if len(raw)>4096:raise ValueError('oversized kernel memory policy')
    try:return raw.decode('ascii').strip()
    except UnicodeError as error:raise ValueError('non-ASCII kernel memory policy') from error


def _choice(text,allowed):
    tokens=text.split();selected=[];seen=set()
    for token in tokens:
        chosen=token.startswith('[') and token.endswith(']')
        name=token[1:-1] if chosen else token
        if name not in allowed or name in seen:raise ValueError('invalid kernel memory policy choices')
        seen.add(name)
        if chosen:selected.append(name)
    if len(selected)!=1:raise ValueError('kernel memory policy needs exactly one selected choice')
    return selected[0]


def _process_mode():
    # PR_GET_THP_DISABLE is read-only and inherited across fork/exec. Modes 0,
    # 1 and 3 include modern except-advised behavior; do not flatten them to bool.
    libc=ctypes.CDLL(None,use_errno=True)
    call=libc.prctl
    call.argtypes=[ctypes.c_int,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_ulong]
    call.restype=ctypes.c_int
    result=call(42,0,0,0,0)
    if result<0:
        code=ctypes.get_errno();raise OSError(code,'cannot read process THP-disable mode')
    return result


def validate(value,base_page_size=None):
    if not isinstance(value,dict) or set(value)!={'kernel_release','process_thp_disable','thp'}:
        raise ValueError('complete kernel memory policy provenance is required')
    release=value['kernel_release']
    if not isinstance(release,str) or not release or len(release)>256 or any(ord(c)<32 or ord(c)>126 for c in release):raise ValueError('invalid recorded Linux kernel release')
    thp=value['thp'];mode=value['process_thp_disable']
    if not isinstance(thp,dict) or set(thp)!={'enabled','defrag','shmem_enabled','sizes_kib'} or not isinstance(thp['sizes_kib'],dict) or len(thp['sizes_kib'])>MAX_SIZES:
        raise ValueError('invalid THP/mTHP provenance')
    if type(mode) is not int or mode not in (0,1,3):raise ValueError('invalid process THP-disable mode')
    if not isinstance(thp['enabled'],str) or thp['enabled'] not in GLOBAL or not isinstance(thp['defrag'],str) or thp['defrag'] not in DEFRAG:raise ValueError('missing supported THP/process policy')
    shared=thp['shmem_enabled']
    if shared is not None and (not isinstance(shared,str) or shared not in SHMEM):raise ValueError('invalid global shmem THP policy')
    for size,modes in thp['sizes_kib'].items():
        if not isinstance(size,str) or not re.fullmatch(r'[1-9][0-9]{0,12}',size) or int(size)&(int(size)-1) or base_page_size is not None and int(size)*1024<=base_page_size:raise ValueError('invalid per-size mTHP size')
        if not isinstance(modes,dict) or set(modes)!={'anonymous','shmem'} or all(v is None for v in modes.values()):raise ValueError('missing per-size mTHP interfaces')
        for name,choices in (('anonymous',PER_SIZE),('shmem',SHMEM_SIZE)):
            if modes[name] is not None and (not isinstance(modes[name],str) or modes[name] not in choices):raise ValueError('invalid per-size mTHP policy')
        if modes['shmem']=='inherit' and shared is None:raise ValueError('inherited shmem THP policy needs its global control')



def _optional_choice(path,allowed):
    try:text=_text(path)
    except FileNotFoundError:return None
    return _choice(text,allowed)


def capture(root=ROOT):
    if platform.system()!='Linux':raise ValueError('Linux kernel memory policy is required for release capture')
    try:info=root.stat()
    except FileNotFoundError as error:raise ValueError('observable THP sysfs policy is required for release evidence') from error
    if not stat.S_ISDIR(info.st_mode):raise ValueError('THP sysfs root is not a directory')
    enabled=_choice(_text(root/'enabled'),GLOBAL);defrag=_choice(_text(root/'defrag'),DEFRAG)
    entries=[]
    for entry in root.iterdir():
        match=SIZE.fullmatch(entry.name)
        if match:
            if len(entries)==MAX_SIZES:raise ValueError('too many mTHP sizes')
            entries.append((int(match[1]),entry))
        elif entry.name.startswith('hugepages-'):raise ValueError('unknown mTHP size directory')
    # Some current kernels expose shmem-only sizes (e.g. 8 KiB). Record
    # interface absence explicitly; never infer anonymous policy as 'never'.
    sizes={str(size):{'anonymous':_optional_choice(entry/'enabled',PER_SIZE),
                     'shmem':_optional_choice(entry/'shmem_enabled',SHMEM_SIZE)} for size,entry in sorted(entries)}
    result={'kernel_release':platform.release(),'process_thp_disable':_process_mode(),
            'thp':{'enabled':enabled,'defrag':defrag,'shmem_enabled':_optional_choice(root/'shmem_enabled',SHMEM),'sizes_kib':sizes}}
    validate(result);return result
