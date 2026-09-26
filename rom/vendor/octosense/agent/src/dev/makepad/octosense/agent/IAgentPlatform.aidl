package dev.makepad.octosense.agent;

import android.content.Intent;
import android.os.Bundle;

/**
 * The agent platform: privileged powers behind one Binder surface. Every call
 * checks the caller's signature and package (see AgentPlatformService.Caller).
 *
 * Results are Bundles with an "ok" boolean and, on failure, a "reason" code the
 * launcher's result_copy turns into a sentence: "denied", "keyguard",
 * "unavailable", "failed".
 */
interface IAgentPlatform {
    /** Protocol version and the capability set present on this device. */
    Bundle getCapabilities();

    /** tasks: "tasks" = list of Bundles {id, package, activity, label, visible, lastActive}. */
    Bundle getTasks(int max);
    /** tasks: "png" = the task's last snapshot as PNG, or reason "unavailable". */
    Bundle getTaskSnapshot(int taskId, int maxWidth);

    /** screen: "png" = the primary display now, scaled to at most maxWidth px wide; "width", "height". */
    Bundle captureScreen(int maxWidth);

    /** input: refused while the keyguard is showing. */
    Bundle tap(float x, float y);
    Bundle swipe(float x0, float y0, float x1, float y1, int durationMs);
    Bundle typeText(String text);
    Bundle pressKey(int keyCode);

    /** settings: secure/system/global by table name. */
    Bundle getSetting(String table, String name);
    Bundle putSetting(String table, String name, String value);

    /** apps */
    Bundle startActivity(in Intent intent);
    Bundle startTask(int taskId);
    Bundle removeTask(int taskId);
    Bundle forceStop(String packageName);

    /** statusbar */
    Bundle expandNotifications();
    Bundle expandQuickSettings();
    Bundle collapsePanels();

    /** The last calls, newest first: who, what, outcome. */
    Bundle getAuditLog(int max);

    /** update: what the latest GitHub release offers against what runs now
     *  (rom_newer, home_newer, rom_offered, home_offered). Blocking network call. */
    Bundle checkUpdate();
    /** update: start installing "rom", "home" or "all" in the background; poll getUpdateStatus. */
    Bundle applyUpdate(String part);
    /** update: rom_phase, rom_progress, rom_error, home_state, running, slot. */
    Bundle getUpdateStatus();
    /** update: restart into the updated slot once rom_phase is updated_need_reboot. */
    Bundle rebootToUpdate();

