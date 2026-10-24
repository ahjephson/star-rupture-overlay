"""Optional read-only live and own-window lifecycle checks; supported game required."""
import ctypes as c
from ctypes import wintypes as w
import datetime, json, os, pathlib, shutil, subprocess, time

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXE = ROOT / 'dist/starrupture-wave-overlay.exe'
RUN = ROOT / 'local' / ('single-app-' + datetime.datetime.now().strftime('%Y%m%dT%H%M%S%f'))
RUN.mkdir(parents=True)
user = c.WinDLL('user32', use_last_error=True)
kernel = c.WinDLL('kernel32', use_last_error=True)
CALLBACK = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
user.EnumWindows.argtypes = [CALLBACK, w.LPARAM]
user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
user.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
user.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]

class Entry(c.Structure):
    _fields_ = [('dwSize', w.DWORD), ('cntUsage', w.DWORD), ('th32ProcessID', w.DWORD),
                ('th32DefaultHeapID', c.c_size_t), ('th32ModuleID', w.DWORD),
                ('cntThreads', w.DWORD), ('th32ParentProcessID', w.DWORD),
                ('pcPriClassBase', w.LONG), ('dwFlags', w.DWORD), ('szExeFile', w.WCHAR * 260)]
kernel.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
kernel.CreateToolhelp32Snapshot.restype = w.HANDLE
kernel.Process32FirstW.argtypes = [w.HANDLE, c.POINTER(Entry)]
kernel.Process32NextW.argtypes = [w.HANDLE, c.POINTER(Entry)]
kernel.CloseHandle.argtypes = [w.HANDLE]

def children(pid):
    h = kernel.CreateToolhelp32Snapshot(2, 0)
    assert h not in (None, c.c_void_p(-1).value)
    found = []
    try:
        e = Entry(); e.dwSize = c.sizeof(e)
        more = kernel.Process32FirstW(h, c.byref(e))
        while more:
            if e.th32ParentProcessID == pid:
                found.append((e.th32ProcessID, e.szExeFile))
            more = kernel.Process32NextW(h, c.byref(e))
    finally:
        kernel.CloseHandle(h)
    return found

def window(pid):
    found = []
    @CALLBACK
    def visit(hwnd, _):
        owner = w.DWORD(); user.GetWindowThreadProcessId(hwnd, c.byref(owner))
        name = c.create_unicode_buffer(256); user.GetClassNameW(hwnd, name, 256)
        if owner.value == pid and name.value == 'StarRuptureWaveTimer': found.append(hwnd)
        return True
    user.EnumWindows(visit, 0)
    return found[0] if found else None

def wait_for(predicate, process, seconds=60):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        assert not children(process.pid), 'Unexpected child process'
        value = predicate()
        if value: return value
        assert process.poll() is None, 'App exited early'
        time.sleep(.025)
    raise AssertionError('Timed out waiting for app')

def start(args, name, env=None):
    path = RUN / (name + '-stderr.txt')
    log = path.open('w', encoding='utf-8')
    p = subprocess.Popen([str(EXE), *args], cwd=RUN, env=env, stdout=subprocess.DEVNULL,
                         stderr=log, creationflags=0x08000000)
    return p, log, path

# Actual embedded live data, with sequence/build/session/world validation enabled.
p, log, path = start(['--self-test-reader', str(RUN/'reader')], 'reader')
try:
    until = time.monotonic() + 70
    while p.poll() is None:
        assert not children(p.pid), 'Reader spawned a child'
        assert time.monotonic() < until, 'Live test timeout'
        time.sleep(.025)
    assert p.returncode == 0, path.read_text(encoding='utf-8')
finally:
    if p.poll() is None: p.kill(); p.wait()
    log.close()
live = json.loads((RUN/'reader/reader-validation.json').read_text(encoding='utf-8'))
assert live['worldReads'] >= 24 and live['lastStatistics']['menuReads'] >= 60

cases = []
for mode in ('normal', 'cold-cancel'):
    appdata = RUN / mode
    env = os.environ.copy(); env['LOCALAPPDATA'] = str(appdata)
    if mode == 'normal':
        shutil.copytree(RUN/'reader/cache', appdata/'StarRuptureWaveOverlay/cache')
    p, log, path = start([], mode, env)
    try:
        hwnd = wait_for(lambda: window(p.pid), p, 10)
        if mode == 'normal':
            wait_for(lambda: 'first validated live observation' in path.read_text(encoding='utf-8'), p)
            other, otherlog, _ = start([], 'duplicate', env)
            try: assert other.wait(timeout=5) == 0
            finally:
                if other.poll() is None: other.kill(); other.wait()
                otherlog.close()
            assert p.poll() is None and window(p.pid) == hwnd
        if mode == 'cold-cancel':
            wait_for(lambda: 'Discovering current Unreal runtime references' in path.read_text(encoding='utf-8'), p)
        started = time.monotonic()
        assert user.PostMessageW(hwnd, 0x10, 0, 0), 'Cannot close own window'
        assert p.wait(timeout=5) == 0
        elapsed = round((time.monotonic()-started)*1000, 2)
        assert not children(p.pid)
        text = path.read_text(encoding='utf-8')
        assert 'no reads remain in flight' in text, text
        cases.append({'mode':mode, 'noArguments':True, 'unrelatedWorkingDirectory':True,
                      'childProcesses':0, 'shutdownMs':elapsed, 'exitCode':p.returncode})
    finally:
        if p.poll() is None:
            hwnd = window(p.pid)
            if hwnd: user.PostMessageW(hwnd, 0x10, 0, 0)
            try: p.wait(timeout=5)
            except subprocess.TimeoutExpired: p.kill(); p.wait()
        log.close()
result = {'status':'passed', 'live':live, 'singleInstance':'passed', 'cases':cases}
(RUN/'validation.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps(result, indent=2))
print('Private evidence:', RUN)
