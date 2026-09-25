import codecs
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
JAVA = ROOT / "home/resources/android/java/dev/makepad/octosense/LauncherPlacements.java"
TABLE = ROOT / "home/tests/fixtures/hosted_identities.json"


def java_hosted_pattern():
    """The regular expression LauncherPlacements.isHosted passes to String.matches."""
    source = JAVA.read_text()
    match = re.search(r'boolean isHosted\(String id\) \{\s*return id!=null && id\.matches\("((?:[^"\\]|\\.)*)"\);', source)
    if not match:
        raise AssertionError("LauncherPlacements.isHosted is no longer a single String.matches call")
    return codecs.decode(match[1], "unicode_escape")


class HostedIdentities(unittest.TestCase):
    # The Rust decoder checks the same table (placement_tests in
    # home/src/android_integration.rs), so the two validators stay in step.
    # Java and Python agree on the pattern's syntax; String.matches is fullmatch.
    def test_java_store_accepts_exactly_the_shared_table(self):
        pattern = re.compile(java_hosted_pattern())
        table = json.loads(TABLE.read_text())
        for key, expected in (("valid", True), ("invalid", False)):
            for identity in table[key]:
                with self.subTest(key=key, identity=identity):
                    self.assertEqual(pattern.fullmatch(identity) is not None, expected)


if __name__ == "__main__":
    unittest.main()
