//! The probe as a self-contained Python 3 script for an ssh host (`M12-P02`): the same rules as the engine of this machine (`mod.rs`, `net.rs`, `http.rs`, `classify.rs`), run
//! on the remote machine through `python3 -c`. Arguments: the process table root (`/proc`), `1` for all users' ports and the time budget in seconds. It prints one line,
//! `LICO_JSON=<reply>` or `LICO_ERR=<code>`. It is written for Python 3.6 and later and uses the standard library only.

pub const SCRIPT: &str = r#"
import json, os, socket, sys, time, ipaddress, unicodedata
from concurrent.futures import ThreadPoolExecutor
root, all_users, budget = sys.argv[1], sys.argv[2] == '1', float(sys.argv[3])
started = time.monotonic()
deadline = started + budget
scope = 'all_users' if all_users else 'same_user'
MAX_PROBED, PARALLEL, BODY_CAP, HEAD_CAP, MAX_MODELS = 64, 16, 64 * 1024, 16 * 1024, 50
def reply(**more):
    out = {'host': None, 'os': 'linux', 'scope': scope, 'supported': True, 'source': 'remote_script', 'listening': 0, 'probed': 0, 'skipped': 0, 'elapsed_ms': int((time.monotonic() - started) * 1000), 'endpoints': []}
    out.update(more)
    print('LICO_JSON=' + json.dumps(out, separators=(',', ':')))
if not sys.platform.startswith('linux'):
    reply(supported=False, os=sys.platform[:32], reason='os_unknown')
    sys.exit(0)
def table(name, v6):
    try:
        with open(os.path.join(root, 'net', name)) as fh:
            lines = fh.read().splitlines()[1:]
    except OSError:
        return None
    found = []
    for line in lines:
        f = line.split()
        if len(f) < 10 or f[3] != '0A' or ':' not in f[1]:
            continue
        addr, port = f[1].rsplit(':', 1)
        try:
            if v6:
                ip = ipaddress.IPv6Address(b''.join(bytes.fromhex(addr[i:i + 8])[::-1] for i in range(0, 32, 8)))
            else:
                ip = ipaddress.IPv4Address(bytes.fromhex(addr)[::-1])
            found.append((ip, int(port, 16), int(f[9])))
        except ValueError:
            continue
    return found
a, b = table('tcp', False), table('tcp6', True)
if a is None and b is None:
    reply(supported=False, reason='proc_unreadable')
    sys.exit(0)
def bind_of(ip):
    if ip.version == 6 and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    return 'loopback' if ip.is_loopback else ('all' if ip.is_unspecified else 'address')
def clean_name(raw):
    return ''.join(c for c in raw.strip() if 33 <= ord(c) <= 126 or c == ' ')[:32]
owners = {}
me = os.geteuid()
try:
    pids = [p for p in os.listdir(root) if p.isdigit()][:32768]
except OSError:
    pids = []
for pid in pids:
    d = os.path.join(root, pid)
    try:
        if os.stat(d).st_uid != me:
            continue
        fds = os.listdir(os.path.join(d, 'fd'))[:8192]
    except OSError:
        continue
    try:
        with open(os.path.join(d, 'comm')) as fh:
            name = clean_name(fh.read())
    except OSError:
        name = ''
    for fd in fds:
        try:
            link = os.readlink(os.path.join(d, 'fd', fd))
        except OSError:
            continue
        if link.startswith('socket:[') and link.endswith(']') and link[8:-1].isdigit():
            owners.setdefault(int(link[8:-1]), name)
chosen = [(ip, port, inode) for ip, port, inode in (a or []) + (b or []) if all_users or inode in owners]
chosen.sort(key=lambda t: (t[1], t[0].version, int(t[0])))
uniq = []
for t in chosen:
    if not uniq or (uniq[-1][0], uniq[-1][1]) != (t[0], t[1]):
        uniq.append(t)
