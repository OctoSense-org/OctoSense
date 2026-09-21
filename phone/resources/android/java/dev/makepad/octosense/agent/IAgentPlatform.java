/*
 * This file is auto-generated.  DO NOT MODIFY.
 * Using: ~/.local/share/octosense/android-tools/sdk/build-tools/35.0.0/aidl -Iresources/android/java -p~/.local/share/octosense/android-tools/sdk/platforms/android-35/framework.aidl resources/android/java/dev/makepad/octosense/agent/IAgentPlatform.aidl resources/android/java/dev/makepad/octosense/agent/IAgentPlatform.java
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
