use super::*;
#[test]
    fn control_values_and_android_permissions_fail_closed() {
        for value in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
            assert!(!SettingsRequest::Device(DeviceSetting::MediaVolume(value)).valid());
            assert!(!SettingsRequest::Brightness { value, automatic: false }.valid());
        }
        let mut state = SettingsSnapshot { capabilities: ["volume".into()].into_iter().collect(), ..Default::default() };
        assert!(!state.permits("volume"));
        state.android = true;
        assert!(!state.permits("volume"));
        state.connected = true;
        assert!(state.permits("volume"));
        assert!(!state.permits("dnd"));
    }

#[test]
    fn device_settings_validate_types_ranges_and_admin_timeout_cap() {
        for value in [f64::NAN, f64::INFINITY, -0.1, 1.01] {
            for setting in [DeviceSetting::MediaVolume(value), DeviceSetting::AlarmVolume(value), DeviceSetting::RingVolume(value), DeviceSetting::NotificationVolume(value)] {
                assert!(!SettingsRequest::Device(setting).valid());
            }
        }
        for value in [0, 999, 90_000, 2_147_483_647] { assert!(!DeviceSetting::ScreenTimeout(value).valid()); }
        for value in TIMEOUTS { assert!(DeviceSetting::ScreenTimeout(value).valid()); }
        for value in [f64::NAN, 0.0, 1.01, 2.0] { assert!(!DeviceSetting::FontScale(value).valid()); }
        for value in FONT_SCALES { assert!(DeviceSetting::FontScale(value).valid()); }
        let mut d = DeviceSnapshot { capabilities: ["screen_timeout_ms".into(), "auto_time".into()].into_iter().collect(),
            screen_timeout_ms: Some(30_000), max_screen_timeout_ms: Some(60_000), ..Default::default() };
        assert!(d.permits(&DeviceSetting::ScreenTimeout(60_000)));
        assert!(!d.permits(&DeviceSetting::ScreenTimeout(120_000)));
        assert!(!d.permits(&DeviceSetting::AutoTime(true)), "capability alone cannot invent missing observed state");
        d.max_screen_timeout_ms = None;
        assert!(!d.permits(&DeviceSetting::ScreenTimeout(15_000)));
        d.max_screen_timeout_ms = Some(0);
        assert!(d.permits(&DeviceSetting::ScreenTimeout(600_000)));
        assert!(matches!(DeviceSetting::ScreenTimeout(15_000).value(), Value::Int(15_000)));
        assert!(matches!(DeviceSetting::HourFormat(true).value(), Value::Bool(true)));
    }

#[test]
    fn device_snapshot_omissions_and_malformed_fields_stay_unavailable() {
        let json = |text: &str| makepad_strict_json::parse(text.as_bytes()).unwrap();
        let d = DeviceSnapshot::decode(&json(r#"{"schema":1,"request_id":7,"capabilities":["font_scale","arbitrary_system_key"],"values":{"font_scale":1.15,"auto_time":"true","volume_alarm":2},"about":{"memory_total":-1,"model":"OnePlus"},"battery":{"level":101,"status":"unknown","plugged":"forged"},"storage":{},"date_time":{}}"#)).unwrap();
        assert_eq!(d.request_id, 7);
        assert_eq!(d.model.as_deref(), Some("OnePlus"));
        assert_eq!(d.font_scale, Some(1.15));
        assert_eq!(d.capabilities.len(), 1);
        assert!(d.auto_time.is_none()); assert!(d.volume_alarm.is_none());
        assert!(d.battery_level.is_none()); assert!(d.battery_plugged.is_none());
        assert_eq!(d.battery_status.as_deref(), Some("unknown"));
        assert!(d.storage_total.is_none()); assert!(d.memory_total.is_none()); assert!(d.local_time.is_none());
        for invalid in [r#"{"schema":2,"request_id":7,"capabilities":[],"values":{}}"#,
            r#"{"schema":1,"request_id":0,"capabilities":[],"values":{}}"#,
            r#"{"schema":1,"request_id":7,"capabilities":[42],"values":{}}"#,
            r#"{"schema":1,"request_id":7,"capabilities":[],"values":[]}"#] {
            assert!(DeviceSnapshot::decode(&json(invalid)).is_none());
        }
    }
