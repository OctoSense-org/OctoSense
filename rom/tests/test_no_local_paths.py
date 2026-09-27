from collections.abc import Iterator
from pathlib import Path
import re
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]

# Paths that only exist on one developer's machine. Each alternative is written
# so that it doesn't match its own line here.
LOCAL_PATH = re.compile(
    r"/Users/[A-Za-z0-9._-]+"  # a macOS home; placeholders such as /Users/<name>/ don't match
    r"|/var/[f]olders/"  # macOS per-user temporary directories
    r"|[A-Za-z]:\\Users\\"  # a Windows home
    r"|~/(?:home|git)/"  # a personal checkout layout
)


def tracked_text_files() -> Iterator[tuple[str, str]]:
    names = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "-z"]).decode().split("\0")
    for name in filter(None, names):
        path = ROOT / name
        if not path.is_file():
            continue
        with path.open("rb") as handle:
            if b"\0" in handle.read(8192):
                continue
        yield name, path.read_text(encoding="utf-8", errors="replace")


class NoLocalPaths(unittest.TestCase):
    def test_tracked_files_have_no_machine_local_paths(self) -> None:
        offenders = [
            f"{name}:{number}: {line.strip()[:160]}"
            for name, text in tracked_text_files()
            for number, line in enumerate(text.splitlines(), 1)
            if LOCAL_PATH.search(line)
        ]
        if offenders:
            self.fail(f"{len(offenders)} machine-local paths in tracked files:\n" + "\n".join(offenders))


if __name__ == "__main__":
    unittest.main()
