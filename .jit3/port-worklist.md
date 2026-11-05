# jit3 port worklist (from check5 after AI1+W4+close-veto: bare relax)

## other (21)
- crates/shell/src/glance_card.rs:1147: no method named `scroll_pos` found for `ViewRef` in the current scope
- crates/shell/src/glance_card.rs:1264: no method named `is_some_and` found for `Option<Ref<{error}>>` in the current scope
- crates/shell/src/glance_card.rs:1264: cannot find type `makepad_widgets::web_reader::WebReader` in this scope
- crates/shell/src/lib.rs:1273: mismatched types: expected `SharedSwapchain`, found `Option<SharedSwapchain>`
- crates/shell/src/lib.rs:6443: missing field `dock` in initializer of `MacosMenu`
- crates/shell/src/lib.rs:6677: mismatched types: expected `&SheetEntry`, found `DesktopStyle`
- crates/shell/src/lib.rs:6904: no function or associated item named `HomeIntent` found for enum `Event` in the current scope
- crates/shell/src/mail_card.rs:69: cannot find value `makepad_widgets::makepad_platform::trusted_user_input` in this scope
- crates/shell/src/mobile_app.rs:868: non-exhaustive patterns: `TouchState::Cancel` not covered
- crates/shell/src/octosense/icon_frame.rs:16: non-exhaustive patterns: `DesktopStyle::BlackOrange` not covered
- crates/shell/src/octosense/paths.rs:25: cannot find value `makepad_widgets::makepad_platform::home::platform_data_dir` in this scope
- crates/shell/src/octosense/style.rs:57: no function or associated item named `load_with_appearance` found for struct `StyleSheet` in the current scope
- crates/shell/src/octosense/style.rs:82: missing fields `family` and `resources` in initializer of `StyleSheet`
- crates/shell/src/octosense/style.rs:147: non-exhaustive patterns: `DesktopStyle::BlackOrange` not covered
- crates/shell/src/run_view.rs:673: mismatched types: expected `SharedSwapchain`, found `Option<SharedSwapchain>`
- crates/shell/src/run_view.rs:887: no method named `dpi_snap` found for `Rect` in the current scope
- crates/shell/src/run_view.rs:1133: missing field `phase` in initializer of `RemoteScroll`
- crates/shell/src/run_view.rs:1134: no field `is_mouse` on type `FingerScrollEvent`
- crates/shell/src/system_chat/view.rs:996: this method takes 3 arguments but 2 were supplied
- crates/shell/src/system_chat/view.rs:1093: this method takes 3 arguments but 2 were supplied
- crates/shell/src/system_chat/view.rs:1320: type `{error}` cannot be dereferenced

## relax-json-macro (15)
- crates/shell/src/approvals/mod.rs:271: internal: macro `json!` is not supported
- crates/shell/src/approvals/mod.rs:450: internal: macro `json!` is not supported
- crates/shell/src/approvals/mod.rs:461: internal: macro `json!` is not supported
- crates/shell/src/approvals/mod.rs:468: internal: macro `json!` is not supported
- crates/shell/src/approvals/mod.rs:479: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:428: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:432: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:448: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:458: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:508: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:532: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:584: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:584: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:609: internal: macro `json!` is not supported
- crates/shell/src/glance_card.rs:725: internal: macro `json!` is not supported

## relax-include-str (14)
- crates/shell/src/glance_card.rs:99: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:115: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:116: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:117: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:121: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:122: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:123: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:124: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:125: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:126: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:127: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:128: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:129: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi
- crates/shell/src/glance_card.rs:130: internal: `include_str!`: expected a literal, found `compile_error ! ("`concat!`: expected a literal, found `compile_error ! (\"`env!`: envi

## accessibility (11)
- crates/shell/src/android_accessibility.rs:3: unresolved import `makepad_widgets::makepad_platform::accessibility::AccessibilityPreferences`
- crates/shell/src/android_accessibility.rs:5: cannot find type `AccessibilityPreferences` in this scope
- crates/shell/src/android_accessibility.rs:5: cannot find type `AccessibilityPreferences` in this scope
- crates/shell/src/android_integration.rs:784: no method named `accessibility_preferences` found for `&mut Cx` in the current scope
- crates/shell/src/android_integration.rs:786: no method named `set_accessibility_preferences` found for `&mut Cx` in the current scope
- crates/shell/src/glance_sheet.rs:240: no method named `accessibility_preferences` found for `&Cx` in the current scope
- crates/shell/src/mobile_island.rs:579: no method named `accessibility_preferences` found for `&mut Cx2d` in the current scope
- crates/shell/src/mobile_island.rs:609: no method named `accessibility_preferences` found for `&mut Cx2d` in the current scope
- crates/shell/src/mobile_island.rs:638: no method named `accessibility_preferences` found for `&mut Cx2d` in the current scope
- crates/shell/src/scene.rs:57: no method named `accessibility_preferences` found for `&mut Cx` in the current scope
- crates/shell/src/scene.rs:98: no method named `accessibility_preferences` found for `&mut Cx` in the current scope

## screen-origin (3)
- crates/shell/src/lib.rs:1265: missing field `screen_origin` in initializer of `StudioToApp`
- crates/shell/src/mobile_app.rs:160: missing field `screen_origin` in initializer of `StudioToApp`
- crates/shell/src/run_view.rs:643: missing field `screen_origin` in initializer of `StudioToApp`

## ime-state-query (2)
- crates/shell/src/lib.rs:7280: no function or associated item named `TextInputStateQuery` found for enum `Event` in the current scope
- crates/shell/src/system_chat/view.rs:1319: no function or associated item named `TextInputStateQuery` found for enum `Event` in the current scope