chosen = uniq
probed = chosen[:MAX_PROBED]
def target(ip, port):
    kind = bind_of(ip)
    if kind == 'address':
        return str(ip), port
    return ('127.0.0.1' if ip.version == 4 else '::1'), port
def left(dl):
    return dl - time.monotonic()
class Fault(Exception):
    def __init__(self, kind):
        self.kind = kind
def contact(addr, dl):
    t = left(dl)
    if t <= 0:
        return ('failed', 'budget')
    try:
        s = socket.create_connection(addr, timeout=min(0.3, t))
    except socket.timeout:
        return ('failed', 'timeout')
    except OSError:
        return ('failed', 'refused')
    s.settimeout(min(0.15, max(0.001, left(dl))))
    try:
        data = s.recv(1)
    except socket.timeout:
        return ('quiet', s)
    except OSError:
        s.close()
        return ('closed', None)
    s.close()
    return ('closed', None) if data == b'' else ('speaks', None)
def parse(buf, capped):
    if not buf:
        raise Fault('timeout')
    if len(buf) < 5 or not buf.startswith(b'HTTP/'):
        raise Fault('timeout' if len(buf) < 5 and b'HTTP/'.startswith(buf) else 'not_http')
    end = buf.find(b'\r\n\r\n')
    if end < 0:
        raise Fault('timeout')
    parts = buf[:min(end, 64)].decode('latin-1').split()
    try:
        status = int(parts[1])
    except (IndexError, ValueError):
        raise Fault('not_http')
    body = buf[end + 4:]
    return status, body[:BODY_CAP], capped or len(body) > BODY_CAP
def get(s, addr, path, dl):
    end = min(time.monotonic() + 1.5, dl)
    host = ('[%s]:%d' % addr) if ':' in addr[0] else ('%s:%d' % addr)
    try:
        s.settimeout(max(0.001, left(end)))
        s.sendall(('GET %s HTTP/1.0\r\nHost: %s\r\nAccept: application/json\r\nUser-Agent: lico-discover\r\nConnection: close\r\n\r\n' % (path, host)).encode())
    except OSError:
        raise Fault('io')
    buf, capped = b'', False
    while True:
        if len(buf) >= 5 and not buf.startswith(b'HTTP/'):
            raise Fault('not_http')
        wait = left(end)
        if wait <= 0:
            break
        s.settimeout(wait)
        try:
            chunk = s.recv(8192)
        except OSError:
            break
        if not chunk:
            break
        buf += chunk
        if len(buf) >= HEAD_CAP + BODY_CAP:
            capped = True
            break
    s.close()
    return parse(buf, capped)
def scan_strings(body, key):
    text = body.decode('utf-8', 'replace')
    needle = '"%s"' % key
    found, rest = [], text
    while True:
        at = rest.find(needle)
        if at < 0:
            return found
        rest = rest[at + len(needle):]
        after = rest.lstrip()
        if not after.startswith(':'):
            continue
        value = after[1:].lstrip()
        if not value.startswith('"'):
            continue
        out, closed, i = [], False, 1
        while i < len(value):
            c = value[i]
            if c == '\\':
                i += 1
                if i < len(value):
                    out.append(value[i])
            elif c == '"':
                closed = True
                break
            else:
                out.append(c)
            i += 1
        if closed:
            found.append(''.join(out))
def models_of(ids, truncated):
    total = len(ids)
    clean = [''.join(c for c in i if unicodedata.category(c) != 'Cc')[:120] for i in ids[:MAX_MODELS]]
    return {'ids': clean, 'total': total, 'truncated': truncated or total > MAX_MODELS}
def parsed(body, truncated):
    if truncated:
        return None
    try:
        return json.loads(body.decode('utf-8'))
    except ValueError:
        return None
