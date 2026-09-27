#!/usr/bin/env python3
"""Stage the separate Recents module into the pinned Lineage Trebuchet tree.

Use --check for a read-only preparation check, or --verify to compare an
already-staged module with the current inputs while the build owns its lock.
Writing requires a passing
upstream baseline build record and the shared platform-build lock. This script
does not build, sign, install, grant permissions or change the phone's Recents.
"""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


REVISION = "dfc9b2347e903ad6930274dafcfdba3e594074ab"
PACKAGE = "dev.makepad.octosense.quickstep"


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError("Pinned source no longer matches: " + old[:90])
    return text.replace(old, new, 1)


def observer_without_home(text):
    text = replace_once(text, "    private final Intent mMyHomeIntent;",
                        "    private final Intent mMyHomeIntent;\n"
                        "    private final boolean mHasIntegratedHome;")
    text = replace_once(text, """        ComponentName myHomeComponent =
                new ComponentName(context.getPackageName(), info.activityInfo.name);
        mMyHomeIntent.setComponent(myHomeComponent);
        mConfigChangesMap.append(myHomeComponent.hashCode(), info.activityInfo.configChanges);""",
                        """        mHasIntegratedHome = info != null && info.activityInfo != null;
        if (mHasIntegratedHome) {
            ComponentName myHomeComponent =
                    new ComponentName(context.getPackageName(), info.activityInfo.name);
            mMyHomeIntent.setComponent(myHomeComponent);
            mConfigChangesMap.append(myHomeComponent.hashCode(), info.activityInfo.configChanges);
        }""")
    text = replace_once(text,
                        "        mIsDefaultHome = Objects.equals(mMyHomeIntent.getComponent(), defaultHome);",
                        "        mIsDefaultHome = mHasIntegratedHome\n"
                        "                && Objects.equals(mMyHomeIntent.getComponent(), defaultHome);")
    text = replace_once(text,
                        "        if (!mIsHomeDisabled && (defaultHome == null || mIsDefaultHome)) {",
                        "        if (mHasIntegratedHome && !mIsHomeDisabled\n"
                        "                && (defaultHome == null || mIsDefaultHome)) {")
    text = replace_once(text,
                        "    boolean canHandleConfigChanges(ComponentName component, int changes) {",
                        "    boolean canHandleConfigChanges(ComponentName component, int changes) {\n"
                        "        if (component == null) {\n"
                        "            return false;\n"
                        "        }")
    return text


