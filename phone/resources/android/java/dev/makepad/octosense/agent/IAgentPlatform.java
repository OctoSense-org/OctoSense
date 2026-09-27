/*
 * This file is auto-generated.  DO NOT MODIFY.
 */
package dev.makepad.octosense.agent;
/**
 * The agent platform: privileged powers behind one Binder surface. Every call
 * checks the caller's signature and package (see AgentPlatformService.Caller).
 * 
 * Results are Bundles with an "ok" boolean and, on failure, a "reason" code the
 * launcher's result_copy turns into a sentence: "denied", "keyguard",
 * "unavailable", "failed".
 */
public interface IAgentPlatform extends android.os.IInterface
{
  /** Default implementation for IAgentPlatform. */
  public static class Default implements dev.makepad.octosense.agent.IAgentPlatform
  {
    /** Protocol version and the capability set present on this device. */
    @Override public android.os.Bundle getCapabilities() throws android.os.RemoteException
    {
      return null;
    }
    /** tasks: "tasks" = list of Bundles {id, package, activity, label, visible, lastActive}. */
    @Override public android.os.Bundle getTasks(int max) throws android.os.RemoteException
    {
      return null;
    }
    /** tasks: "png" = the task's last snapshot as PNG, or reason "unavailable". */
    @Override public android.os.Bundle getTaskSnapshot(int taskId, int maxWidth) throws android.os.RemoteException
    {
      return null;
    }
    /** screen: "png" = the primary display now, scaled to at most maxWidth px wide; "width", "height". */
    @Override public android.os.Bundle captureScreen(int maxWidth) throws android.os.RemoteException
    {
      return null;
    }
    /** input: refused while the keyguard is showing. */
    @Override public android.os.Bundle tap(float x, float y) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle swipe(float x0, float y0, float x1, float y1, int durationMs) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle typeText(java.lang.String text) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle pressKey(int keyCode) throws android.os.RemoteException
    {
      return null;
    }
    /** settings: secure/system/global by table name. */
    @Override public android.os.Bundle getSetting(java.lang.String table, java.lang.String name) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle putSetting(java.lang.String table, java.lang.String name, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    /** apps */
    @Override public android.os.Bundle startActivity(android.content.Intent intent) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle startTask(int taskId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle removeTask(int taskId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle forceStop(java.lang.String packageName) throws android.os.RemoteException
    {
      return null;
    }
    /** statusbar */
    @Override public android.os.Bundle expandNotifications() throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle expandQuickSettings() throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle collapsePanels() throws android.os.RemoteException
    {
      return null;
    }
    /** The last calls, newest first: who, what, outcome. */
    @Override public android.os.Bundle getAuditLog(int max) throws android.os.RemoteException
    {
      return null;
    }
    /**
     * update: what the latest GitHub release offers against what runs now
     *  (rom_newer, home_newer, rom_offered, home_offered). Blocking network call.
     */
    @Override public android.os.Bundle checkUpdate() throws android.os.RemoteException
    {
      return null;
    }
    /** update: start installing "rom", "home" or "all" in the background; poll getUpdateStatus. */
    @Override public android.os.Bundle applyUpdate(java.lang.String part) throws android.os.RemoteException
    {
      return null;
    }
    /** update: rom_phase, rom_progress, rom_error, home_state, running, slot. */
    @Override public android.os.Bundle getUpdateStatus() throws android.os.RemoteException
    {
      return null;
    }
    /** update: restart into the updated slot once rom_phase is updated_need_reboot. */
    @Override public android.os.Bundle rebootToUpdate() throws android.os.RemoteException
    {
      return null;
    }
    /**
     * Wi-Fi v1: bounded credential-free JSON, current unlocked user only.
     * Append only: older clients retain every existing transaction number.
     */
    @Override public android.os.Bundle getWifiSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setWifiEnabled(boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle scanWifi() throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle wifiNetwork(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException
    {
      return null;
    }
    /** Returns only a platform editor Intent; never its credential-bearing result. */
    @Override public android.os.Bundle wifiConfiguration(java.lang.String observedKey) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle getControlsSnapshot(long requestId, java.lang.String page) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setControl(java.lang.String page, java.lang.String control, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    /** Bluetooth v1: observed targets, finite actions, no raw device input. */
    @Override public android.os.Bundle getBluetoothSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setBluetoothEnabled(boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle scanBluetooth(boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setBluetoothName(java.lang.String name) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle bluetoothDevice(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle bluetoothSharing(java.lang.String observedKey, java.lang.String kind, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    /** Accounts v1: current-user metadata, observed sync targets, native confirmation. */
    @Override public android.os.Bundle getAccountsSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle getAccountDetails(long requestId, java.lang.String observedKey) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setAccountsMasterSync(boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle accountSync(java.lang.String observedKey, java.lang.String authorityKey, java.lang.String action, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle accountAddition(java.lang.String providerKey) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle accountRemoval(java.lang.String observedKey) throws android.os.RemoteException
    {
      return null;
    }
    /** Updates v1: read-only status, explicit check, immutable reviewed install targets. */
    @Override public android.os.Bundle getUpdatesSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle checkReviewedUpdates() throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle installReviewedUpdate(java.lang.String offerKey, java.lang.String part) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle rebootReviewedUpdate(java.lang.String rebootKey) throws android.os.RemoteException
    {
      return null;
    }
    /** Network policy v1: current-user observation, finite controls, no arbitrary keys. */
    @Override public android.os.Bundle getNetworkSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle networkAirplane(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle networkDataSaver(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle networkPrivateDns(java.lang.String observedKey, java.lang.String mode, java.lang.String hostname) throws android.os.RemoteException
    {
      return null;
    }
    /** Date/time v1: exact observed configuration, finite time-detector requests. */
    @Override public android.os.Bundle getDateTimeSnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setDateTime(java.lang.String observedKey, java.lang.String action, java.lang.String value, java.lang.String occurrence) throws android.os.RemoteException
    {
      return null;
    }
    /** Read-only owner history: stable, expiring pages; no notification actions. */
    @Override public android.os.Bundle getNotificationHistory(long requestId, java.lang.String snapshotKey, int offset) throws android.os.RemoteException
    {
      return null;
    }
    /** Sounds v1: bounded catalogs, observed targets, finite preview/save and unconditional stop. */
    @Override public android.os.Bundle getSoundsSnapshot(long requestId, java.lang.String type, java.lang.String catalogKey, int offset) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle previewSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle saveSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle stopSound() throws android.os.RemoteException
    {
      return null;
    }
    /** Display v1: observed default-display choices and finite Night Light fields. */
    @Override public android.os.Bundle getDisplaySnapshot(long requestId) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setDisplaySetting(java.lang.String observedKey, java.lang.String setting, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    /** App notifications v1: bounded owner pages and finite observed user changes. */
    @Override public android.os.Bundle getAppNotifications(long requestId, java.lang.String packageName, int offset, java.lang.String generation) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setAppNotification(java.lang.String packageName, java.lang.String observedKey, java.lang.String targetKey, java.lang.String action, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    /** Default roles v1: native qualification and immutable native confirmation, no raw grant. */
    @Override public android.os.Bundle getRolesSnapshot(long requestId, java.lang.String role, int offset, java.lang.String generation) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle confirmRole(java.lang.String role, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException
    {
      return null;
    }
    /** Common runtime permissions: native observed choices and one-shot platform operation. */
    @Override public android.os.Bundle getPermissionsSnapshot(long requestId, java.lang.String packageName, java.lang.String group, int offset, java.lang.String generation) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle requestPermissionChoice(java.lang.String packageName, java.lang.String group, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException
    {
      return null;
    }
    /** DND policy/time schedules: observed opaque targets, native scheduler, no condition URI. */
    @Override public android.os.Bundle getDndSnapshot(long requestId, int offset, java.lang.String generation) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setDndPolicy(java.lang.String key, java.lang.String field, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle saveDndSchedule(java.lang.String key, java.lang.String target, java.lang.String name, int[] days, int startMinute, int endMinute, boolean exitAtAlarm, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setDndRuleEnabled(java.lang.String key, java.lang.String target, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle deleteDndRule(java.lang.String key, java.lang.String target) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle getAppNetworkSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setAppNetwork(java.lang.String packageName, java.lang.String key, java.lang.String field, boolean enabled) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle getAppBatterySnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setAppBattery(java.lang.String packageName, java.lang.String key, java.lang.String mode) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle getAppStorageSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle applyAppStorage(java.lang.String packageName, java.lang.String key, java.lang.String action) throws android.os.RemoteException
    {
      return null;
    }
    /** Per-app language: native-derived opaque catalog/choices, no caller-provided locale tags. */
    @Override public android.os.Bundle getAppLanguageSnapshot(long requestId, java.lang.String packageName, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setAppLanguage(java.lang.String packageName, java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException
    {
      return null;
    }
    /** Custom captions: process Binder lifetime plus scoped finite editor operations. */
    @Override public android.os.Bundle getCaptionCustomSnapshot(long requestId, android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit, java.lang.String field, java.lang.String value) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle closeCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException
    {
      return null;
    }
    /** Native asset caption languages, selected only through reviewed opaque choices. */
    @Override public android.os.Bundle getCaptionLanguageSnapshot(long requestId, java.lang.String catalogKey, java.lang.String query, int offset) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle setCaptionLanguage(java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException
    {
      return null;
    }
    /** System language order: native catalog tokens only, one reviewed configuration write. */
    @Override public android.os.Bundle getSystemLanguagesSnapshot(long requestId, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle applySystemLanguages(java.lang.String catalogKey, java.lang.String[] orderedTargets) throws android.os.RemoteException
    {
      return null;
    }
    // Appended keyboard inventory and finite native consent/provider flows (84/85).
    @Override public android.os.Bundle getKeyboardsSnapshot(long requestId, java.lang.String query, int offset) throws android.os.RemoteException
    {
      return null;
    }
    @Override public android.os.Bundle prepareKeyboardFlow(long requestId, java.lang.String key, java.lang.String target, java.lang.String operation) throws android.os.RemoteException
    {
      return null;
    }
    @Override
    public android.os.IBinder asBinder() {
      return null;
    }
  }
  /** Local-side IPC implementation stub class. */
  public static abstract class Stub extends android.os.Binder implements dev.makepad.octosense.agent.IAgentPlatform
  {
    /** Construct the stub at attach it to the interface. */
    @SuppressWarnings("this-escape")
    public Stub()
    {
      this.attachInterface(this, DESCRIPTOR);
    }
    /**
     * Cast an IBinder object into an dev.makepad.octosense.agent.IAgentPlatform interface,
     * generating a proxy if needed.
     */
    public static dev.makepad.octosense.agent.IAgentPlatform asInterface(android.os.IBinder obj)
    {
      if ((obj==null)) {
        return null;
      }
      android.os.IInterface iin = obj.queryLocalInterface(DESCRIPTOR);
      if (((iin!=null)&&(iin instanceof dev.makepad.octosense.agent.IAgentPlatform))) {
        return ((dev.makepad.octosense.agent.IAgentPlatform)iin);
      }
      return new dev.makepad.octosense.agent.IAgentPlatform.Stub.Proxy(obj);
    }
    @Override public android.os.IBinder asBinder()
    {
      return this;
    }
    @Override public boolean onTransact(int code, android.os.Parcel data, android.os.Parcel reply, int flags) throws android.os.RemoteException
    {
      java.lang.String descriptor = DESCRIPTOR;
      if (code >= android.os.IBinder.FIRST_CALL_TRANSACTION && code <= android.os.IBinder.LAST_CALL_TRANSACTION) {
        data.enforceInterface(descriptor);
      }
      if (code == INTERFACE_TRANSACTION) {
        reply.writeString(descriptor);
        return true;
      }
      switch (code)
      {
        case TRANSACTION_getCapabilities:
        {
          android.os.Bundle _result = this.getCapabilities();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getTasks:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.getTasks(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getTaskSnapshot:
        {
          int _arg0;
          _arg0 = data.readInt();
          int _arg1;
          _arg1 = data.readInt();
          android.os.Bundle _result = this.getTaskSnapshot(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_captureScreen:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.captureScreen(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_tap:
        {
          float _arg0;
          _arg0 = data.readFloat();
          float _arg1;
          _arg1 = data.readFloat();
          android.os.Bundle _result = this.tap(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_swipe:
        {
          float _arg0;
          _arg0 = data.readFloat();
          float _arg1;
          _arg1 = data.readFloat();
          float _arg2;
          _arg2 = data.readFloat();
          float _arg3;
          _arg3 = data.readFloat();
          int _arg4;
          _arg4 = data.readInt();
          android.os.Bundle _result = this.swipe(_arg0, _arg1, _arg2, _arg3, _arg4);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_typeText:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.typeText(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_pressKey:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.pressKey(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getSetting:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getSetting(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_putSetting:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.putSetting(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_startActivity:
        {
          android.content.Intent _arg0;
          _arg0 = _Parcel.readTypedObject(data, android.content.Intent.CREATOR);
          android.os.Bundle _result = this.startActivity(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_startTask:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.startTask(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_removeTask:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.removeTask(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_forceStop:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.forceStop(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_expandNotifications:
        {
          android.os.Bundle _result = this.expandNotifications();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_expandQuickSettings:
        {
          android.os.Bundle _result = this.expandQuickSettings();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_collapsePanels:
        {
          android.os.Bundle _result = this.collapsePanels();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAuditLog:
        {
          int _arg0;
          _arg0 = data.readInt();
          android.os.Bundle _result = this.getAuditLog(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_checkUpdate:
        {
          android.os.Bundle _result = this.checkUpdate();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_applyUpdate:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.applyUpdate(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getUpdateStatus:
        {
          android.os.Bundle _result = this.getUpdateStatus();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_rebootToUpdate:
        {
          android.os.Bundle _result = this.rebootToUpdate();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getWifiSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getWifiSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setWifiEnabled:
        {
          boolean _arg0;
          _arg0 = (0!=data.readInt());
          android.os.Bundle _result = this.setWifiEnabled(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_scanWifi:
        {
          android.os.Bundle _result = this.scanWifi();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_wifiNetwork:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.wifiNetwork(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_wifiConfiguration:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.wifiConfiguration(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getControlsSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getControlsSnapshot(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setControl:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.setControl(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getBluetoothSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getBluetoothSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setBluetoothEnabled:
        {
          boolean _arg0;
          _arg0 = (0!=data.readInt());
          android.os.Bundle _result = this.setBluetoothEnabled(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_scanBluetooth:
        {
          boolean _arg0;
          _arg0 = (0!=data.readInt());
          android.os.Bundle _result = this.scanBluetooth(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setBluetoothName:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.setBluetoothName(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_bluetoothDevice:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.bluetoothDevice(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_bluetoothSharing:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.bluetoothSharing(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAccountsSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getAccountsSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAccountDetails:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getAccountDetails(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setAccountsMasterSync:
        {
          boolean _arg0;
          _arg0 = (0!=data.readInt());
          android.os.Bundle _result = this.setAccountsMasterSync(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_accountSync:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          boolean _arg3;
          _arg3 = (0!=data.readInt());
          android.os.Bundle _result = this.accountSync(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_accountAddition:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.accountAddition(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_accountRemoval:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.accountRemoval(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getUpdatesSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getUpdatesSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_checkReviewedUpdates:
        {
          android.os.Bundle _result = this.checkReviewedUpdates();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_installReviewedUpdate:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.installReviewedUpdate(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_rebootReviewedUpdate:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          android.os.Bundle _result = this.rebootReviewedUpdate(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getNetworkSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getNetworkSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_networkAirplane:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          boolean _arg1;
          _arg1 = (0!=data.readInt());
          android.os.Bundle _result = this.networkAirplane(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_networkDataSaver:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          boolean _arg1;
          _arg1 = (0!=data.readInt());
          android.os.Bundle _result = this.networkDataSaver(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_networkPrivateDns:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.networkPrivateDns(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getDateTimeSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getDateTimeSnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setDateTime:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          android.os.Bundle _result = this.setDateTime(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getNotificationHistory:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          int _arg2;
          _arg2 = data.readInt();
          android.os.Bundle _result = this.getNotificationHistory(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getSoundsSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          int _arg3;
          _arg3 = data.readInt();
          android.os.Bundle _result = this.getSoundsSnapshot(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_previewSound:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.previewSound(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_saveSound:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.saveSound(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_stopSound:
        {
          android.os.Bundle _result = this.stopSound();
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getDisplaySnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.Bundle _result = this.getDisplaySnapshot(_arg0);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setDisplaySetting:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.setDisplaySetting(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAppNotifications:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          int _arg2;
          _arg2 = data.readInt();
          java.lang.String _arg3;
          _arg3 = data.readString();
          android.os.Bundle _result = this.getAppNotifications(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setAppNotification:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          java.lang.String _arg4;
          _arg4 = data.readString();
          android.os.Bundle _result = this.setAppNotification(_arg0, _arg1, _arg2, _arg3, _arg4);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getRolesSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          int _arg2;
          _arg2 = data.readInt();
          java.lang.String _arg3;
          _arg3 = data.readString();
          android.os.Bundle _result = this.getRolesSnapshot(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_confirmRole:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.confirmRole(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getPermissionsSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          int _arg3;
          _arg3 = data.readInt();
          java.lang.String _arg4;
          _arg4 = data.readString();
          android.os.Bundle _result = this.getPermissionsSnapshot(_arg0, _arg1, _arg2, _arg3, _arg4);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_requestPermissionChoice:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          android.os.Bundle _result = this.requestPermissionChoice(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getDndSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          int _arg1;
          _arg1 = data.readInt();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.getDndSnapshot(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setDndPolicy:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.setDndPolicy(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_saveDndSchedule:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          int[] _arg3;
          _arg3 = data.createIntArray();
          int _arg4;
          _arg4 = data.readInt();
          int _arg5;
          _arg5 = data.readInt();
          boolean _arg6;
          _arg6 = (0!=data.readInt());
          boolean _arg7;
          _arg7 = (0!=data.readInt());
          android.os.Bundle _result = this.saveDndSchedule(_arg0, _arg1, _arg2, _arg3, _arg4, _arg5, _arg6, _arg7);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setDndRuleEnabled:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          boolean _arg2;
          _arg2 = (0!=data.readInt());
          android.os.Bundle _result = this.setDndRuleEnabled(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_deleteDndRule:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.deleteDndRule(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAppNetworkSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getAppNetworkSnapshot(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setAppNetwork:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          boolean _arg3;
          _arg3 = (0!=data.readInt());
          android.os.Bundle _result = this.setAppNetwork(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAppBatterySnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getAppBatterySnapshot(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setAppBattery:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.setAppBattery(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAppStorageSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.getAppStorageSnapshot(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_applyAppStorage:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.applyAppStorage(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getAppLanguageSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          java.lang.String _arg4;
          _arg4 = data.readString();
          int _arg5;
          _arg5 = data.readInt();
          android.os.Bundle _result = this.getAppLanguageSnapshot(_arg0, _arg1, _arg2, _arg3, _arg4, _arg5);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setAppLanguage:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          android.os.Bundle _result = this.setAppLanguage(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getCaptionCustomSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          android.os.IBinder _arg1;
          _arg1 = data.readStrongBinder();
          java.lang.String _arg2;
          _arg2 = data.readString();
          long _arg3;
          _arg3 = data.readLong();
          android.os.Bundle _result = this.getCaptionCustomSnapshot(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setCaptionCustom:
        {
          android.os.IBinder _arg0;
          _arg0 = data.readStrongBinder();
          java.lang.String _arg1;
          _arg1 = data.readString();
          long _arg2;
          _arg2 = data.readLong();
          java.lang.String _arg3;
          _arg3 = data.readString();
          java.lang.String _arg4;
          _arg4 = data.readString();
          android.os.Bundle _result = this.setCaptionCustom(_arg0, _arg1, _arg2, _arg3, _arg4);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_closeCaptionCustom:
        {
          android.os.IBinder _arg0;
          _arg0 = data.readStrongBinder();
          java.lang.String _arg1;
          _arg1 = data.readString();
          long _arg2;
          _arg2 = data.readLong();
          android.os.Bundle _result = this.closeCaptionCustom(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getCaptionLanguageSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          int _arg3;
          _arg3 = data.readInt();
          android.os.Bundle _result = this.getCaptionLanguageSnapshot(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_setCaptionLanguage:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String _arg1;
          _arg1 = data.readString();
          android.os.Bundle _result = this.setCaptionLanguage(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getSystemLanguagesSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          int _arg4;
          _arg4 = data.readInt();
          android.os.Bundle _result = this.getSystemLanguagesSnapshot(_arg0, _arg1, _arg2, _arg3, _arg4);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_applySystemLanguages:
        {
          java.lang.String _arg0;
          _arg0 = data.readString();
          java.lang.String[] _arg1;
          _arg1 = data.createStringArray();
          android.os.Bundle _result = this.applySystemLanguages(_arg0, _arg1);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_getKeyboardsSnapshot:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          int _arg2;
          _arg2 = data.readInt();
          android.os.Bundle _result = this.getKeyboardsSnapshot(_arg0, _arg1, _arg2);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        case TRANSACTION_prepareKeyboardFlow:
        {
          long _arg0;
          _arg0 = data.readLong();
          java.lang.String _arg1;
          _arg1 = data.readString();
          java.lang.String _arg2;
          _arg2 = data.readString();
          java.lang.String _arg3;
          _arg3 = data.readString();
          android.os.Bundle _result = this.prepareKeyboardFlow(_arg0, _arg1, _arg2, _arg3);
          reply.writeNoException();
          _Parcel.writeTypedObject(reply, _result, android.os.Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
          break;
        }
        default:
        {
          return super.onTransact(code, data, reply, flags);
        }
      }
      return true;
    }
    private static class Proxy implements dev.makepad.octosense.agent.IAgentPlatform
    {
      private android.os.IBinder mRemote;
      Proxy(android.os.IBinder remote)
      {
        mRemote = remote;
      }
      @Override public android.os.IBinder asBinder()
      {
        return mRemote;
      }
      public java.lang.String getInterfaceDescriptor()
      {
        return DESCRIPTOR;
      }
      /** Protocol version and the capability set present on this device. */
      @Override public android.os.Bundle getCapabilities() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getCapabilities, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** tasks: "tasks" = list of Bundles {id, package, activity, label, visible, lastActive}. */
      @Override public android.os.Bundle getTasks(int max) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(max);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getTasks, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** tasks: "png" = the task's last snapshot as PNG, or reason "unavailable". */
      @Override public android.os.Bundle getTaskSnapshot(int taskId, int maxWidth) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(taskId);
          _data.writeInt(maxWidth);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getTaskSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** screen: "png" = the primary display now, scaled to at most maxWidth px wide; "width", "height". */
      @Override public android.os.Bundle captureScreen(int maxWidth) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(maxWidth);
          boolean _status = mRemote.transact(Stub.TRANSACTION_captureScreen, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** input: refused while the keyguard is showing. */
      @Override public android.os.Bundle tap(float x, float y) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeFloat(x);
          _data.writeFloat(y);
          boolean _status = mRemote.transact(Stub.TRANSACTION_tap, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle swipe(float x0, float y0, float x1, float y1, int durationMs) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeFloat(x0);
          _data.writeFloat(y0);
          _data.writeFloat(x1);
          _data.writeFloat(y1);
          _data.writeInt(durationMs);
          boolean _status = mRemote.transact(Stub.TRANSACTION_swipe, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle typeText(java.lang.String text) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(text);
          boolean _status = mRemote.transact(Stub.TRANSACTION_typeText, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle pressKey(int keyCode) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(keyCode);
          boolean _status = mRemote.transact(Stub.TRANSACTION_pressKey, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** settings: secure/system/global by table name. */
      @Override public android.os.Bundle getSetting(java.lang.String table, java.lang.String name) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(table);
          _data.writeString(name);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getSetting, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle putSetting(java.lang.String table, java.lang.String name, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(table);
          _data.writeString(name);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_putSetting, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** apps */
      @Override public android.os.Bundle startActivity(android.content.Intent intent) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _Parcel.writeTypedObject(_data, intent, 0);
          boolean _status = mRemote.transact(Stub.TRANSACTION_startActivity, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle startTask(int taskId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(taskId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_startTask, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle removeTask(int taskId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(taskId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_removeTask, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle forceStop(java.lang.String packageName) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          boolean _status = mRemote.transact(Stub.TRANSACTION_forceStop, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** statusbar */
      @Override public android.os.Bundle expandNotifications() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_expandNotifications, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle expandQuickSettings() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_expandQuickSettings, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle collapsePanels() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_collapsePanels, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** The last calls, newest first: who, what, outcome. */
      @Override public android.os.Bundle getAuditLog(int max) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(max);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAuditLog, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /**
       * update: what the latest GitHub release offers against what runs now
       *  (rom_newer, home_newer, rom_offered, home_offered). Blocking network call.
       */
      @Override public android.os.Bundle checkUpdate() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_checkUpdate, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** update: start installing "rom", "home" or "all" in the background; poll getUpdateStatus. */
      @Override public android.os.Bundle applyUpdate(java.lang.String part) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(part);
          boolean _status = mRemote.transact(Stub.TRANSACTION_applyUpdate, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** update: rom_phase, rom_progress, rom_error, home_state, running, slot. */
      @Override public android.os.Bundle getUpdateStatus() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getUpdateStatus, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** update: restart into the updated slot once rom_phase is updated_need_reboot. */
      @Override public android.os.Bundle rebootToUpdate() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_rebootToUpdate, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /**
       * Wi-Fi v1: bounded credential-free JSON, current unlocked user only.
       * Append only: older clients retain every existing transaction number.
       */
      @Override public android.os.Bundle getWifiSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getWifiSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setWifiEnabled(boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_setWifiEnabled, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle scanWifi() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_scanWifi, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle wifiNetwork(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(action);
          boolean _status = mRemote.transact(Stub.TRANSACTION_wifiNetwork, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Returns only a platform editor Intent; never its credential-bearing result. */
      @Override public android.os.Bundle wifiConfiguration(java.lang.String observedKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_wifiConfiguration, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle getControlsSnapshot(long requestId, java.lang.String page) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(page);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getControlsSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setControl(java.lang.String page, java.lang.String control, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(page);
          _data.writeString(control);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setControl, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Bluetooth v1: observed targets, finite actions, no raw device input. */
      @Override public android.os.Bundle getBluetoothSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getBluetoothSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setBluetoothEnabled(boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_setBluetoothEnabled, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle scanBluetooth(boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_scanBluetooth, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setBluetoothName(java.lang.String name) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(name);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setBluetoothName, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle bluetoothDevice(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(action);
          boolean _status = mRemote.transact(Stub.TRANSACTION_bluetoothDevice, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle bluetoothSharing(java.lang.String observedKey, java.lang.String kind, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(kind);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_bluetoothSharing, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Accounts v1: current-user metadata, observed sync targets, native confirmation. */
      @Override public android.os.Bundle getAccountsSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAccountsSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle getAccountDetails(long requestId, java.lang.String observedKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(observedKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAccountDetails, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setAccountsMasterSync(boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_setAccountsMasterSync, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle accountSync(java.lang.String observedKey, java.lang.String authorityKey, java.lang.String action, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(authorityKey);
          _data.writeString(action);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_accountSync, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle accountAddition(java.lang.String providerKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(providerKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_accountAddition, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle accountRemoval(java.lang.String observedKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_accountRemoval, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Updates v1: read-only status, explicit check, immutable reviewed install targets. */
      @Override public android.os.Bundle getUpdatesSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getUpdatesSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle checkReviewedUpdates() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_checkReviewedUpdates, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle installReviewedUpdate(java.lang.String offerKey, java.lang.String part) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(offerKey);
          _data.writeString(part);
          boolean _status = mRemote.transact(Stub.TRANSACTION_installReviewedUpdate, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle rebootReviewedUpdate(java.lang.String rebootKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(rebootKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_rebootReviewedUpdate, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Network policy v1: current-user observation, finite controls, no arbitrary keys. */
      @Override public android.os.Bundle getNetworkSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getNetworkSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle networkAirplane(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_networkAirplane, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle networkDataSaver(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_networkDataSaver, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle networkPrivateDns(java.lang.String observedKey, java.lang.String mode, java.lang.String hostname) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(mode);
          _data.writeString(hostname);
          boolean _status = mRemote.transact(Stub.TRANSACTION_networkPrivateDns, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Date/time v1: exact observed configuration, finite time-detector requests. */
      @Override public android.os.Bundle getDateTimeSnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getDateTimeSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setDateTime(java.lang.String observedKey, java.lang.String action, java.lang.String value, java.lang.String occurrence) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(action);
          _data.writeString(value);
          _data.writeString(occurrence);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setDateTime, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Read-only owner history: stable, expiring pages; no notification actions. */
      @Override public android.os.Bundle getNotificationHistory(long requestId, java.lang.String snapshotKey, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(snapshotKey);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getNotificationHistory, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Sounds v1: bounded catalogs, observed targets, finite preview/save and unconditional stop. */
      @Override public android.os.Bundle getSoundsSnapshot(long requestId, java.lang.String type, java.lang.String catalogKey, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(type);
          _data.writeString(catalogKey);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getSoundsSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle previewSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(type);
          _data.writeString(catalogKey);
          _data.writeString(targetKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_previewSound, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle saveSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(type);
          _data.writeString(catalogKey);
          _data.writeString(targetKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_saveSound, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle stopSound() throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          boolean _status = mRemote.transact(Stub.TRANSACTION_stopSound, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Display v1: observed default-display choices and finite Night Light fields. */
      @Override public android.os.Bundle getDisplaySnapshot(long requestId) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getDisplaySnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setDisplaySetting(java.lang.String observedKey, java.lang.String setting, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(observedKey);
          _data.writeString(setting);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setDisplaySetting, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** App notifications v1: bounded owner pages and finite observed user changes. */
      @Override public android.os.Bundle getAppNotifications(long requestId, java.lang.String packageName, int offset, java.lang.String generation) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          _data.writeInt(offset);
          _data.writeString(generation);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAppNotifications, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setAppNotification(java.lang.String packageName, java.lang.String observedKey, java.lang.String targetKey, java.lang.String action, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(observedKey);
          _data.writeString(targetKey);
          _data.writeString(action);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setAppNotification, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Default roles v1: native qualification and immutable native confirmation, no raw grant. */
      @Override public android.os.Bundle getRolesSnapshot(long requestId, java.lang.String role, int offset, java.lang.String generation) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(role);
          _data.writeInt(offset);
          _data.writeString(generation);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getRolesSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle confirmRole(java.lang.String role, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(role);
          _data.writeString(observedKey);
          _data.writeString(targetKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_confirmRole, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Common runtime permissions: native observed choices and one-shot platform operation. */
      @Override public android.os.Bundle getPermissionsSnapshot(long requestId, java.lang.String packageName, java.lang.String group, int offset, java.lang.String generation) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          _data.writeString(group);
          _data.writeInt(offset);
          _data.writeString(generation);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getPermissionsSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle requestPermissionChoice(java.lang.String packageName, java.lang.String group, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(group);
          _data.writeString(observedKey);
          _data.writeString(targetKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_requestPermissionChoice, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** DND policy/time schedules: observed opaque targets, native scheduler, no condition URI. */
      @Override public android.os.Bundle getDndSnapshot(long requestId, int offset, java.lang.String generation) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeInt(offset);
          _data.writeString(generation);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getDndSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setDndPolicy(java.lang.String key, java.lang.String field, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(key);
          _data.writeString(field);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setDndPolicy, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle saveDndSchedule(java.lang.String key, java.lang.String target, java.lang.String name, int[] days, int startMinute, int endMinute, boolean exitAtAlarm, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(key);
          _data.writeString(target);
          _data.writeString(name);
          _data.writeIntArray(days);
          _data.writeInt(startMinute);
          _data.writeInt(endMinute);
          _data.writeInt(((exitAtAlarm)?(1):(0)));
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_saveDndSchedule, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setDndRuleEnabled(java.lang.String key, java.lang.String target, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(key);
          _data.writeString(target);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_setDndRuleEnabled, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle deleteDndRule(java.lang.String key, java.lang.String target) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(key);
          _data.writeString(target);
          boolean _status = mRemote.transact(Stub.TRANSACTION_deleteDndRule, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle getAppNetworkSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAppNetworkSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setAppNetwork(java.lang.String packageName, java.lang.String key, java.lang.String field, boolean enabled) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(key);
          _data.writeString(field);
          _data.writeInt(((enabled)?(1):(0)));
          boolean _status = mRemote.transact(Stub.TRANSACTION_setAppNetwork, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle getAppBatterySnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAppBatterySnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setAppBattery(java.lang.String packageName, java.lang.String key, java.lang.String mode) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(key);
          _data.writeString(mode);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setAppBattery, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle getAppStorageSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAppStorageSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle applyAppStorage(java.lang.String packageName, java.lang.String key, java.lang.String action) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(key);
          _data.writeString(action);
          boolean _status = mRemote.transact(Stub.TRANSACTION_applyAppStorage, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Per-app language: native-derived opaque catalog/choices, no caller-provided locale tags. */
      @Override public android.os.Bundle getAppLanguageSnapshot(long requestId, java.lang.String packageName, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(packageName);
          _data.writeString(catalogKey);
          _data.writeString(parentKey);
          _data.writeString(query);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getAppLanguageSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setAppLanguage(java.lang.String packageName, java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(packageName);
          _data.writeString(catalogKey);
          _data.writeString(choiceKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setAppLanguage, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Custom captions: process Binder lifetime plus scoped finite editor operations. */
      @Override public android.os.Bundle getCaptionCustomSnapshot(long requestId, android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeStrongBinder(owner);
          _data.writeString(session);
          _data.writeLong(visit);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getCaptionCustomSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit, java.lang.String field, java.lang.String value) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeStrongBinder(owner);
          _data.writeString(session);
          _data.writeLong(visit);
          _data.writeString(field);
          _data.writeString(value);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setCaptionCustom, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle closeCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeStrongBinder(owner);
          _data.writeString(session);
          _data.writeLong(visit);
          boolean _status = mRemote.transact(Stub.TRANSACTION_closeCaptionCustom, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** Native asset caption languages, selected only through reviewed opaque choices. */
      @Override public android.os.Bundle getCaptionLanguageSnapshot(long requestId, java.lang.String catalogKey, java.lang.String query, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(catalogKey);
          _data.writeString(query);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getCaptionLanguageSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle setCaptionLanguage(java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(catalogKey);
          _data.writeString(choiceKey);
          boolean _status = mRemote.transact(Stub.TRANSACTION_setCaptionLanguage, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      /** System language order: native catalog tokens only, one reviewed configuration write. */
      @Override public android.os.Bundle getSystemLanguagesSnapshot(long requestId, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(catalogKey);
          _data.writeString(parentKey);
          _data.writeString(query);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getSystemLanguagesSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle applySystemLanguages(java.lang.String catalogKey, java.lang.String[] orderedTargets) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeString(catalogKey);
          _data.writeStringArray(orderedTargets);
          boolean _status = mRemote.transact(Stub.TRANSACTION_applySystemLanguages, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      // Appended keyboard inventory and finite native consent/provider flows (84/85).
      @Override public android.os.Bundle getKeyboardsSnapshot(long requestId, java.lang.String query, int offset) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(query);
          _data.writeInt(offset);
          boolean _status = mRemote.transact(Stub.TRANSACTION_getKeyboardsSnapshot, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
      @Override public android.os.Bundle prepareKeyboardFlow(long requestId, java.lang.String key, java.lang.String target, java.lang.String operation) throws android.os.RemoteException
      {
        android.os.Parcel _data = android.os.Parcel.obtain();
        android.os.Parcel _reply = android.os.Parcel.obtain();
        android.os.Bundle _result;
        try {
          _data.writeInterfaceToken(DESCRIPTOR);
          _data.writeLong(requestId);
          _data.writeString(key);
          _data.writeString(target);
          _data.writeString(operation);
          boolean _status = mRemote.transact(Stub.TRANSACTION_prepareKeyboardFlow, _data, _reply, 0);
          _reply.readException();
          _result = _Parcel.readTypedObject(_reply, android.os.Bundle.CREATOR);
        }
        finally {
          _reply.recycle();
          _data.recycle();
        }
        return _result;
      }
    }
    static final int TRANSACTION_getCapabilities = (android.os.IBinder.FIRST_CALL_TRANSACTION + 0);
    static final int TRANSACTION_getTasks = (android.os.IBinder.FIRST_CALL_TRANSACTION + 1);
    static final int TRANSACTION_getTaskSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 2);
    static final int TRANSACTION_captureScreen = (android.os.IBinder.FIRST_CALL_TRANSACTION + 3);
    static final int TRANSACTION_tap = (android.os.IBinder.FIRST_CALL_TRANSACTION + 4);
    static final int TRANSACTION_swipe = (android.os.IBinder.FIRST_CALL_TRANSACTION + 5);
    static final int TRANSACTION_typeText = (android.os.IBinder.FIRST_CALL_TRANSACTION + 6);
    static final int TRANSACTION_pressKey = (android.os.IBinder.FIRST_CALL_TRANSACTION + 7);
    static final int TRANSACTION_getSetting = (android.os.IBinder.FIRST_CALL_TRANSACTION + 8);
    static final int TRANSACTION_putSetting = (android.os.IBinder.FIRST_CALL_TRANSACTION + 9);
    static final int TRANSACTION_startActivity = (android.os.IBinder.FIRST_CALL_TRANSACTION + 10);
    static final int TRANSACTION_startTask = (android.os.IBinder.FIRST_CALL_TRANSACTION + 11);
    static final int TRANSACTION_removeTask = (android.os.IBinder.FIRST_CALL_TRANSACTION + 12);
    static final int TRANSACTION_forceStop = (android.os.IBinder.FIRST_CALL_TRANSACTION + 13);
    static final int TRANSACTION_expandNotifications = (android.os.IBinder.FIRST_CALL_TRANSACTION + 14);
    static final int TRANSACTION_expandQuickSettings = (android.os.IBinder.FIRST_CALL_TRANSACTION + 15);
    static final int TRANSACTION_collapsePanels = (android.os.IBinder.FIRST_CALL_TRANSACTION + 16);
    static final int TRANSACTION_getAuditLog = (android.os.IBinder.FIRST_CALL_TRANSACTION + 17);
    static final int TRANSACTION_checkUpdate = (android.os.IBinder.FIRST_CALL_TRANSACTION + 18);
    static final int TRANSACTION_applyUpdate = (android.os.IBinder.FIRST_CALL_TRANSACTION + 19);
    static final int TRANSACTION_getUpdateStatus = (android.os.IBinder.FIRST_CALL_TRANSACTION + 20);
    static final int TRANSACTION_rebootToUpdate = (android.os.IBinder.FIRST_CALL_TRANSACTION + 21);
    static final int TRANSACTION_getWifiSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 22);
    static final int TRANSACTION_setWifiEnabled = (android.os.IBinder.FIRST_CALL_TRANSACTION + 23);
    static final int TRANSACTION_scanWifi = (android.os.IBinder.FIRST_CALL_TRANSACTION + 24);
    static final int TRANSACTION_wifiNetwork = (android.os.IBinder.FIRST_CALL_TRANSACTION + 25);
    static final int TRANSACTION_wifiConfiguration = (android.os.IBinder.FIRST_CALL_TRANSACTION + 26);
    static final int TRANSACTION_getControlsSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 27);
    static final int TRANSACTION_setControl = (android.os.IBinder.FIRST_CALL_TRANSACTION + 28);
    static final int TRANSACTION_getBluetoothSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 29);
    static final int TRANSACTION_setBluetoothEnabled = (android.os.IBinder.FIRST_CALL_TRANSACTION + 30);
    static final int TRANSACTION_scanBluetooth = (android.os.IBinder.FIRST_CALL_TRANSACTION + 31);
    static final int TRANSACTION_setBluetoothName = (android.os.IBinder.FIRST_CALL_TRANSACTION + 32);
    static final int TRANSACTION_bluetoothDevice = (android.os.IBinder.FIRST_CALL_TRANSACTION + 33);
    static final int TRANSACTION_bluetoothSharing = (android.os.IBinder.FIRST_CALL_TRANSACTION + 34);
    static final int TRANSACTION_getAccountsSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 35);
    static final int TRANSACTION_getAccountDetails = (android.os.IBinder.FIRST_CALL_TRANSACTION + 36);
    static final int TRANSACTION_setAccountsMasterSync = (android.os.IBinder.FIRST_CALL_TRANSACTION + 37);
    static final int TRANSACTION_accountSync = (android.os.IBinder.FIRST_CALL_TRANSACTION + 38);
    static final int TRANSACTION_accountAddition = (android.os.IBinder.FIRST_CALL_TRANSACTION + 39);
    static final int TRANSACTION_accountRemoval = (android.os.IBinder.FIRST_CALL_TRANSACTION + 40);
    static final int TRANSACTION_getUpdatesSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 41);
    static final int TRANSACTION_checkReviewedUpdates = (android.os.IBinder.FIRST_CALL_TRANSACTION + 42);
    static final int TRANSACTION_installReviewedUpdate = (android.os.IBinder.FIRST_CALL_TRANSACTION + 43);
    static final int TRANSACTION_rebootReviewedUpdate = (android.os.IBinder.FIRST_CALL_TRANSACTION + 44);
    static final int TRANSACTION_getNetworkSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 45);
    static final int TRANSACTION_networkAirplane = (android.os.IBinder.FIRST_CALL_TRANSACTION + 46);
    static final int TRANSACTION_networkDataSaver = (android.os.IBinder.FIRST_CALL_TRANSACTION + 47);
    static final int TRANSACTION_networkPrivateDns = (android.os.IBinder.FIRST_CALL_TRANSACTION + 48);
    static final int TRANSACTION_getDateTimeSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 49);
    static final int TRANSACTION_setDateTime = (android.os.IBinder.FIRST_CALL_TRANSACTION + 50);
    static final int TRANSACTION_getNotificationHistory = (android.os.IBinder.FIRST_CALL_TRANSACTION + 51);
    static final int TRANSACTION_getSoundsSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 52);
    static final int TRANSACTION_previewSound = (android.os.IBinder.FIRST_CALL_TRANSACTION + 53);
    static final int TRANSACTION_saveSound = (android.os.IBinder.FIRST_CALL_TRANSACTION + 54);
    static final int TRANSACTION_stopSound = (android.os.IBinder.FIRST_CALL_TRANSACTION + 55);
    static final int TRANSACTION_getDisplaySnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 56);
    static final int TRANSACTION_setDisplaySetting = (android.os.IBinder.FIRST_CALL_TRANSACTION + 57);
    static final int TRANSACTION_getAppNotifications = (android.os.IBinder.FIRST_CALL_TRANSACTION + 58);
    static final int TRANSACTION_setAppNotification = (android.os.IBinder.FIRST_CALL_TRANSACTION + 59);
    static final int TRANSACTION_getRolesSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 60);
    static final int TRANSACTION_confirmRole = (android.os.IBinder.FIRST_CALL_TRANSACTION + 61);
    static final int TRANSACTION_getPermissionsSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 62);
    static final int TRANSACTION_requestPermissionChoice = (android.os.IBinder.FIRST_CALL_TRANSACTION + 63);
    static final int TRANSACTION_getDndSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 64);
    static final int TRANSACTION_setDndPolicy = (android.os.IBinder.FIRST_CALL_TRANSACTION + 65);
    static final int TRANSACTION_saveDndSchedule = (android.os.IBinder.FIRST_CALL_TRANSACTION + 66);
    static final int TRANSACTION_setDndRuleEnabled = (android.os.IBinder.FIRST_CALL_TRANSACTION + 67);
    static final int TRANSACTION_deleteDndRule = (android.os.IBinder.FIRST_CALL_TRANSACTION + 68);
    static final int TRANSACTION_getAppNetworkSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 69);
    static final int TRANSACTION_setAppNetwork = (android.os.IBinder.FIRST_CALL_TRANSACTION + 70);
    static final int TRANSACTION_getAppBatterySnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 71);
    static final int TRANSACTION_setAppBattery = (android.os.IBinder.FIRST_CALL_TRANSACTION + 72);
    static final int TRANSACTION_getAppStorageSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 73);
    static final int TRANSACTION_applyAppStorage = (android.os.IBinder.FIRST_CALL_TRANSACTION + 74);
    static final int TRANSACTION_getAppLanguageSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 75);
    static final int TRANSACTION_setAppLanguage = (android.os.IBinder.FIRST_CALL_TRANSACTION + 76);
    static final int TRANSACTION_getCaptionCustomSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 77);
    static final int TRANSACTION_setCaptionCustom = (android.os.IBinder.FIRST_CALL_TRANSACTION + 78);
    static final int TRANSACTION_closeCaptionCustom = (android.os.IBinder.FIRST_CALL_TRANSACTION + 79);
    static final int TRANSACTION_getCaptionLanguageSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 80);
    static final int TRANSACTION_setCaptionLanguage = (android.os.IBinder.FIRST_CALL_TRANSACTION + 81);
    static final int TRANSACTION_getSystemLanguagesSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 82);
    static final int TRANSACTION_applySystemLanguages = (android.os.IBinder.FIRST_CALL_TRANSACTION + 83);
    static final int TRANSACTION_getKeyboardsSnapshot = (android.os.IBinder.FIRST_CALL_TRANSACTION + 84);
    static final int TRANSACTION_prepareKeyboardFlow = (android.os.IBinder.FIRST_CALL_TRANSACTION + 85);
  }
  /** @hide */
  public static final java.lang.String DESCRIPTOR = "dev.makepad.octosense.agent.IAgentPlatform";
  /** Protocol version and the capability set present on this device. */
  public android.os.Bundle getCapabilities() throws android.os.RemoteException;
  /** tasks: "tasks" = list of Bundles {id, package, activity, label, visible, lastActive}. */
  public android.os.Bundle getTasks(int max) throws android.os.RemoteException;
  /** tasks: "png" = the task's last snapshot as PNG, or reason "unavailable". */
  public android.os.Bundle getTaskSnapshot(int taskId, int maxWidth) throws android.os.RemoteException;
  /** screen: "png" = the primary display now, scaled to at most maxWidth px wide; "width", "height". */
  public android.os.Bundle captureScreen(int maxWidth) throws android.os.RemoteException;
  /** input: refused while the keyguard is showing. */
  public android.os.Bundle tap(float x, float y) throws android.os.RemoteException;
  public android.os.Bundle swipe(float x0, float y0, float x1, float y1, int durationMs) throws android.os.RemoteException;
  public android.os.Bundle typeText(java.lang.String text) throws android.os.RemoteException;
  public android.os.Bundle pressKey(int keyCode) throws android.os.RemoteException;
  /** settings: secure/system/global by table name. */
  public android.os.Bundle getSetting(java.lang.String table, java.lang.String name) throws android.os.RemoteException;
  public android.os.Bundle putSetting(java.lang.String table, java.lang.String name, java.lang.String value) throws android.os.RemoteException;
  /** apps */
  public android.os.Bundle startActivity(android.content.Intent intent) throws android.os.RemoteException;
  public android.os.Bundle startTask(int taskId) throws android.os.RemoteException;
  public android.os.Bundle removeTask(int taskId) throws android.os.RemoteException;
  public android.os.Bundle forceStop(java.lang.String packageName) throws android.os.RemoteException;
  /** statusbar */
  public android.os.Bundle expandNotifications() throws android.os.RemoteException;
  public android.os.Bundle expandQuickSettings() throws android.os.RemoteException;
  public android.os.Bundle collapsePanels() throws android.os.RemoteException;
  /** The last calls, newest first: who, what, outcome. */
  public android.os.Bundle getAuditLog(int max) throws android.os.RemoteException;
  /**
   * update: what the latest GitHub release offers against what runs now
   *  (rom_newer, home_newer, rom_offered, home_offered). Blocking network call.
   */
  public android.os.Bundle checkUpdate() throws android.os.RemoteException;
  /** update: start installing "rom", "home" or "all" in the background; poll getUpdateStatus. */
  public android.os.Bundle applyUpdate(java.lang.String part) throws android.os.RemoteException;
  /** update: rom_phase, rom_progress, rom_error, home_state, running, slot. */
  public android.os.Bundle getUpdateStatus() throws android.os.RemoteException;
  /** update: restart into the updated slot once rom_phase is updated_need_reboot. */
  public android.os.Bundle rebootToUpdate() throws android.os.RemoteException;
  /**
   * Wi-Fi v1: bounded credential-free JSON, current unlocked user only.
   * Append only: older clients retain every existing transaction number.
   */
  public android.os.Bundle getWifiSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle setWifiEnabled(boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle scanWifi() throws android.os.RemoteException;
  public android.os.Bundle wifiNetwork(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException;
  /** Returns only a platform editor Intent; never its credential-bearing result. */
  public android.os.Bundle wifiConfiguration(java.lang.String observedKey) throws android.os.RemoteException;
  public android.os.Bundle getControlsSnapshot(long requestId, java.lang.String page) throws android.os.RemoteException;
  public android.os.Bundle setControl(java.lang.String page, java.lang.String control, java.lang.String value) throws android.os.RemoteException;
  /** Bluetooth v1: observed targets, finite actions, no raw device input. */
  public android.os.Bundle getBluetoothSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle setBluetoothEnabled(boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle scanBluetooth(boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle setBluetoothName(java.lang.String name) throws android.os.RemoteException;
  public android.os.Bundle bluetoothDevice(java.lang.String observedKey, java.lang.String action) throws android.os.RemoteException;
  public android.os.Bundle bluetoothSharing(java.lang.String observedKey, java.lang.String kind, java.lang.String value) throws android.os.RemoteException;
  /** Accounts v1: current-user metadata, observed sync targets, native confirmation. */
  public android.os.Bundle getAccountsSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle getAccountDetails(long requestId, java.lang.String observedKey) throws android.os.RemoteException;
  public android.os.Bundle setAccountsMasterSync(boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle accountSync(java.lang.String observedKey, java.lang.String authorityKey, java.lang.String action, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle accountAddition(java.lang.String providerKey) throws android.os.RemoteException;
  public android.os.Bundle accountRemoval(java.lang.String observedKey) throws android.os.RemoteException;
  /** Updates v1: read-only status, explicit check, immutable reviewed install targets. */
  public android.os.Bundle getUpdatesSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle checkReviewedUpdates() throws android.os.RemoteException;
  public android.os.Bundle installReviewedUpdate(java.lang.String offerKey, java.lang.String part) throws android.os.RemoteException;
  public android.os.Bundle rebootReviewedUpdate(java.lang.String rebootKey) throws android.os.RemoteException;
  /** Network policy v1: current-user observation, finite controls, no arbitrary keys. */
  public android.os.Bundle getNetworkSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle networkAirplane(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle networkDataSaver(java.lang.String observedKey, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle networkPrivateDns(java.lang.String observedKey, java.lang.String mode, java.lang.String hostname) throws android.os.RemoteException;
  /** Date/time v1: exact observed configuration, finite time-detector requests. */
  public android.os.Bundle getDateTimeSnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle setDateTime(java.lang.String observedKey, java.lang.String action, java.lang.String value, java.lang.String occurrence) throws android.os.RemoteException;
  /** Read-only owner history: stable, expiring pages; no notification actions. */
  public android.os.Bundle getNotificationHistory(long requestId, java.lang.String snapshotKey, int offset) throws android.os.RemoteException;
  /** Sounds v1: bounded catalogs, observed targets, finite preview/save and unconditional stop. */
  public android.os.Bundle getSoundsSnapshot(long requestId, java.lang.String type, java.lang.String catalogKey, int offset) throws android.os.RemoteException;
  public android.os.Bundle previewSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException;
  public android.os.Bundle saveSound(java.lang.String type, java.lang.String catalogKey, java.lang.String targetKey) throws android.os.RemoteException;
  public android.os.Bundle stopSound() throws android.os.RemoteException;
  /** Display v1: observed default-display choices and finite Night Light fields. */
  public android.os.Bundle getDisplaySnapshot(long requestId) throws android.os.RemoteException;
  public android.os.Bundle setDisplaySetting(java.lang.String observedKey, java.lang.String setting, java.lang.String value) throws android.os.RemoteException;
  /** App notifications v1: bounded owner pages and finite observed user changes. */
  public android.os.Bundle getAppNotifications(long requestId, java.lang.String packageName, int offset, java.lang.String generation) throws android.os.RemoteException;
  public android.os.Bundle setAppNotification(java.lang.String packageName, java.lang.String observedKey, java.lang.String targetKey, java.lang.String action, java.lang.String value) throws android.os.RemoteException;
  /** Default roles v1: native qualification and immutable native confirmation, no raw grant. */
  public android.os.Bundle getRolesSnapshot(long requestId, java.lang.String role, int offset, java.lang.String generation) throws android.os.RemoteException;
  public android.os.Bundle confirmRole(java.lang.String role, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException;
  /** Common runtime permissions: native observed choices and one-shot platform operation. */
  public android.os.Bundle getPermissionsSnapshot(long requestId, java.lang.String packageName, java.lang.String group, int offset, java.lang.String generation) throws android.os.RemoteException;
  public android.os.Bundle requestPermissionChoice(java.lang.String packageName, java.lang.String group, java.lang.String observedKey, java.lang.String targetKey) throws android.os.RemoteException;
  /** DND policy/time schedules: observed opaque targets, native scheduler, no condition URI. */
  public android.os.Bundle getDndSnapshot(long requestId, int offset, java.lang.String generation) throws android.os.RemoteException;
  public android.os.Bundle setDndPolicy(java.lang.String key, java.lang.String field, java.lang.String value) throws android.os.RemoteException;
  public android.os.Bundle saveDndSchedule(java.lang.String key, java.lang.String target, java.lang.String name, int[] days, int startMinute, int endMinute, boolean exitAtAlarm, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle setDndRuleEnabled(java.lang.String key, java.lang.String target, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle deleteDndRule(java.lang.String key, java.lang.String target) throws android.os.RemoteException;
  public android.os.Bundle getAppNetworkSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException;
  public android.os.Bundle setAppNetwork(java.lang.String packageName, java.lang.String key, java.lang.String field, boolean enabled) throws android.os.RemoteException;
  public android.os.Bundle getAppBatterySnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException;
  public android.os.Bundle setAppBattery(java.lang.String packageName, java.lang.String key, java.lang.String mode) throws android.os.RemoteException;
  public android.os.Bundle getAppStorageSnapshot(long requestId, java.lang.String packageName) throws android.os.RemoteException;
  public android.os.Bundle applyAppStorage(java.lang.String packageName, java.lang.String key, java.lang.String action) throws android.os.RemoteException;
  /** Per-app language: native-derived opaque catalog/choices, no caller-provided locale tags. */
  public android.os.Bundle getAppLanguageSnapshot(long requestId, java.lang.String packageName, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException;
  public android.os.Bundle setAppLanguage(java.lang.String packageName, java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException;
  /** Custom captions: process Binder lifetime plus scoped finite editor operations. */
  public android.os.Bundle getCaptionCustomSnapshot(long requestId, android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException;
  public android.os.Bundle setCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit, java.lang.String field, java.lang.String value) throws android.os.RemoteException;
  public android.os.Bundle closeCaptionCustom(android.os.IBinder owner, java.lang.String session, long visit) throws android.os.RemoteException;
  /** Native asset caption languages, selected only through reviewed opaque choices. */
  public android.os.Bundle getCaptionLanguageSnapshot(long requestId, java.lang.String catalogKey, java.lang.String query, int offset) throws android.os.RemoteException;
  public android.os.Bundle setCaptionLanguage(java.lang.String catalogKey, java.lang.String choiceKey) throws android.os.RemoteException;
  /** System language order: native catalog tokens only, one reviewed configuration write. */
  public android.os.Bundle getSystemLanguagesSnapshot(long requestId, java.lang.String catalogKey, java.lang.String parentKey, java.lang.String query, int offset) throws android.os.RemoteException;
  public android.os.Bundle applySystemLanguages(java.lang.String catalogKey, java.lang.String[] orderedTargets) throws android.os.RemoteException;
  // Appended keyboard inventory and finite native consent/provider flows (84/85).
  public android.os.Bundle getKeyboardsSnapshot(long requestId, java.lang.String query, int offset) throws android.os.RemoteException;
  public android.os.Bundle prepareKeyboardFlow(long requestId, java.lang.String key, java.lang.String target, java.lang.String operation) throws android.os.RemoteException;
  /** @hide */
  static class _Parcel {
    static private <T> T readTypedObject(
        android.os.Parcel parcel,
        android.os.Parcelable.Creator<T> c) {
      if (parcel.readInt() != 0) {
          return c.createFromParcel(parcel);
      } else {
          return null;
      }
    }
    static private <T extends android.os.Parcelable> void writeTypedObject(
        android.os.Parcel parcel, T value, int parcelableFlags) {
      if (value != null) {
        parcel.writeInt(1);
        value.writeToParcel(parcel, parcelableFlags);
      } else {
        parcel.writeInt(0);
      }
    }
  }
}
