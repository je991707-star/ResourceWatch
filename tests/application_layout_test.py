"""Static contracts for app / widget modes (no KDE runtime required)."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ApplicationLayoutTest(unittest.TestCase):
    def test_html_reuses_a_single_monitor_root(self):
        html = (ROOT / 'package/contents/web/index.html').read_text()
        js = (ROOT / 'package/contents/web/app.js').read_text()
        self.assertEqual(html.count('id="resourceWatch"'), 1)
        self.assertIn('id="navOverview"', html)
        self.assertIn('id="navWidgets"', html)
        self.assertIn('id="appSettings"', html)
        self.assertIn('appendChild($("resourceWatch"))', js)
        self.assertIn('new URLSearchParams(window.location.search)', js)
        self.assertIn('function switchPage(page)', js)

    def test_app_has_minimum_not_fixed_maximum(self):
        qml = (ROOT / 'package/contents/ui/main.qml').read_text()
        self.assertIn('property int minAppWidth: 900', qml)
        self.assertIn('property int minAppHeight: 580', qml)
        self.assertIn('property: "minimumWidth"', qml)
        self.assertNotIn('property: "maximumWidth"', qml)
        self.assertNotIn('property: "maximumHeight"', qml)
        self.assertIn('viewMode: root.isPlasmaWindowed ? "application" : "widget"', qml)

    def test_kde_bridge_remains_single_source(self):
        view = (ROOT / 'package/contents/ui/HtmlView.qml').read_text()
        self.assertIn('url: Qt.resolvedUrl("../web/index.html?mode=" + view.viewMode)', view)
        self.assertIn('function publishSnapshot()', view)
        self.assertIn('onSnapshotDue()', view)

    def test_launcher(self):
        entry = (ROOT / 'packaging/io.github.local.resourcewatch.desktop').read_text()
        self.assertIn('Exec=plasmawindowed io.github.local.resourcewatch', entry)
        install = (ROOT / 'install.sh').read_text()
        self.assertIn('install -Dm644', install)


if __name__ == '__main__':
    unittest.main()
