"""Build TouchLatency.jar using an existing JDK (Java 8 target) and Android SDK.

No SDK downloads, package installation, device access or app rebuilds.
Push the output jar to the authorized device, then use capture_drawer.py.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--android-jar", type=Path, required=True)
    parser.add_argument("--d8-jar", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory)
        classes, dex = work / "classes", work / "dex"
        classes.mkdir()
        dex.mkdir()
        subprocess.run([
            str(args.java_home / "bin/javac"), "--release", "8", "-classpath",
            str(args.android_jar), "-d", str(classes),
            str(Path(__file__).with_name("TouchLatency.java")),
        ], check=True)
        subprocess.run([
            str(args.java_home / "bin/java"), "-cp", str(args.d8_jar),
            "com.android.tools.r8.D8", "--min-api", "26", "--lib",
            str(args.android_jar), "--output", str(dex), str(classes / "TouchLatency.class"),
        ], check=True)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.output, "w") as jar:
            jar.write(dex / "classes.dex", "classes.dex")
    print(f"{hashlib.sha256(args.output.read_bytes()).hexdigest()}  {args.output}")


if __name__ == "__main__":
    main()
