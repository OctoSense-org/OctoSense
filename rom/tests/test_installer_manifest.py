import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("installer_manifest", ROOT / "scripts/make-manifest.py")
manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manifest)


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for part in manifest.PARTITIONS:
            (self.root / f"{part}.img").write_bytes(f"image for {part}".encode())

    def test_complete_release_has_all_digests_sizes_and_fixed_slot_rules(self):
        result = manifest.write_manifest(self.root, "Fixture build")
        self.assertEqual(result, json.loads((self.root / "manifest.json").read_text()))
        self.assertEqual(result["schema"], 1)
        self.assertEqual(result["device"], "enchilada")
        self.assertEqual(result["channel"], "development")
        self.assertEqual([i["partition"] for i in result["images"]], list(manifest.PARTITIONS))
        for image in result["images"]:
            data = (self.root / image["file"]).read_bytes()
            self.assertEqual(image["sha256"], hashlib.sha256(data).hexdigest())
            self.assertEqual(image["size"], len(data))
            self.assertEqual(image["expanded_size"], len(data))
            self.assertEqual(image["slot"], image["partition"] != "vbmeta")

    def test_missing_image_does_not_replace_existing_manifest(self):
        original = manifest.write_manifest(self.root, "Previous complete build")
        (self.root / "boot.img").unlink()
        with self.assertRaisesRegex(ValueError, "Required image missing"):
            manifest.write_manifest(self.root, "Incomplete build")
        self.assertEqual(json.loads((self.root / "manifest.json").read_text()), original)
        self.assertEqual(list(self.root.glob(".manifest-*")), [])

    def test_empty_release_and_empty_image_are_rejected(self):
        with tempfile.TemporaryDirectory() as empty:
            with self.assertRaisesRegex(ValueError, "Required image missing"):
                manifest.write_manifest(empty, "Empty")
            self.assertFalse((Path(empty) / "manifest.json").exists())
        (self.root / "boot.img").write_bytes(b"")
        with self.assertRaisesRegex(ValueError, "Invalid image size"):
            manifest.write_manifest(self.root, "Empty boot")

    def test_sparse_expanded_size_is_recorded_separately(self):
        data = struct.pack("<IHHHHIIII", manifest.SPARSE_MAGIC, 1, 0, 28, 12, 4096, 1000, 1, 0)
        data += struct.pack("<HHII", 0xCAC3, 0, 1000, 12)
        (self.root / "system.img").write_bytes(data)
        image = manifest.build_manifest(self.root, "Sparse")["images"][0]
        self.assertTrue(image["sparse"])
        self.assertEqual(image["expanded_size"], 4096000)
        self.assertEqual(image["size"], len(data))
        self.assertEqual(image["sha256"], hashlib.sha256(data).hexdigest())

    def test_invalid_sparse_header_is_rejected(self):
        for header in [struct.pack("<I", manifest.SPARSE_MAGIC), struct.pack("<IHHHHIIII", manifest.SPARSE_MAGIC, 1, 0, 28, 12, 0, 1000, 1, 0)]:
            (self.root / "system.img").write_bytes(header)
            with self.assertRaises(ValueError):
                manifest.build_manifest(self.root, "Broken sparse")

    def test_shared_chipset_cannot_name_a_release_device(self):
        with self.assertRaisesRegex(ValueError, "not a unique phone identity"):
            manifest.build_manifest(self.root, "Wrong identity", "sdm845")

    def test_malformed_sparse_chunk_tables_are_rejected(self):
        header = struct.pack("<IHHHHIIII", manifest.SPARSE_MAGIC, 1, 0, 28, 12, 4096, 1000, 1, 0)
        for chunk in [b"", struct.pack("<HHII", 0xCAC1, 0, 1000, 12),
                      struct.pack("<HHII", 0xCAC3, 0, 999, 12),
                      struct.pack("<HHII", 0x1234, 0, 1000, 12)]:
            (self.root / "system.img").write_bytes(header + chunk)
            with self.assertRaises(ValueError):
                manifest.build_manifest(self.root, "Broken chunks")

    def test_large_raw_input_requires_sparse_conversion(self):
        with (self.root / "system.img").open("wb") as image:
            image.truncate(manifest.MAX_RAW_BYTES + 1)
        with self.assertRaisesRegex(ValueError, "Convert large raw image"):
            manifest.build_manifest(self.root, "Raw system")


if __name__ == "__main__":
    unittest.main()
