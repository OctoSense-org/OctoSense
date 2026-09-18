# OctoSense ROM: the OctoSense launcher, System Bridge and Quickstep as
# platform-signed privileged system apps on top of LineageOS.
# Included from device/oneplus/enchilada/lineage_enchilada.mk by scripts/apply-to-tree.sh.

PRODUCT_PACKAGES += \
    OctoSenseHome \
    OctoSenseBridge \
    OctoSenseAgent \
    OctoSenseFrameworkOverlay

# The native Quickstep fork with the OctoSense panel and Recents, staged into
# packages/apps/Trebuchet by android/platform-build/stage-quickstep.py.
PRODUCT_PACKAGES += OctoSenseQuickstep

PRODUCT_COPY_FILES += \
    vendor/octosense/privapp-permissions-octosense.xml:$(TARGET_COPY_OUT_SYSTEM_EXT)/etc/permissions/privapp-permissions-octosense.xml

# OctoSense is the Home app out of the box: patches/lineage-no-trebuchet.py keeps
# the upstream launcher package out whenever this layer is present.
PRODUCT_PRODUCT_PROPERTIES += \
    ro.octosense.rom=1

# The bench Mac's adb public key is pre-authorised in recovery and in Android
# (/adb_keys), so a freshly wiped phone and the recovery answer adb without a
# tap on the screen. Public keys only; add one file per trusted machine.
PRODUCT_ADB_KEYS := vendor/octosense/adb_keys/bench-mac.pub
