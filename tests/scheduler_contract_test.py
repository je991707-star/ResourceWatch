"""Static contract: one HTML-controlled interval for all eight update groups."""
from pathlib import Path
import re
import unittest
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[1]
CONTENTS = ROOT / 'package/contents'
LEGACY = (
    'cpuInterval', 'nvidiaInterval', 'gpuMemoryInterval', 'ramInterval',
    'diskInterval', 'networkInterval', 'historyInterval', 'htmlInterval'
)


class SchedulerContractTest(unittest.TestCase):
    def test_one_kconfig_interval(self):
        xml = ElementTree.parse(CONTENTS / 'config/main.xml')
        ns = {'k': 'http://www.kde.org/standards/kcfg/1.0'}
        entries = {e.attrib['name']:e for e in xml.findall('.//k:entry', ns)}
        interval = entries['updateInterval']
        self.assertEqual(interval.find('k:default', ns).text, '500')
        self.assertEqual(interval.find('k:min', ns).text, '100')
        self.assertEqual(interval.find('k:max', ns).text, '10000')
        for key in LEGACY:
            self.assertNotIn(key, entries)
        self.assertIn('ipRefreshSerial', entries)
        self.assertIn('diskRefreshSerial', entries)

    def test_one_html_setting_and_matching_bridge(self):
        html = (CONTENTS / 'web/settings/index.html').read_text()
        js = (CONTENTS / 'web/settings/settings.js').read_text()
        qml = (CONTENTS / 'ui/configGeneral.qml').read_text()
        main = (CONTENTS / 'ui/main.qml').read_text()
        manager = (CONTENTS / 'ui/models/SensorManager.qml').read_text()
        self.assertIn('id="updateInterval"', html)
        self.assertIn('updateInterval: {min:100, max:10000, step:50}', js)
        self.assertIn('property int cfg_updateInterval', qml)
        self.assertIn('updateInterval: Plasmoid.configuration.updateInterval', main)
        self.assertIn('property int updateInterval: 500', manager)
        for key in LEGACY:
            self.assertNotIn(f'id="{key}"', html)
            self.assertNotIn(f'cfg_{key}', qml)
            self.assertNotIn(f'Plasmoid.configuration.{key}', main)
            self.assertNotIn(f'manager.{key}', manager)
        self.assertIn('id="refreshIPv4"', html)
        self.assertIn('id="refreshDisks"', html)
        self.assertIn('function refreshDevice(kind)', qml)

    def test_one_scheduler_and_unchanged_manual_tasks(self):
        manager = (CONTENTS / 'ui/models/SensorManager.qml').read_text()
        view = (CONTENTS / 'ui/HtmlView.qml').read_text()
        self.assertEqual(len(re.findall(r'property Timer \w+: Timer', manager)), 1)
        self.assertNotRegex(view, r'(?m)^\s*Timer\s*\{')
        self.assertIn('onUpdateIntervalChanged: _rescheduleAll()', manager)
        for source in ['cpuSensor','gpuIdx','gpuTemp','memUsed','swapUsed','netDownS']:
            self.assertRegex(manager, r'property Sensors.Sensor ' + source + r'\b')
        self.assertIn('refreshDisks();', manager)
        self.assertIn('_requestHardwareProbe();', manager)
        self.assertNotIn('_hardwareProbeTimer', manager)
        self.assertNotIn('_diskDiscoveryTimer', manager)
        self.assertIn('function refreshNetworkIp()', manager)
        self.assertIn('onSnapshotDue()', view)


if __name__ == '__main__':
    unittest.main()
