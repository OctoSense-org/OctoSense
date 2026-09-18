#!/bin/bash
# Generates the OctoSense ROM signing keys (platform, APK and APEX) into $KEYS and installs
# them as vendor/lineage-priv/keys in the tree, the layout LineageOS's build reads.
set -euo pipefail
TREE=$HOME/octosense-adr0001/build
KEYS=$HOME/octosense-adr0001/keys
SUBJECT='/C=US/ST=California/L=Santa Clara/O=OctoSense/OU=OctoSense/CN=OctoSense'
mkdir -p "$KEYS"; cd "$KEYS"
gen() { # name bits -> name.pem (PKCS#1 private), name.pk8 (PKCS#8 DER), name.x509.pem (cert)
  local name=$1 bits=$2
  [ -f "$name.pk8" ] && return
  openssl genrsa -out "$name.pem" "$bits" 2>/dev/null
  openssl req -new -x509 -key "$name.pem" -out "$name.x509.pem" -days 10000 -subj "$SUBJECT" -sha256
  openssl pkcs8 -in "$name.pem" -topk8 -outform DER -out "$name.pk8" -nocrypt
}
for k in releasekey platform shared media networkstack testkey bluetooth sdk_sandbox nfc cyngn-priv-app verifiedboot verity; do gen "$k" 4096; done
AVB=$TREE/external/avb/avbtool.py
APEXES=$(grep -rhoE 'name: "com\.android\.[a-z0-9_.]+\.key"' "$TREE/system/apex" "$TREE/packages/modules" 2>/dev/null | sed -E 's/name: "(.*)\.key"/\1/' | sort -u)
for a in $APEXES; do
  gen "$a" 4096
  [ -f "$a.avbpubkey" ] || python3 "$AVB" extract_public_key --key "$a.pem" --output "$a.avbpubkey"
done
chmod 600 *.pem *.pk8
ls | wc -l
mkdir -p "$TREE/vendor/lineage-priv/keys"
cp -f *.pem *.pk8 *.avbpubkey "$TREE/vendor/lineage-priv/keys/"
cat > "$TREE/vendor/lineage-priv/keys/keys.mk" <<'MK'
PRODUCT_DEFAULT_DEV_CERTIFICATE := vendor/lineage-priv/keys/releasekey
MK
cat > "$TREE/vendor/lineage-priv/keys/BUILD.bazel" <<'BZL'
filegroup(
    name = "android_certificate_directory",
    srcs = glob([
        "*.pk8",
        "*.pem",
    ]),
    visibility = ["//visibility:public"],
)
BZL
echo "keys in tree: $(ls $TREE/vendor/lineage-priv/keys | wc -l)"
openssl x509 -in releasekey.x509.pem -noout -subject -fingerprint -sha256 | head -2
