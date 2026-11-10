#!/usr/bin/env python3
"""GPU-only regression tests; fake sysfs, NVML and nvidia-smi responses."""
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / 'package/contents/scripts/gpu_backend.py'
spec = importlib.util.spec_from_file_location('resourcewatch_gpu_backend', MODULE)
gpu = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gpu)


class GpuBackendTests(unittest.TestCase):
    def test_percent_and_pci_validation(self):
        self.assertEqual(gpu.percent('8'), 8)
        self.assertEqual(gpu.percent(0), 0)
        self.assertIsNone(gpu.percent('N/A'))
        self.assertIsNone(gpu.percent(104))
        self.assertEqual(gpu.pci_key('0000:01:00.0'), '00000000:01:00.0')
        self.assertEqual(gpu.pci_key('00000000:01:00.0'), '00000000:01:00.0')
        self.assertIsNone(gpu.pci_key('invalid'))

    def test_vendor_discovery_excludes_non_gpus(self):
        with tempfile.TemporaryDirectory() as temp:
            sysroot = Path(temp)
            for card, vendor, klass, pci in [
                ('card0', '0x8086', '0x030000', '0000:00:02.0'),
                ('card1', '0x10de', '0x030200', '0000:01:00.0'),
                ('card2', '0x1002', '0x030000', '0000:02:00.0'),
                ('card3', '0x10de', '0x020000', '0000:03:00.0'),
            ]:
                target = sysroot / 'pci' / pci
                target.mkdir(parents=True)
                (target / 'vendor').write_text(vendor)
                (target / 'class').write_text(klass)
                (sysroot / card).mkdir()
                (sysroot / card / 'device').symlink_to(target, target_is_directory=True)
            found = gpu.drm_devices(sysroot)
            self.assertEqual([x['vendor'] for x in found], ['intel', 'nvidia', 'amd'])
            self.assertEqual(found[1]['pci'], '00000000:01:00.0')

    def test_nvml_only_samples_nvidia(self):
        devices = [
            {'vendor': 'amd', 'pci': '00000000:00:02.0', 'source': 'ksystemstats', 'usage': None},
            {'vendor': 'nvidia', 'pci': '00000000:01:00.0', 'source': 'ksystemstats', 'usage': None},
        ]
        class FakeNvml:
            seen = []
            def query(self, pci):
                self.seen.append(pci)
                return {'source': 'nvml', 'name': 'Quadro P620',
                        'usage': 8, 'encoder': 40, 'decoder': 0}
            def close(self): pass
        fake = FakeNvml()
        with patch.object(gpu, 'drm_devices', return_value=devices), patch.object(gpu, 'NvmlReader', return_value=fake):
            result = gpu.sample()
        self.assertEqual(fake.seen, ['00000000:01:00.0'])
        self.assertEqual(result['devices'][1]['usage'], 8)
        self.assertEqual(result['devices'][1]['encoder'], 40)
        self.assertEqual(result['devices'][0]['source'], 'ksystemstats')

    def test_smi_fallback_and_unsupported_encoder(self):
        xml = '''<nvidia_smi_log>
          <gpu id="00000000:01:00.0"><product_name>GeForce GT 1030</product_name>
            <utilization><gpu_util>8 %</gpu_util><encoder_util>0 %</encoder_util>
              <decoder_util>5 %</decoder_util></utilization></gpu>
          <gpu id="00000000:02:00.0"><product_name>Quadro P620</product_name>
            <utilization><gpu_util>12 %</gpu_util><encoder_util>N/A</encoder_util>
              <decoder_util>0 %</decoder_util></utilization></gpu>
        </nvidia_smi_log>'''
        completed = subprocess.CompletedProcess(args=[], returncode=0, stdout=xml, stderr='')
        with patch.object(gpu.subprocess, 'run', return_value=completed):
            result = gpu.read_smi_xml()
        self.assertEqual(result['00000000:01:00.0']['usage'], 8)
        self.assertIsNone(result['00000000:01:00.0']['encoder'])
        self.assertEqual(result['00000000:01:00.0']['decoder'], 5)
        self.assertIsNone(result['00000000:02:00.0']['encoder'])
        self.assertEqual(result['00000000:02:00.0']['decoder'], 0)

    def test_smi_only_if_nvml_initialization_fails(self):
        device = {'vendor': 'nvidia', 'pci': '00000000:01:00.0', 'source': 'ksystemstats',
                  'name': None, 'usage': None, 'encoder': None, 'decoder': None}
        fallback = {'00000000:01:00.0': {'usage': 9, 'encoder': None, 'decoder': 20,
                                          'name': 'Quadro P620', 'source': 'nvidia-smi'}}
        with patch.object(gpu, 'drm_devices', return_value=[device]), \
             patch.object(gpu, 'NvmlReader', side_effect=OSError('missing')), \
             patch.object(gpu, 'read_smi_xml', return_value=fallback):
            result = gpu.sample()['devices'][0]
        self.assertEqual(result['usage'], 9)
        self.assertEqual(result['source'], 'nvidia-smi')
        self.assertIsNone(result['encoder'])


if __name__ == '__main__':
    unittest.main()
