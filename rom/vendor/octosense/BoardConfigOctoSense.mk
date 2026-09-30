# Board-level additions of the OctoSense ROM, included from the device's
# BoardConfig.mk by scripts/apply-to-tree.sh (sepolicy dirs are board variables).
SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += vendor/octosense/sepolicy/private

# File modes (config.fs): Home's octos kernel is executable.
TARGET_FS_CONFIG_GEN += vendor/octosense/config.fs
