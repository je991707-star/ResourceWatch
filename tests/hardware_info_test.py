"""Unit tests for the optional, unprivileged hardware label probe."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'package/contents/scripts/hardware_info.py'
spec = importlib.util.spec_from_file_location('hardware_info', SCRIPT)
hw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hw)


class HardwareInfoTests(unittest.TestCase):
    def test_cpu_model(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'cpuinfo'
            source.write_text('processor : 0\nmodel name : Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz\n')
            self.assertEqual(hw.cpu_model(source), 'Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz')

    def test_dmi_prefers_configured_speed_and_skips_empty(self):
        dimm = bytearray(0x28)
        dimm[0] = 17
        dimm[1] = 0x28
        dimm[0x0c:0x0e] = (8192).to_bytes(2, 'little')
        dimm[0x15:0x17] = (1866).to_bytes(2, 'little')
        dimm[0x20:0x22] = (1600).to_bytes(2, 'little')
        self.assertEqual(hw.parse_smbios_type17(dimm), 1600)
        dimm[0x0c:0x0e] = (0).to_bytes(2, 'little')
        self.assertIsNone(hw.parse_smbios_type17(dimm))

    def test_dmidecode_parsing_actual_not_rated(self):
        sample = '''Handle 0x0030, DMI type 17, 40 bytes
Memory Device
\tSize: 8 GB
\tSpeed: 1866 MT/s
\tConfigured Memory Speed: 1600 MT/s
Handle 0x0031, DMI type 17, 40 bytes
Memory Device
\tSize: No Module Installed
\tSpeed: Unknown
Handle 0x0032, DMI type 17, 40 bytes
Memory Device
\tSize: 8 GB
\tSpeed: 1600 MT/s
'''
        self.assertEqual(hw.read_dmidecode_speeds(sample), [1600, 1600])
        self.assertEqual(hw.unique_speed(hw.read_dmidecode_speeds(sample)), '1600 MT/s')

    def test_cache_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'resourcewatch' / 'ram_speed.json'
            hw.save_cache([1600, 1600], path)
            self.assertEqual(hw.read_cache(path), [1600, 1600])
            self.assertEqual(json.loads(path.read_text())['speedsMTs'], [1600, 1600])
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_unknown_or_conflicting_speed(self):
        self.assertIsNone(hw.unique_speed([]))
        self.assertEqual(hw.unique_speed([1333, 1600]), '1333–1600 MT/s')
        self.assertEqual(hw.read_dmidecode_speeds('no DMI data'), [])


if __name__ == '__main__':
    unittest.main()
