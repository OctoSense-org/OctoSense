"""Exercise the installed runtime methods, with minimal Android transport doubles.

This covers event ownership and queued full-state authority at IME handoff;
actual Gboard and fast Android input are also tested on the emulator.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
JAVA = ROOT / ".sources/makepad/tools/cargo_makepad/src/android/java/dev/makepad/android"

def method(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]

class AndroidImeHandoffTest(unittest.TestCase):
    def test_real_routing_methods_across_both_ime_handoffs(self):
        if not JAVA.exists():
            self.skipTest("run scripts/setup-home.py to install the pinned Android runtime")
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        activity = (JAVA / "MakepadActivity.java").read_text()
        connection = (JAVA / "MakepadInputConnection.java").read_text()
        harness = (ROOT / "tests/java/AndroidImeHandoffTest.java").read_text()
        harness = harness.replace("/* SURFACE_METHOD */", method(activity, "public boolean onKey(View v, int keyCode, KeyEvent event)"))
        harness = harness.replace("/* CONNECTION_METHODS */", "\n".join(method(connection, signature) for signature in [
            "public boolean sendKeyEvent(KeyEvent event)",
            "private boolean handleKeyEvent(KeyEvent event)",
            "boolean isTextKey(KeyEvent event)",
            "private boolean isNavigationKey(int keyCode)",
        ]))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "AndroidImeHandoffTest.java"
            path.write_text(harness)
            subprocess.run([javac, "-d", temporary, str(path)], check=True, capture_output=True, text=True)
            subprocess.run([java, "-cp", temporary, "AndroidImeHandoffTest"], check=True, capture_output=True, text=True)

    def test_real_android_batch_flushes_editor_updates_before_next_input(self):
        source = ROOT / ".sources/makepad/platform/src/os/linux/android/android.rs"
        rustc = shutil.which("rustc")
        if not source.exists() or not rustc:
            self.skipTest("installed runtime and Rust compiler required")
        source = source.read_text()
        start = source.index("let mut pending_touch_move: Option<FromJavaMessage>")
        end = source.index("self.handle_other_events();", start)
        harness = (ROOT / "tests/rust/AndroidEventBatchTest.rs").read_text()
        harness = harness.replace("/* ANDROID_BATCH */", source[start:end])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "android_batch.rs"
            binary = Path(temporary) / "android_batch"
            path.write_text(harness)
            subprocess.run([rustc, "--edition=2021", str(path), "-o", str(binary)], check=True, capture_output=True, text=True)
            subprocess.run([str(binary)], check=True, capture_output=True, text=True)

    def test_real_editor_setters_sync_once_before_later_input(self):
        source = ROOT / ".sources/makepad/widgets/src/text_input.rs"
        rustc = shutil.which("rustc")
        if not source.exists() or not rustc:
            self.skipTest("installed runtime and Rust compiler required")
        source = source.read_text()
        harness = (ROOT / "tests/rust/AndroidEditorSyncTest.rs").read_text()
        harness = harness.replace("/* REAL_EDITOR_METHODS */", "\n".join(method(source, signature) for signature in [
            "fn set_text(&mut self, cx: &mut Cx, text: &str)",
            "pub fn set_selection(&mut self, cx: &mut Cx, selection: Selection)",
            "fn has_composition(&self)", "fn clear_composition(&mut self)",
            "fn update_ime_context(&mut self, cx: &mut Cx)",
        ]))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "editor_sync.rs"
            binary = Path(temporary) / "editor_sync"
            path.write_text(harness)
            subprocess.run([rustc, "--edition=2021", str(path), "-o", str(binary)], check=True, capture_output=True, text=True)
            subprocess.run([str(binary)], check=True, capture_output=True, text=True)

    def test_real_editor_operations_order_against_programmatic_changes(self):
        runtime = ROOT / ".sources/makepad/platform/src/os/linux/android"
        rustc = shutil.which("rustc")
        if not runtime.exists() or not rustc:
            self.skipTest("installed runtime and Rust compiler required")
        source = (runtime / "android.rs").read_text()
        harness = (ROOT / "tests/rust/AndroidEditorOperationsTest.rs").read_text()
        harness = harness.replace("/* ENGINE_PATH */", str(runtime / "android_ime.rs"))
        harness = harness.replace("/* REAL_EDITOR_METHODS */", "\n".join(method(source, signature) for signature in [
            "fn native_editor_state(", "fn publish_native_editor(", "fn native_editor_operation(",
        ]))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "editor_operations.rs"
            binary = Path(temporary) / "editor_operations"
            path.write_text(harness)
            subprocess.run([rustc, "--edition=2021", str(path), "-o", str(binary)], check=True, capture_output=True, text=True)
            subprocess.run([str(binary)], check=True, capture_output=True, text=True)

    def test_real_android_lifecycle_survives_bootstrap(self):
        runtime = ROOT / ".sources/makepad/platform/src/os/linux/android"
        rustc = shutil.which("rustc")
        if not runtime.exists() or not rustc:
            self.skipTest("installed runtime and Rust compiler required")
        jni = (runtime / "android_jni.rs").read_text()
        host = (runtime / "android.rs").read_text()
        harness = (ROOT / "tests/rust/AndroidLifecycleTest.rs").read_text()
        state_start = jni.index("static ACTIVITY_RESUMED:")
        state_end = jni.index("static MESSAGES_TX:", state_start)
        harness = harness.replace("/* ACTIVITY_STATE */", jni[state_start:state_end])
        harness = harness.replace("/* JNI_CALLBACKS */", "\n".join(method(jni, signature) for signature in [
            "unsafe extern \"C\" fn Java_dev_makepad_android_MakepadNative_activityOnResume(",
            "unsafe extern \"C\" fn Java_dev_makepad_android_MakepadNative_activityOnPause(",
        ]))
        harness = harness.replace("/* RESTORE_AFTER_STARTUP */", method(host, "if android_jni::activity_is_resumed()"))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "lifecycle.rs"
            binary = Path(temporary) / "lifecycle"
            path.write_text(harness)
            subprocess.run([rustc, "--edition=2021", str(path), "-o", str(binary)], check=True, capture_output=True, text=True)
            subprocess.run([str(binary)], check=True, capture_output=True, text=True)

    def test_real_android_dispatch_revalidates_keyboard_dismissal(self):
        runtime = ROOT / ".sources/makepad/platform/src/os/linux/android"
        rustc = shutil.which("rustc")
        if not runtime.exists() or not rustc:
            self.skipTest("installed runtime and Rust compiler required")
        source = (runtime / "android.rs").read_text()
        harness = (ROOT / "tests/rust/AndroidImeVisibilityTest.rs").read_text()
        harness = harness.replace("/* REAL_SHOW_ARM */", method(source, "CxOsOp::ShowTextIME(_area, _pos, config) => unsafe"))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "ime_visibility.rs"
            binary = Path(temporary) / "ime_visibility"
            path.write_text(harness)
            subprocess.run([rustc, "--edition=2021", str(path), "-o", str(binary)], check=True, capture_output=True, text=True)
            subprocess.run([str(binary)], check=True, capture_output=True, text=True)
