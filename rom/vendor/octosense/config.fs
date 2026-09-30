# File modes of the OctoSense layer (TARGET_FS_CONFIG_GEN, added by
# BoardConfigOctoSense.mk). Home runs its octos kernel as a child process from
# its native library dir, so the kernel file must be executable. The system_ext
# path also matches system/system_ext/ on a phone without a system_ext partition.

[system_ext/priv-app/OctoSenseHome/lib/arm64/liboctos.so]
mode: 0755
user: AID_ROOT
group: AID_ROOT
caps: 0
