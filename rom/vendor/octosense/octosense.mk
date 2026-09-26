# OctoSense ROM: the OctoSense launcher, System Bridge and Quickstep as
# platform-signed privileged system apps on top of LineageOS.
# Included from device/oneplus/enchilada/lineage_enchilada.mk by scripts/apply-to-tree.sh.

PRODUCT_PACKAGES += \
    OctoSenseHome \
    OctoSenseBridge \
    OctoSenseAgent \
    OctoSenseSettingsBroker \
    OctoSenseFrameworkOverlay

# The native Quickstep fork with the OctoSense panel and Recents, staged into
# packages/apps/Trebuchet by android/platform-build/stage-quickstep.py.
PRODUCT_PACKAGES += OctoSenseQuickstep

PRODUCT_COPY_FILES += \
    vendor/octosense/privapp-permissions-octosense.xml:$(TARGET_COPY_OUT_SYSTEM_EXT)/etc/permissions/privapp-permissions-octosense.xml \
    vendor/octosense/default-permissions-octosense.xml:$(TARGET_COPY_OUT_SYSTEM_EXT)/etc/default-permissions/default-permissions-octosense.xml

# OctoSense is the Home app out of the box: patches/lineage-no-trebuchet.py keeps
# the upstream launcher package out whenever this layer is present.
PRODUCT_PRODUCT_PROPERTIES += \
    ro.octosense.rom=1

# Bench builds only: a machine's adb public key placed (uncommitted) at
# vendor/octosense/adb_keys/bench.pub is pre-authorised in recovery and Android
# (/adb_keys), so a wiped bench phone answers adb without a tap. Public releases
# are built without it: a published ROM must not trust anyone's computer.
ifneq ($(wildcard vendor/octosense/adb_keys/bench.pub),)
PRODUCT_ADB_KEYS := vendor/octosense/adb_keys/bench.pub
endif