    /** Wi-Fi v1: bounded credential-free JSON, current unlocked user only.
     * Append only: older clients retain every existing transaction number. */
    Bundle getWifiSnapshot(long requestId);
    Bundle setWifiEnabled(boolean enabled);
    Bundle scanWifi();
    Bundle wifiNetwork(String observedKey, String action);
    /** Returns only a platform editor Intent; never its credential-bearing result. */
    Bundle wifiConfiguration(String observedKey);
    Bundle getControlsSnapshot(long requestId, String page);
    Bundle setControl(String page, String control, String value);
    /** Bluetooth v1: observed targets, finite actions, no raw device input. */
    Bundle getBluetoothSnapshot(long requestId);
    Bundle setBluetoothEnabled(boolean enabled);
    Bundle scanBluetooth(boolean enabled);
    Bundle setBluetoothName(String name);
    Bundle bluetoothDevice(String observedKey, String action);
    Bundle bluetoothSharing(String observedKey, String kind, String value);
    /** Accounts v1: current-user metadata, observed sync targets, native confirmation. */
    Bundle getAccountsSnapshot(long requestId);
    Bundle getAccountDetails(long requestId, String observedKey);
    Bundle setAccountsMasterSync(boolean enabled);
    Bundle accountSync(String observedKey, String authorityKey, String action, boolean enabled);
    Bundle accountAddition(String providerKey);
    Bundle accountRemoval(String observedKey);
    /** Updates v1: read-only status, explicit check, immutable reviewed install targets. */
    Bundle getUpdatesSnapshot(long requestId);
    Bundle checkReviewedUpdates();
    Bundle installReviewedUpdate(String offerKey,String part);
    Bundle rebootReviewedUpdate(String rebootKey);
    /** Network policy v1: current-user observation, finite controls, no arbitrary keys. */
    Bundle getNetworkSnapshot(long requestId);
    Bundle networkAirplane(String observedKey,boolean enabled);
    Bundle networkDataSaver(String observedKey,boolean enabled);
    Bundle networkPrivateDns(String observedKey,String mode,String hostname);
    /** Date/time v1: exact observed configuration, finite time-detector requests. */
    Bundle getDateTimeSnapshot(long requestId);
    Bundle setDateTime(String observedKey,String action,String value,String occurrence);
    /** Read-only owner history: stable, expiring pages; no notification actions. */
    Bundle getNotificationHistory(long requestId,String snapshotKey,int offset);
    /** Sounds v1: bounded catalogs, observed targets, finite preview/save and unconditional stop. */
    Bundle getSoundsSnapshot(long requestId,String type,String catalogKey,int offset);
    Bundle previewSound(String type,String catalogKey,String targetKey);
    Bundle saveSound(String type,String catalogKey,String targetKey);
    Bundle stopSound();
    /** Display v1: observed default-display choices and finite Night Light fields. */
    Bundle getDisplaySnapshot(long requestId);
    Bundle setDisplaySetting(String observedKey,String setting,String value);
    /** App notifications v1: bounded owner pages and finite observed user changes. */
    Bundle getAppNotifications(long requestId,String packageName,int offset,String generation);
    Bundle setAppNotification(String packageName,String observedKey,String targetKey,String action,String value);
    /** Default roles v1: native qualification and immutable native confirmation, no raw grant. */
    Bundle getRolesSnapshot(long requestId,String role,int offset,String generation);
    Bundle confirmRole(String role,String observedKey,String targetKey);
    /** Common runtime permissions: native observed choices and one-shot platform operation. */
    Bundle getPermissionsSnapshot(long requestId,String packageName,String group,int offset,String generation);
    Bundle requestPermissionChoice(String packageName,String group,String observedKey,String targetKey);
    /** DND policy/time schedules: observed opaque targets, native scheduler, no condition URI. */
    Bundle getDndSnapshot(long requestId,int offset,String generation);
    Bundle setDndPolicy(String key,String field,String value);
    Bundle saveDndSchedule(String key,String target,String name,in int[] days,int startMinute,int endMinute,boolean exitAtAlarm,boolean enabled);
    Bundle setDndRuleEnabled(String key,String target,boolean enabled);
    Bundle deleteDndRule(String key,String target);
    Bundle getAppNetworkSnapshot(long requestId,String packageName);
    Bundle setAppNetwork(String packageName,String key,String field,boolean enabled);
    Bundle getAppBatterySnapshot(long requestId,String packageName);
    Bundle setAppBattery(String packageName,String key,String mode);
    Bundle getAppStorageSnapshot(long requestId,String packageName);
    Bundle applyAppStorage(String packageName,String key,String action);
    /** Per-app language: native-derived opaque catalog/choices, no caller-provided locale tags. */
    Bundle getAppLanguageSnapshot(long requestId,String packageName,String catalogKey,String parentKey,String query,int offset);
    Bundle setAppLanguage(String packageName,String catalogKey,String choiceKey);
    /** Custom captions: process Binder lifetime plus scoped finite editor operations. */
    Bundle getCaptionCustomSnapshot(long requestId,IBinder owner,String session,long visit);
    Bundle setCaptionCustom(IBinder owner,String session,long visit,String field,String value);
    Bundle closeCaptionCustom(IBinder owner,String session,long visit);
    /** Native asset caption languages, selected only through reviewed opaque choices. */
    Bundle getCaptionLanguageSnapshot(long requestId,String catalogKey,String query,int offset);
    Bundle setCaptionLanguage(String catalogKey,String choiceKey);
    /** System language order: native catalog tokens only, one reviewed configuration write. */
    Bundle getSystemLanguagesSnapshot(long requestId,String catalogKey,String parentKey,String query,int offset);
    Bundle applySystemLanguages(String catalogKey,in String[] orderedTargets);
    // Appended keyboard inventory and finite native consent/provider flows (84/85).
    Bundle getKeyboardsSnapshot(long requestId,String query,int offset);
    Bundle prepareKeyboardFlow(long requestId,String key,String target,String operation);
}
