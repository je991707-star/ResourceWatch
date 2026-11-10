#!/usr/bin/env python3
"""Read GPU vendor from sysfs; use NVML for NVIDIA utilization and codec engines.

One-shot, unprivileged, read-only sampler for ResourceWatch Plasma Widget.
The caller must poll it; this script never stays resident. JSON on stdout.
No shell, external network, or write operations are used.
"""

from __future__ import annotations

import ctypes
import json
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

SYS_DRM = Path('/sys/class/drm')
PCI_VENDOR = {'0x10de': 'nvidia', '0x1002': 'amd', '0x1022': 'amd',
              '0x8086': 'intel'}
GPU_CLASSES = {'0x0300', '0x0302', '0x0380'}


class Utilization(ctypes.Structure):
    _fields_ = [('gpu', ctypes.c_uint), ('memory', ctypes.c_uint)]


def percent(value):
    """Never turn unavailable or invalid samples into 0%."""
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if 0 <= number <= 100 else None


def pci_key(bus_id):
    """Unify 0000:01:00.0 and 00000000:01:00.0 without losing the domain."""
    match = re.fullmatch(r'([0-9a-fA-F]{4,8}):([0-9a-fA-F]{2}):([0-9a-fA-F]{2}\.[0-7])', str(bus_id or ''))
    if not match:
        return None
    return f'{int(match[1], 16):08x}:{match[2].lower()}:{match[3].lower()}'


def drm_devices(root=SYS_DRM):
    devices = []
    for card in sorted(root.glob('card[0-9]*'), key=lambda p: int(p.name[4:]) if p.name[4:].isdigit() else 99999):
        if not re.fullmatch(r'card[0-9]+', card.name):
            continue
        device = card / 'device'
        try:
            vendor_id = (device / 'vendor').read_text().strip().lower()
            class_id = (device / 'class').read_text().strip().lower()
            bus = pci_key(device.resolve().name)
        except (OSError, ValueError):
            continue
        if class_id[:6] not in GPU_CLASSES:
            continue
        # GPU PCI device; only NVIDIA is queried through NVML.
        devices.append({'card': card.name, 'pci': bus,
                        'vendor': PCI_VENDOR.get(vendor_id, 'other'),
                        'name': None, 'usage': None, 'encoder': None, 'decoder': None,
                        'source': 'ksystemstats'})
    return devices


class NvmlReader:
    def __init__(self):
        self.lib = ctypes.CDLL('libnvidia-ml.so.1')
        self.lib.nvmlInit_v2.restype = ctypes.c_int
        if self.lib.nvmlInit_v2() != 0:
            raise RuntimeError('nvmlInit failed')
        self.lib.nvmlDeviceGetHandleByPciBusId_v2.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
        self.lib.nvmlDeviceGetHandleByPciBusId_v2.restype = ctypes.c_int
        self.lib.nvmlDeviceGetUtilizationRates.argtypes = [ctypes.c_void_p, ctypes.POINTER(Utilization)]
        self.lib.nvmlDeviceGetUtilizationRates.restype = ctypes.c_int
        for name in ('nvmlDeviceGetEncoderUtilization', 'nvmlDeviceGetDecoderUtilization'):
            method = getattr(self.lib, name)
            method.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint)]
            method.restype = ctypes.c_int
        self.lib.nvmlDeviceGetName.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
        self.lib.nvmlDeviceGetName.restype = ctypes.c_int
        self.lib.nvmlShutdown.restype = ctypes.c_int

    def close(self):
        self.lib.nvmlShutdown()

    def query(self, pci):
        if not pci:
            return None
        handle = ctypes.c_void_p()
        if self.lib.nvmlDeviceGetHandleByPciBusId_v2(pci.encode('ascii'), ctypes.byref(handle)) != 0:
            return None
        output = {'source': 'nvml', 'name': None, 'usage': None,
                  'encoder': None, 'decoder': None}
        label = ctypes.create_string_buffer(128)
        if self.lib.nvmlDeviceGetName(handle, label, len(label)) == 0:
            output['name'] = label.value.decode('utf-8', 'replace')
        util = Utilization()
        if self.lib.nvmlDeviceGetUtilizationRates(handle, ctypes.byref(util)) == 0:
            output['usage'] = percent(util.gpu)
        for field, symbol in [('encoder', 'nvmlDeviceGetEncoderUtilization'),
                              ('decoder', 'nvmlDeviceGetDecoderUtilization')]:
            value = ctypes.c_uint()
            sample_period = ctypes.c_uint()
            # NVML_ERROR_NOT_SUPPORTED is intentionally surfaced as null, not 0.
            if getattr(self.lib, symbol)(handle, ctypes.byref(value), ctypes.byref(sample_period)) == 0:
                output[field] = percent(value.value)
        return enforce_known_capabilities(output)


