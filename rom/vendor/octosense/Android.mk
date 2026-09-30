# OctoSense Home, the prebuilt launcher, re-signed with this ROM's platform
# certificate at build time like the android_app_import modules in Android.bp.
#
# Home's native libraries are installed as files in the app's own lib/arm64 dir
# (LOCAL_PREBUILT_JNI_LIBS; android_app_import cannot install JNI libraries).
# scripts/stage-home.py stages Home without its lib/ entries and puts them in
# prebuilt/lib/arm64, so the package manager takes
# /system_ext/priv-app/OctoSenseHome/lib/arm64 as Home's native library dir.
# Home must find its octos kernel (liboctos.so) there as a file: an app may exec
# only from its native library dir, and a system app's libraries are never
# extracted from its APK. config.fs makes the kernel executable.
LOCAL_PATH := $(call my-dir)

include $(CLEAR_VARS)
LOCAL_MODULE := OctoSenseHome
LOCAL_MODULE_CLASS := APPS
LOCAL_MODULE_SUFFIX := $(COMMON_ANDROID_PACKAGE_SUFFIX)
LOCAL_SRC_FILES := prebuilt/OctoSenseHome.apk
LOCAL_CERTIFICATE := platform
LOCAL_PRIVILEGED_MODULE := true
LOCAL_SYSTEM_EXT_MODULE := true
LOCAL_DEX_PREOPT := false
LOCAL_PREBUILT_JNI_LIBS := \
    prebuilt/lib/arm64/libmakepad.so \
    prebuilt/lib/arm64/liboctos.so
include $(BUILD_PREBUILT)
