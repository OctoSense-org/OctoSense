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

# OctoSense is the Home app out of the box: the upstream launcher package is not
# installed (patches/lineage-no-trebuchet.py honours this variable).
OCTOSENSE_NO_TREBUCHET := true

PRODUCT_PRODUCT_PROPERTIES += \
    ro.octosense.rom=1
