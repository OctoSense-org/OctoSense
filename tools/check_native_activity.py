"""Test the exact portable host contract; the SDK adapter is scripted here."""
import hashlib
import json
import os
import pathlib
import platform
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[1]
source = root / "crates/shell/src/native_activity.rs"
target = pathlib.Path(os.environ.get("CARGO_TARGET_DIR", root / "target")) / "native-activity"
target.mkdir(parents=True, exist_ok=True)
binary = target / ("native-activity-tests.exe" if sys.platform == "win32" else "native-activity-tests")
flags = [value for value in os.environ.get("CARGO_ENCODED_RUSTFLAGS", "").split(chr(31)) if value]
subprocess.run(["rustc", "--edition=2024", "--test", str(source), "-o", str(binary), *flags], check=True)
result = subprocess.run([str(binary)], text=True, capture_output=True, encoding="utf-8")
print(result.stdout)
result.check_returncode()
match = re.search(r"test result: ok\. (\d+) passed; 0 failed;", result.stdout)
if not match: raise RuntimeError("Complete passing contract result missing")
report = dict(passed=True, scope="native activity contract with scripted SDK backend", tests_passed=int(match[1]), source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(), os=platform.system(), live_sdk_verified=False, desktop_integration_verified=False)
(target / "result.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
print(json.dumps(report))