def fallback_with_home_geometry(text):
    text = replace_once(text, "import java.util.List;", """import java.util.List;
import java.util.HashMap;
import com.android.systemui.shared.recents.model.ThumbnailData;
import dev.makepad.octosense.contracts.Protocol;
import dev.makepad.octosense.quickstep.HomeIntegrationService;
import dev.makepad.octosense.quickstep.HomeTransitions;""")
    text = replace_once(text, "    private FallbackHomeAnimationFactory mActiveAnimationFactory;",
                        """    private FallbackHomeAnimationFactory mActiveAnimationFactory;
    // Home task appearance clears mActiveAnimationFactory before the spring ends.
    private FallbackHomeAnimationFactory mOctoSenseAnimationFactory;

    private void cancelOctoSenseTransition() {
        if (mOctoSenseAnimationFactory != null) {
            mOctoSenseAnimationFactory.finishOctoSenseTransition(true);
        }
    }

    @Override
    public void onRecentsAnimationCanceled(HashMap<Integer, ThumbnailData> thumbnails) {
        cancelOctoSenseTransition();
        super.onRecentsAnimationCanceled(thumbnails);
    }

    @Override
    public void onRecentsAnimationFinished(@NonNull RecentsAnimationController controller) {
        cancelOctoSenseTransition();
        super.onRecentsAnimationFinished(controller);
    }

    @Override
    public void onConsumerAboutToBeSwitched() {
        cancelOctoSenseTransition();
        super.onConsumerAboutToBeSwitched();
    }""")
    text = replace_once(text, "        mAppCanEnterPip = appCanEnterPip;",
                        "        cancelOctoSenseTransition();\n        mAppCanEnterPip = appCanEnterPip;")
    text = replace_once(text, "        super.onTasksAppeared(appearedTaskTargets);",
                        "        cancelOctoSenseTransition();\n        super.onTasksAppeared(appearedTaskTargets);")
    text = replace_once(text, "        private final RectF mTargetRect = new RectF();",
                        """        private final RectF mTargetRect = new RectF();
        private final RectF mGenericTargetRect = new RectF();
        private HomeTransitions.Return mOctoSenseTransition;
        private HomeTransitions.Target mOctoSenseTarget;
        private int mOctoSenseDisplay;
        private boolean mUsesOctoSense;

        private void refreshOctoSenseTarget() {
            if (!mUsesOctoSense || mGenericTargetRect.isEmpty()) return;
            HomeTransitions.Target next = mOctoSenseTransition == null ? null
                    : mOctoSenseTransition.target(mOctoSenseDisplay,
                            mDeviceState.getRotationTouchHelper().getCurrentActiveRotation());
            if (next == mOctoSenseTarget) return;
            mOctoSenseTarget = next;
            if (next == null) mTargetRect.set(mGenericTargetRect);
            else mTargetRect.set(next.left, next.top, next.right, next.bottom);
            if (mSpringAnim != null) mSpringAnim.onTargetPositionChanged();
        }

        private void finishOctoSenseTransition(boolean cancelled) {
            if (mOctoSenseTransition != null) mOctoSenseTransition.finish(cancelled);
            if (mOctoSenseAnimationFactory == this) mOctoSenseAnimationFactory = null;
        }

        @Override
        public void onCancel() {
            finishOctoSenseTransition(true);
            super.onCancel();
        }""")
    text = replace_once(text, """            if (mTargetRect.isEmpty()) {
                mTargetRect.set(super.getWindowTargetRect());
            }
            return mTargetRect;""", """            if (mGenericTargetRect.isEmpty()) mGenericTargetRect.set(super.getWindowTargetRect());
            if (mTargetRect.isEmpty()) mTargetRect.set(mGenericTargetRect);
            refreshOctoSenseTarget();
            return mTargetRect;""")
    text = replace_once(text, "        private void onRectAnimationEnd() {",
                        "        private void onRectAnimationEnd() {\n            finishOctoSenseTransition(false);")
    text = replace_once(text, "        public void accept(Message msg) {",
                        "        public void accept(Message msg) {\n            if (mUsesOctoSense) return;")
    text = replace_once(text, "        public void update(RectF currentRect, float progress, float radius, int overlayAlpha) {",
                        """        public void update(RectF currentRect, float progress, float radius, int overlayAlpha) {
            refreshOctoSenseTarget();
            if (mOctoSenseTransition != null) mOctoSenseTransition.progress(progress);""")
    text = replace_once(text, "            TaskKey key = new TaskKey(runningTaskInfo);",
                        """            TaskKey key = new TaskKey(runningTaskInfo);
            if (intent.getComponent() != null
                    && Protocol.HOME_PACKAGE.equals(intent.getComponent().getPackageName())) {
                mUsesOctoSense = true;
                // Keep native generic/PiP/split behavior when no single-app target exists.
                if (mRemoteTargetHandles != null && mRemoteTargetHandles.length == 1) {
                    mOctoSenseDisplay = runningTaskInfo.displayId;
                    mOctoSenseTransition = HomeIntegrationService.beginReturn(key.getComponent(),
                            UserHandle.of(key.userId), mOctoSenseDisplay,
                            mDeviceState.getRotationTouchHelper().getCurrentActiveRotation());
                    if (mOctoSenseTransition != null) mOctoSenseAnimationFactory = this;
                }
                return;
            }""")
    return text


