#!/usr/bin/env python3
"""Read display-only CPU model and RAM transfer rate without persistent privileges.

Normal operation reads /proc/cpuinfo and optionally accessible SMBIOS type-17
entries or a small user-owned cache. The optional `--cache-ram-from-stdin` mode
is designed for: sudo dmidecode --type 17 | python3 hardware_info.py --cache-ram-from-stdin
The sudo privilege is only used by dmidecode, not by ResourceWatch.
"""
import json
import os
from pathlib import Path
import re
import sys
import tempfile


SYSFS_DIMM_DIR = Path('/sys/firmware/dmi/entries')


def cache_path(home=None):
    if home is not None:
        state = Path(home) / '.local' / 'state'
    else:
        state = Path(os.environ.get('XDG_STATE_HOME') or Path.home() / '.local' / 'state')
    return state / 'resourcewatch' / 'ram_speed.json'


def cpu_model(info_path=Path('/proc/cpuinfo')):
    try:
        content = Path(info_path).read_text(encoding='utf-8', errors='replace')
    except OSError:
        return None
    for line in content.splitlines():
        if re.match(r'^\s*(model name|Hardware)\s*:', line, flags=re.I):
            value = line.split(':', 1)[1].strip()
            if value:
                return re.sub(r'\s+', ' ', value)
    return None


def unique_speed(speeds):
    speeds = sorted({int(s) for s in speeds if isinstance(s, int) and 100 <= s <= 10000})
    if not speeds:
        return None
    if len(speeds) == 1:
        return f'{speeds[0]} MT/s'
    return f'{speeds[0]}–{speeds[-1]} MT/s'


def parse_smbios_type17(blob):
    """Read installed DIMM's configured speed (SMBIOS type 17), else rated speed."""
    if len(blob) < 0x17 or blob[0] != 17:
        return None
    size = int.from_bytes(blob[0x0c:0x0e], 'little')
    if size in (0, 0xffff):  # Unpopulated or unknown slot
        return None
    structure_len = min(blob[1], len(blob))
    rated = int.from_bytes(blob[0x15:0x17], 'little') if structure_len >= 0x17 else 0
    configured = int.from_bytes(blob[0x20:0x22], 'little') if structure_len >= 0x22 else 0
    for speed in (configured, rated):
        if 100 <= speed <= 10000:
            return speed
    return None


def read_sysfs_speeds(root=SYSFS_DIMM_DIR):
    speeds = []
    try:
        candidates = sorted(Path(root).glob('17-*/raw'))
    except OSError:
        return speeds
    for item in candidates:
        try:
            value = parse_smbios_type17(item.read_bytes())
        except OSError:  # Usually protected on Fedora/Bazzite
            continue
        if value is not None:
            speeds.append(value)
    return speeds


def read_dmidecode_speeds(output):
    """Parse a privileged dmidecode text dump supplied once via stdin."""
    speeds = []
    blocks = re.split(r'(?m)^Handle\s+0x[0-9a-f]+.*$', output, flags=re.I)
    for block in blocks:
        if not re.search(r'(?m)^\s*Memory Device\s*$', block):
            continue
        size = re.search(r'(?m)^\s*Size:\s*(.+)$', block)
        if not size or re.search(r'No Module Installed|Not Installed|Unknown', size.group(1), re.I):
            continue
        # Prefer actual configured speed; memory's rated speed can differ.
        for field in ('Configured Memory Speed', 'Configured Clock Speed', 'Speed'):
            match = re.search(r'(?m)^\s*' + re.escape(field) + r':\s*(\d+)\s*(?:MT/s|MHz)', block, re.I)
            if match:
                speed = int(match.group(1))
                if 100 <= speed <= 10000:
                    speeds.append(speed)
                    break
    return speeds


def read_cache(path=None):
    try:
        value = json.loads(Path(path or cache_path()).read_text(encoding='utf-8'))
        speeds = value.get('speedsMTs', [])
        if not isinstance(speeds, list):
            return []
        return [v for v in speeds if type(v) is int and 100 <= v <= 10000]
    except (OSError, ValueError, TypeError, AttributeError):
        return []


def save_cache(speeds, path=None):
    path = Path(path or cache_path())
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(prefix='.ram-speed-', dir=str(path.parent))
    try:
        with os.fdopen(fd, 'w', encoding='utf-8') as target:
            json.dump({'speedsMTs': speeds}, target)
            target.write('\n')
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def sample():
    speeds = read_sysfs_speeds() or read_cache()
    return {'cpuModel': cpu_model(), 'ramSpeedText': unique_speed(speeds)}


def main(argv):
    if argv == ['--cache-ram-from-stdin']:
        speeds = read_dmidecode_speeds(sys.stdin.read())
        result = unique_speed(speeds)
        if not result:
            print('No readable installed DIMM speeds in dmidecode output.', file=sys.stderr)
            return 1
        save_cache(speeds)
        print('RAM speed cached: ' + result)
        return 0
    if argv:
        print('Usage: hardware_info.py [--cache-ram-from-stdin]', file=sys.stderr)
        return 2
    print(json.dumps(sample(), ensure_ascii=True, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