def openai_models(status, body, truncated):
    if status != 200:
        return None
    v = parsed(body, truncated)
    if v is not None:
        if not isinstance(v, dict) or not isinstance(v.get('data'), list):
            return None
        return models_of([m['id'] for m in v['data'] if isinstance(m, dict) and isinstance(m.get('id'), str)], False)
    if truncated:
        text = body.decode('utf-8', 'replace')
        at = text.find('"data"')
        if at >= 0 and text[at + 6:].lstrip().startswith(':') and text[at + 6:].lstrip()[1:].lstrip().startswith('['):
            return models_of(scan_strings(body, 'id'), True)
    return None
def ollama_tags(status, body, truncated):
    if status != 200:
        return None
    v = parsed(body, truncated)
    if v is not None:
        if not isinstance(v, dict) or not isinstance(v.get('models'), list):
            return None
        names = []
        for m in v['models']:
            if isinstance(m, dict):
                n = m.get('name') if isinstance(m.get('name'), str) else m.get('model')
                if isinstance(n, str):
                    names.append(n)
        return models_of(names, False)
    if truncated:
        text = body.decode('utf-8', 'replace')
        at = text.find('"models"')
        if at >= 0 and text[at + 8:].lstrip().startswith(':') and text[at + 8:].lstrip()[1:].lstrip().startswith('['):
            return models_of(scan_strings(body, 'name'), True)
    return None
def key_needed(status, body, truncated):
    if status not in (401, 403):
        return False
    v = parsed(body, truncated)
    return isinstance(v, dict) and isinstance(v.get('error'), (dict, str))
def plain(kind, detail):
    return {'kind': kind, 'apis': [], 'auth': 'unknown', 'models': {'ids': [], 'total': 0, 'truncated': False}, 'detail': detail}
def probe(ip, port, dl):
    addr = target(ip, port)
    state, s = contact(addr, dl)
    if state == 'speaks':
        return plain('not-http', 'speaks_first')
    if state == 'closed':
        return plain('not-http', 'closed')
    if state == 'failed':
        return plain('unknown', s)
    try:
        status, body, trunc = get(s, addr, '/v1/models', dl)
    except Fault as f:
        return plain('not-http', 'not_http') if f.kind == 'not_http' else plain('unknown', f.kind)
    tags = None
    try:
        t = left(dl)
        if t > 0:
            s2 = socket.create_connection(addr, timeout=min(0.3, t))
            tags = get(s2, addr, '/api/tags', dl)
    except (Fault, OSError):
        tags = None
    openai = openai_models(status, body, trunc)
    ollama = ollama_tags(*tags) if tags else None
    keyed = key_needed(status, body, trunc)
    apis = (['openai'] if openai is not None or keyed else []) + (['ollama'] if ollama is not None else [])
    empty = {'ids': [], 'total': 0, 'truncated': False}
    if ollama is not None:
        kind, auth, models = 'ollama', 'none_needed', ollama
    elif openai is not None:
        kind, auth, models = 'openai', 'none_needed', openai
    elif keyed:
        kind, auth, models = 'openai', 'key_needed', empty
    else:
        kind, auth, models = 'http-other', 'unknown', empty
    return {'kind': kind, 'apis': apis, 'auth': auth, 'models': models, 'detail': None}
def run_one(item):
    i, (ip, port, inode) = item
    t0 = time.monotonic()
    found = probe(ip, port, deadline)
    return i, {'port': port, 'bind': bind_of(ip), 'address': str(ip), 'kind': found['kind'], 'apis': found['apis'], 'auth': found['auth'],
               'models': found['models']['ids'], 'models_total': found['models']['total'], 'models_truncated': found['models']['truncated'],
               'process': owners.get(inode), 'probe_ms': int((time.monotonic() - t0) * 1000), 'detail': found['detail']}
with ThreadPoolExecutor(max_workers=max(1, min(PARALLEL, len(probed)))) as pool:
    results = list(pool.map(run_one, list(enumerate(probed))))
results.sort(key=lambda r: (r[1]['kind'] not in ('openai', 'ollama'), r[0]))
reply(listening=len(chosen), probed=len(probed), skipped=len(chosen) - len(probed), endpoints=[r[1] for r in results])
"#;