def enforce_known_capabilities(device):
    # The GeForce GT 1030 has NVDEC but no NVENC. Some older driver versions
    # can return 0 for an absent encoder instead of NOT_SUPPORTED.
    if re.search(r'\bgt\s*1030\b', str(device.get('name') or ''), re.I):
        device['encoder'] = None
    return device


def read_smi_xml():
    """Fallback if NVML cannot be initialized; XML fields are optional by GPU."""
    try:
        completed = subprocess.run(
            ['nvidia-smi', '-q', '-x', '-d', 'UTILIZATION'],
            capture_output=True, text=True, timeout=3, check=True,
        )
        root = ET.fromstring(completed.stdout)
    except (FileNotFoundError, subprocess.CalledProcessError,
            subprocess.TimeoutExpired, ET.ParseError, OSError):
        return {}
    values = {}
    for gpu in root.findall('.//gpu'):
        bus = pci_key(gpu.get('id') or gpu.findtext('pci/pci_bus_id'))
        if not bus:
            continue
        def read(path):
            raw = gpu.findtext(path)
            match = re.fullmatch(r'\s*(\d+(?:\.\d+)?)\s*%?\s*', raw or '')
            return percent(match.group(1)) if match else None
        values[bus] = enforce_known_capabilities({
            'source': 'nvidia-smi',
            'name': gpu.findtext('product_name'),
            'usage': read('utilization/gpu_util'),
            'encoder': read('utilization/encoder_util'),
            'decoder': read('utilization/decoder_util')
        })
    return values


def sample():
    devices = drm_devices()
    nvidia = [d for d in devices if d['vendor'] == 'nvidia']
    if not nvidia:
        return {'devices': devices}

    reader = None
    nvml_failed = False
    try:
        reader = NvmlReader()
    except (OSError, AttributeError, RuntimeError):
        nvml_failed = True
    try:
        for device in nvidia:
            try:
                result = reader.query(device['pci']) if reader else None
            except (OSError, AttributeError, ValueError, RuntimeError):
                result = None
            if result:
                device.update(result)
                if result['usage'] is None:
                    nvml_failed = True
            else:
                nvml_failed = True
    finally:
        if reader:
            reader.close()

    if nvml_failed:
        fallback = read_smi_xml()
        for device in nvidia:
            result = fallback.get(device['pci'])
            if not result:
                continue
            if device['source'] == 'nvml':
                # Keep NVML's per-engine capability results (including unsupported).
                # Only the missing GPU-Util may use nvidia-smi's GPU-Util.
                if device['usage'] is None and result['usage'] is not None:
                    device['usage'] = result['usage']
                    device['source'] = 'nvidia-smi'
            else:
                device.update(result)
    return {'devices': devices}


if __name__ == '__main__':
    try:
        print(json.dumps(sample(), ensure_ascii=True, separators=(',', ':')))
    except Exception as exc:
        # Avoid printing stack traces or spurious fabricated sensor data.
        print(json.dumps({'devices': [], 'error': type(exc).__name__}))
        sys.exit(1)
