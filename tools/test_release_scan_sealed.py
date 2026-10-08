"""One-shot diagnostics must encrypt every finding and fail without plaintext."""
import base64
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("sealed_scan", Path(__file__).with_name("release-scan-sealed.py"))
sealed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sealed)


@unittest.skipUnless(shutil.which("openssl"), "standard OpenSSL required for independent decryption")
class SealedDiagnosticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.private = cls.root / "private.pem"
        subprocess.run(["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:4096",
                        "-out", str(cls.private)], check=True, capture_output=True)
        cls.private.chmod(0o600)
        cls.public = cls.root / "public.pem"
        subprocess.run(["openssl", "pkey", "-in", str(cls.private), "-pubout", "-out", str(cls.public)],
                       check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_findings_are_ciphertext_and_independently_decryptable(self):
        binary = self.root / "synthetic.bin"
        value = b"/home/diagnostic-example/"
        binary.write_bytes(b"prefix " + value + b" file")
        result = sealed.collect(binary, self.public)
        encoded = json.dumps(result).encode()
        self.assertNotIn(value, encoded)
        self.assertNotIn(base64.b64encode(value), encoded)
        self.assertEqual(len(result["records"]), 1)
        ciphertext = base64.b64decode(result["records"][0], validate=True)
        plain = subprocess.run(
            ["openssl", "pkeyutl", "-decrypt", "-inkey", str(self.private),
             "-pkeyopt", "rsa_padding_mode:oaep", "-pkeyopt", "rsa_oaep_md:sha256"],
            input=ciphertext, check=True, capture_output=True,
        ).stdout
        record = json.loads(plain)
        self.assertEqual(base64.b64decode(record["value"]), value)
        self.assertEqual(record["offset"], 7)
        self.assertNotEqual(sealed.collect(binary, self.public)["records"][0], result["records"][0],
                            "OAEP must use fresh randomness")

    def test_encryption_failure_does_not_echo_native_error(self):
        failure = subprocess.CompletedProcess([], 1, b"sensitive", b"sensitive")
        with patch.object(sealed.subprocess, "run", return_value=failure):
            with self.assertRaisesRegex(RuntimeError, "encryption failed") as raised:
                sealed.seal(b"synthetic", self.public)
        self.assertNotIn("sensitive", str(raised.exception))

    def test_oversize_record_is_refused(self):
        with self.assertRaisesRegex(RuntimeError, "encryption budget"):
            sealed.seal(b"x" * 447, self.public)


if __name__ == "__main__":
    unittest.main()
