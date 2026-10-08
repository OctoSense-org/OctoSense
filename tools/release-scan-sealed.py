#!/usr/bin/env python3
"""One-shot, encrypted diagnostics for PR361's release privacy finding.

This never changes release-scan's result. Only RSA-OAEP ciphertext leaves the
runner; the diagnostic private key is held outside the checkout. Delete this
helper and the public key once the offending public source has been traced.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys


def seal(payload, public_key):
    # A 4096-bit RSA key with OAEP-SHA256 admits at most 446 bytes. Refuse
    # oversize input rather than truncate the finding or emit plaintext.
    if len(payload) > 446:
        raise RuntimeError("diagnostic record exceeds its encryption budget")
    if sys.platform == "win32":
        script = """
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Security.Cryptography;
public static class OctoSenseDiagnosticSeal {
    public static string Seal(string pem, string payload) {
        using (var rsa = RSA.Create()) {
            rsa.ImportFromPem(pem);
            return Convert.ToBase64String(rsa.Encrypt(Encoding.UTF8.GetBytes(payload), RSAEncryptionPadding.OaepSHA256));
        }
    }
}
'@
[Console]::Out.Write([OctoSenseDiagnosticSeal]::Seal($env:OCTOSENSE_SCAN_DIAGNOSTIC_PUBLIC_KEY, [Console]::In.ReadToEnd()))
"""
        result = subprocess.run(
            ["pwsh", "-NoProfile", "-NonInteractive", "-Command", script],
            input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={**os.environ, "OCTOSENSE_SCAN_DIAGNOSTIC_PUBLIC_KEY": public_key.read_text()},
            timeout=30,
        )
        if result.returncode:
            raise RuntimeError("native RSA diagnostic encryption failed")
        encrypted = base64.b64decode(result.stdout.strip(), validate=True)
    else:
        result = subprocess.run(
            ["openssl", "pkeyutl", "-encrypt", "-pubin", "-inkey", str(public_key),
             "-pkeyopt", "rsa_padding_mode:oaep", "-pkeyopt", "rsa_oaep_md:sha256"],
            input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        if result.returncode:
            raise RuntimeError("RSA diagnostic encryption failed")
        encrypted = result.stdout
    if len(encrypted) != 512:
        raise RuntimeError("diagnostic key is not RSA-4096")
    return base64.b64encode(encrypted).decode("ascii")


def collect(binary, public_key):
    spec = importlib.util.spec_from_file_location("release_scan", Path(__file__).with_name("release-scan.py"))
    scan = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scan)
    data = binary.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    records = []
    for label, pattern in scan.compile_patterns(identity=False):
        seen = set()
        for match in pattern.finditer(data):
            value = match.group()
            if value in seen:
                continue
            if len(records) >= 16 or len(value) > 128:
                raise RuntimeError("finding exceeds bounded diagnostic scope")
            seen.add(value)
            record = {
                "label": label, "offset": match.start(), "binary_sha256": digest,
                "value": base64.b64encode(value).decode("ascii"),
                "before": base64.b64encode(data[max(0, match.start()-24):match.start()]).decode("ascii"),
                "after": base64.b64encode(data[match.end():match.end()+24]).decode("ascii"),
            }
            payload = json.dumps(record, separators=(",", ":")).encode("utf-8")
            records.append(seal(payload, public_key))
            if len(seen) >= 5:
                break
    return {
        "schema": 1, "algorithm": "RSA-OAEP-SHA256",
        "public_key_sha256": hashlib.sha256(public_key.read_bytes()).hexdigest(),
        "records": records,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--public-key", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        sealed = collect(args.binary, args.public_key)
        # No plaintext temporary file, stdout, diagnostic or artifact.
        with args.output.open("x") as out:
            json.dump(sealed, out, indent=2)
            out.write("\n")
    except Exception:
        print("release-scan diagnostic failed; no plaintext was emitted", file=sys.stderr)
        return 1
    print(f"Sealed {len(sealed['records'])} diagnostic records; privacy gate remains failed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
