"""Validate the installed-app resource contracts, without an SDK or renderer."""
import json
import importlib.util
from pathlib import Path
import re
import struct
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
ANDROID = "{http://schemas.android.com/apk/res/android}"


def png_info(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"Not a PNG: {path}")
    return struct.unpack(">IIBBBBB", data[16:29])


class AppIconTests(unittest.TestCase):
    def test_brand_geometry_is_shared_with_android_and_shell(self):
        ns = "{http://www.w3.org/2000/svg}"
        source = ET.parse(ROOT / "desktop/packaging/icons/mark.svg").findall(".//" + ns + "path")
        self.assertEqual(len(source), 8)
        self.assertEqual(len({p.get("d") for p in source}), 1)
        self.assertEqual([p.get("transform") for p in source],
                         [f"rotate({45*k})" for k in range(8)])
        arm = source[0].get("d")
        shell = ET.parse(ROOT / "crates/shell/resources/icons/octopus.svg").findall(".//" + ns + "path")
        self.assertEqual([p.get("d") for p in shell], [arm] * 8)
        for package in ("desktop", "phone"):
            vector = ET.parse(ROOT / package / "resources/android/res/drawable/ic_launcher_foreground.xml")
            self.assertEqual([p.get(ANDROID + "pathData") for p in vector.findall(".//path")], [arm] * 8)

    def test_overlapping_arms_stay_filled_and_centre_stays_open(self):
        spec = importlib.util.spec_from_file_location("make_icons", ROOT / "desktop/packaging/make_icons.py")
        renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(renderer)
        coverage = renderer.mark_coverage(128)
        self.assertEqual(coverage[64][64], 0)
        # The two neighbouring arm roots overlap here. Treating the full
        # mark as one even/odd contour cuts an unintended hole at this spot.
        self.assertEqual(coverage[53][59], 16)
        self.assertTrue(all(value <= 16 for row in coverage for value in row))

    def test_android_launcher_references_resolve_at_all_densities(self):
        for package in ("desktop", "phone"):
            base = ROOT / package / "resources/android"
            app = ET.parse(base / "AndroidManifest.xml.template").getroot().find("application")
            self.assertEqual(app.get(ANDROID + "icon"), "@mipmap/ic_launcher", package)
            self.assertEqual(app.get(ANDROID + "roundIcon"), "@mipmap/ic_launcher", package)
            for density, size in (("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)):
                path = base / f"res/mipmap-{density}/ic_launcher.png"
                self.assertTrue(path.is_file(), path)
                self.assertEqual(png_info(path)[:2], (size, size))
            for version, tags in ((26, ("background", "foreground")), (33, ("background", "foreground", "monochrome"))):
                path = base / f"res/mipmap-anydpi-v{version}/ic_launcher.xml"
                self.assertTrue(path.is_file(), path)
                adaptive = ET.parse(path).getroot()
                self.assertEqual(adaptive.tag, "adaptive-icon")
                for tag in tags:
                    ref = adaptive.find(tag).get(ANDROID + "drawable")
                    kind, name = ref.lstrip("@").split("/")
                    self.assertTrue((base / "res" / kind / f"{name}.xml").is_file(), ref)

    def test_ios_catalog_has_correctly_sized_opaque_icons(self):
        for package in ("desktop", "phone"):
            base = ROOT / package / "packaging/ios/icons/Assets.xcassets/AppIcon.appiconset"
            self.assertTrue((base / "Contents.json").is_file(), base)
            images = json.loads((base / "Contents.json").read_text())["images"]
            slots = set()
            for image in images:
                size = float(image["size"].split("x")[0])
                scale = int(image["scale"][:-1])
                pixels = int(size * scale)
                path = base / image["filename"]
                self.assertTrue(path.is_file(), path)
                self.assertEqual(png_info(path), (pixels, pixels, 8, 2, 0, 0, 0), "iOS requires RGB, no alpha")
                slots.add((image["idiom"], size, scale))
            required = {(idiom, size, scale) for idiom, scales in (("iphone", (2, 3)), ("ipad", (1, 2)))
                        for size in (20, 29, 40) for scale in scales}
            required |= {("iphone", 60, 2), ("iphone", 60, 3), ("ipad", 76, 1), ("ipad", 76, 2),
                         ("ipad", 83.5, 2), ("ios-marketing", 1024, 1)}
            self.assertTrue(required <= slots, required - slots)

    def test_desktop_discovery_and_cargo_environment_use_brand_assets(self):
        config = (ROOT / ".cargo/config.toml").read_text()
        for slot in ("32", "64", "128", "256", "512", "1024", "ICO"):
            match = re.search(rf'MAKEPAD_APP_ICON_{slot}\s*=\s*\{{\s*value\s*=\s*"([^"]+)"[^\n]*relative\s*=\s*true', config)
            self.assertIsNotNone(match, f"plain cargo needs MAKEPAD_APP_ICON_{slot}")
            self.assertTrue((ROOT / match[1]).is_file())
        for package in ("desktop", "phone"):
            base = ROOT / package / "resources"
            for size in (32, 64, 128, 256, 512, 1024):
                path = base / f"icon_{size}.png"
                self.assertTrue(path.is_file(), path)
                self.assertEqual(png_info(path)[:2], (size, size))
            data = (base / "icon.icns").read_bytes()
            self.assertEqual(data[:4], b"icns")
            self.assertEqual(struct.unpack(">I", data[4:8])[0], len(data))
            offset, sizes = 8, set()
            while offset < len(data):
                length = struct.unpack(">I", data[offset + 4:offset + 8])[0]
                self.assertGreater(length, 8)
                self.assertEqual(data[offset + 8:offset + 16], b"\x89PNG\r\n\x1a\n")
                sizes.add(struct.unpack(">I", data[offset + 24:offset + 28])[0])
                offset += length
            self.assertEqual(offset, len(data))
            self.assertTrue({32, 128, 256, 512, 1024} <= sizes)
            self.assertEqual((base / "icon.ico").read_bytes(), (ROOT / "desktop/packaging/icons/icon.ico").read_bytes())


if __name__ == "__main__":
    unittest.main()