def touch_service_with_shade(text):
    text = replace_once(text, "    public void onCreate() {\n        super.onCreate();",
                        "    private dev.makepad.octosense.quickstep.GlobalShadeController mOctoSenseShade;\n\n"
                        "    public void onCreate() {\n        super.onCreate();\n"
                        "        try { mOctoSenseShade = new dev.makepad.octosense.quickstep.GlobalShadeController(this); }\n"
                        "        catch (RuntimeException failure) { Log.w(TAG, \"OctoSense shade unavailable\", failure); }")
    # The original @Override must stay on the method, not the added field.
    text = replace_once(text,
                        "    @Override\n    private dev.makepad.octosense.quickstep.GlobalShadeController mOctoSenseShade;\n\n    public void onCreate()",
                        "    private dev.makepad.octosense.quickstep.GlobalShadeController mOctoSenseShade;\n\n    @Override\n    public void onCreate()")
    text = replace_once(text, "    public void onDestroy() {",
                        "    public void onDestroy() {\n"
                        "        if (mOctoSenseShade != null) { mOctoSenseShade.close(); mOctoSenseShade = null; }")
    text = replace_once(text, "                tis.mDeviceState.setSystemUiFlags(stateFlags);",
                        "                tis.mDeviceState.setSystemUiFlags(stateFlags);\n"
                        "                if (tis.mOctoSenseShade != null) tis.mOctoSenseShade.onSystemUiStateChanged(stateFlags);")
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--baseline-result", type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    tree = args.tree.resolve()
    upstream = tree / "packages/apps/Trebuchet"
    source = Path(__file__).resolve().parents[2]
    inputs = source / "android/platform-build/quickstep"

    def git(*parts):
        return subprocess.check_output(["git", "-C", str(upstream), *parts], text=True)

    if git("rev-parse", "HEAD").strip() != REVISION:
        raise RuntimeError("Trebuchet revision differs from the reviewed source")
    original_bp = git("show", "HEAD:Android.bp")
    observer_path = "quickstep/src/com/android/quickstep/OverviewComponentObserver.java"
    original_observer = git("show", "HEAD:" + observer_path)
    fallback_path = "quickstep/src/com/android/quickstep/FallbackSwipeHandler.java"
    original_fallback = git("show", "HEAD:" + fallback_path)
    touch_path = "quickstep/src/com/android/quickstep/TouchInteractionService.java"
    original_touch = git("show", "HEAD:" + touch_path)
    original_config = git("show", "HEAD:src_build_config/com/android/launcher3/BuildConfig.java")
    config = replace_once(original_config, 'APPLICATION_ID = "com.android.launcher3"',
                          'APPLICATION_ID = "' + PACKAGE + '"')
    fragment = (inputs / "Android.bp.fragment").read_text()
    files = {
        "AndroidManifest.xml": (inputs / "AndroidManifest.xml").read_bytes(),
        "java/com/android/quickstep/OverviewComponentObserver.java": observer_without_home(original_observer).encode(),
        "java/com/android/quickstep/FallbackSwipeHandler.java": fallback_with_home_geometry(original_fallback).encode(),
        "java/com/android/quickstep/TouchInteractionService.java": touch_service_with_shade(original_touch).encode(),
        "java/com/android/launcher3/BuildConfig.java": config.encode(),
    }
    for directory, destination in [
        (source / "android/contracts/src/main/java", "contracts/java"),
        (source / "android/contracts/src/main/aidl", "contracts/aidl"),
        (source / "android/quickstep/src/main/java", "java"),
        (inputs / "java", "java"),
        (source / "android/quickstep/src/main/res", "res"),
        (inputs / "res", "res"),
    ]:
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                name = str(Path(destination) / path.relative_to(directory))
                if name in files:
                    raise RuntimeError("Duplicate staged source: " + name)
                files[name] = path.read_bytes()

    record = {
        "upstream_revision": REVISION,
        "package": PACKAGE,
        "module": "OctoSenseQuickstep",
        "bp_fragment": fragment,
        "bp_fragment_sha256": hashlib.sha256(fragment.encode()).hexdigest(),
        "source_sha256": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()},
        "geometry_controller_adapted": True,
        "geometry_controller_compiled": False,
        "phone_validated": False,
        "native_deployment": False,
    }
    if args.check:
        print(json.dumps({key: value for key, value in record.items() if key != "bp_fragment"}, indent=2))
        return
    if args.baseline_result is None:
        parser.error("Staging/verification requires --baseline-result from the upstream Quickstep build")
    baseline = json.loads(args.baseline_result.read_text())
    if baseline.get("status") != "pass" or baseline.get("command", [])[-1:] != ["quickstep"]:
        raise RuntimeError("Upstream Quickstep baseline has not passed")
    if not baseline.get("source_revisions_match_installed_manifest"):
        raise RuntimeError("Upstream baseline was not verified against the installed ROM sources")
    if baseline.get("trebuchet_revision") != REVISION:
        raise RuntimeError("Upstream baseline used a different Trebuchet revision")
    apk = args.baseline_result.parent / "upstream-TrebuchetQuickStep.apk"
    if not apk.is_file() or hashlib.sha256(apk.read_bytes()).hexdigest() != baseline.get("apk_sha256"):
        raise RuntimeError("Upstream baseline APK is missing or differs from its recorded hash")
    record["upstream_baseline"] = {
        "result_sha256": hashlib.sha256(args.baseline_result.read_bytes()).hexdigest(),
        "apk_sha256": baseline["apk_sha256"],
        "source_manifest_sha256": baseline["source_manifest_sha256"],
    }

    if args.verify:
        # The build supervisor owns the exclusive lock across this check and
        # compilation. Do not take a second conflicting lock in its child.
        destination = upstream / "octosense"
        previous = json.loads((destination / ".octosense-stage.json").read_text())
        if previous != record:
            raise RuntimeError("Staging record differs from current source inputs or upstream baseline")
        actual = {}
        for path in destination.rglob("*"):
            if path.is_symlink():
                raise RuntimeError("Staged source must not contain symlinks: " + str(path))
            if path.is_file() and path.name != ".octosense-stage.json":
                actual[str(path.relative_to(destination))] = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != record["source_sha256"]:
            raise RuntimeError("Staged sources differ from current source inputs")
        if (upstream / "Android.bp").read_text() != original_bp + "\n" + fragment:
            raise RuntimeError("Android.bp differs from the exact OctoSense fragment")
        for relative, original in [(observer_path, original_observer), (fallback_path, original_fallback),
                                   (touch_path, original_touch),
                                   ("src_build_config/com/android/launcher3/BuildConfig.java", original_config)]:
            if (upstream / relative).read_text() != original:
                raise RuntimeError("Original upstream source has changes: " + relative)
        print(json.dumps({"status": "pass", "module": record["module"],
                          "source_sha256": record["source_sha256"],
                          "bp_fragment_sha256": record["bp_fragment_sha256"],
                          "upstream_baseline": record["upstream_baseline"],
                          "native_deployment": False}, indent=2))
        return

    with (tree / ".octosense-build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        destination = upstream / "octosense"
        expected_bp = original_bp
        if destination.exists():
            previous = json.loads((destination / ".octosense-stage.json").read_text())
            actual = {str(p.relative_to(destination)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in destination.rglob("*") if p.is_file() and p.name != ".octosense-stage.json"}
            if actual != previous["source_sha256"]:
                raise RuntimeError("Staged sources have local edits; preserve/review them before restaging")
            expected_bp += "\n" + previous["bp_fragment"]
        if (upstream / "Android.bp").read_text() != expected_bp:
            raise RuntimeError("Android.bp has edits outside the recorded OctoSense fragment")
        if (upstream / observer_path).read_text() != original_observer:
            raise RuntimeError("Original observer has local changes; refusing to ignore them")
        if (upstream / fallback_path).read_text() != original_fallback:
            raise RuntimeError("Original fallback controller has local changes; refusing to ignore them")
        if (upstream / "src_build_config/com/android/launcher3/BuildConfig.java").read_text() != original_config:
            raise RuntimeError("Original BuildConfig has local changes; refusing to ignore them")

        with tempfile.TemporaryDirectory(prefix=".octosense-stage-", dir=upstream.parent) as temporary:
            staged = Path(temporary) / "octosense"
            for name, data in files.items():
                path = staged / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            (staged / ".octosense-stage.json").write_text(json.dumps(record, indent=2) + "\n")
            if destination.exists():
                shutil.rmtree(destination)
            staged.rename(destination)
        (upstream / "Android.bp").write_text(original_bp + "\n" + fragment)
    print(json.dumps({"staged": str(destination), "module": record["module"],
                      "source_files": len(files), "native_deployment": False}, indent=2))


if __name__ == "__main__":
    main()
