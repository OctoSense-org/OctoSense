# jit3 port worklist (from check4: bare relax, 182 errors)

## other (55)
- crates/ai-host/src/module_peer.rs:46: no method named `try_iter` found for `Receiver<{error}>` in the current scope
- crates/ai-host/src/module_peer.rs:53: no method named `clone` found for `Arc<Mutex<Sender<{error}>>>` in the current scope
- crates/ai-host/src/qr.rs:157: no method named `show_qr_scanner` found for `&mut Cx` in the current scope
- crates/shell/src/ai_bus.rs:48: no field `self_confirm` on type `&mut ToolDef`
- crates/shell/src/ai_bus.rs:58: no method named `confirms_itself` found for `&ToolDef` in the current scope
- crates/shell/src/ai_bus.rs:139: no field `self_confirm` on type `&mut ToolDef`
- crates/shell/src/ai_bus.rs:473: no method named `clear_self_confirm` found for `&mut ServiceManifest` in the current scope
- crates/shell/src/ai_bus.rs:512: missing field `caller` in initializer of `ServiceCall`
- crates/shell/src/android_integration.rs:472: no function or associated item named `AndroidIntegration` found for enum `Event` in the current scope
- crates/shell/src/android_integration.rs:479: cannot find value `payload` in this scope
- crates/shell/src/android_integration.rs:480: cannot find value `channel` in this scope
- crates/shell/src/android_integration.rs:484: cannot find value `channel` in this scope
- crates/shell/src/android_integration.rs:486: cannot find value `channel` in this scope
- crates/shell/src/clients.rs:827: no method named `reserve` found for `&TaskPool` in the current scope
- crates/shell/src/glance_card.rs:1147: no method named `scroll_pos` found for `ViewRef` in the current scope
- crates/shell/src/glance_card.rs:1264: no method named `is_some_and` found for `Option<Ref<{error}>>` in the current scope
- crates/shell/src/glance_card.rs:1264: cannot find type `makepad_widgets::web_reader::WebReader` in this scope
- crates/shell/src/lib.rs:1273: mismatched types: expected `SharedSwapchain`, found `Option<SharedSwapchain>`
- crates/shell/src/lib.rs:2987: cannot find value `key` in this scope
- crates/shell/src/lib.rs:2994: cannot find value `title` in this scope
- crates/shell/src/lib.rs:3006: cannot find value `size` in this scope
- crates/shell/src/lib.rs:3006: no field `x` on type `?173`
- crates/shell/src/lib.rs:3011: cannot find value `title` in this scope
- crates/shell/src/lib.rs:3015: cannot find value `root` in this scope
- crates/shell/src/lib.rs:3017: cannot find value `key` in this scope
- crates/shell/src/lib.rs:3018: cannot find value `title` in this scope
- crates/shell/src/lib.rs:3026: cannot find value `key` in this scope
- crates/shell/src/lib.rs:5613: missing field `caller` in initializer of `ServiceCall`
- crates/shell/src/lib.rs:6442: missing field `dock` in initializer of `MacosMenu`
- crates/shell/src/lib.rs:6676: mismatched types: expected `&SheetEntry`, found `DesktopStyle`
- crates/shell/src/lib.rs:6903: no function or associated item named `HomeIntent` found for enum `Event` in the current scope
- crates/shell/src/mail_card.rs:69: cannot find value `makepad_widgets::makepad_platform::trusted_user_input` in this scope
- crates/shell/src/mobile_app.rs:868: non-exhaustive patterns: `TouchState::Cancel` not covered
- crates/shell/src/module_host.rs:178: no method named `extend` found for `Vec<(SplashVmId, {error})>` in the current scope
- crates/shell/src/module_host.rs:182: no method named `extend` found for `Vec<(SplashVmId, {error})>` in the current scope
- crates/shell/src/module_host.rs:636: struct `InstanceHandles` has no field named `windows`
- crates/shell/src/module_host.rs:1125: no method named `is_none` found for `Option<{error}>` in the current scope
- crates/shell/src/module_host.rs:1494: no method named `unwrap_or_else` found for `Option<{error}>` in the current scope
- crates/shell/src/module_host.rs:1508: no method named `map` found for `Option<RefMut<{error}>>` in the current scope
- crates/shell/src/module_host.rs:1508: cannot find type `makepad_terminal::tabs::TermTabs` in this scope
- crates/shell/src/native_apps.rs:562: cannot find value `makepad_terminal::TERMINAL_MODULE` in this scope
- crates/shell/src/octosense/icon_frame.rs:16: non-exhaustive patterns: `DesktopStyle::BlackOrange` not covered
- crates/shell/src/octosense/paths.rs:25: cannot find value `makepad_widgets::makepad_platform::home::platform_data_dir` in this scope
- crates/shell/src/octosense/style.rs:57: no function or associated item named `load_with_appearance` found for struct `StyleSheet` in the current scope
- crates/shell/src/octosense/style.rs:82: missing fields `family` and `resources` in initializer of `StyleSheet`
- crates/shell/src/octosense/style.rs:147: non-exhaustive patterns: `DesktopStyle::BlackOrange` not covered
- crates/shell/src/run_view.rs:673: mismatched types: expected `SharedSwapchain`, found `Option<SharedSwapchain>`
- crates/shell/src/run_view.rs:887: no method named `dpi_snap` found for `Rect` in the current scope
- crates/shell/src/run_view.rs:1133: missing field `phase` in initializer of `RemoteScroll`
- crates/shell/src/run_view.rs:1134: no field `is_mouse` on type `FingerScrollEvent`
- crates/shell/src/studio/mod.rs:537: no method named `set_device_consent` found for `SplashRef` in the current scope
- crates/shell/src/studio/mod.rs:544: no method named `set_memory_bytes` found for `SplashRef` in the current scope
- crates/shell/src/system_chat/view.rs:996: this method takes 3 arguments but 2 were supplied
- crates/shell/src/system_chat/view.rs:1093: this method takes 3 arguments but 2 were supplied
- crates/shell/src/system_chat/view.rs:1320: type `{error}` cannot be dereferenced

## ai-services-links (21)
- crates/ai-host/src/module_peer.rs:22: unresolved import `makepad_ai_services::peer::PeerDown`
- crates/ai-host/src/module_peer.rs:22: unresolved import `makepad_ai_services::peer::PeerLink`
- crates/ai-host/src/module_peer.rs:22: unresolved import `makepad_ai_services::peer::PeerUp`
- crates/shell/src/kernel_port.rs:17: unresolved import `makepad_ai_services::ui_port::UiPortEvent`
- crates/shell/src/kernel_port.rs:17: unresolved import `makepad_ai_services::ui_port::UiPortLink`
- crates/shell/src/pane_links.rs:19: unresolved import `makepad_ai_services::port::PendingServiceLinks`
- crates/ai-host/src/module_peer.rs:34: cannot find type `PeerUp` in this scope
- crates/ai-host/src/module_peer.rs:35: cannot find type `PeerDown` in this scope
- crates/ai-host/src/module_peer.rs:39: cannot find type `PeerLink` in this scope
- crates/ai-host/src/module_peer.rs:55: failed to resolve: use of undeclared type `PeerDown`
- crates/shell/src/kernel_port.rs:20: cannot find type `UiPortLink` in this scope
- crates/shell/src/kernel_port.rs:33: cannot find type `UiPortLink` in this scope
- crates/shell/src/module_host.rs:126: cannot find type `makepad_ai_services::peer::PeerLink` in this scope
- crates/shell/src/module_host.rs:135: cannot find type `makepad_ai_services::ui_port::UiPortLink` in this scope
- crates/shell/src/module_host.rs:151: cannot find type `makepad_ai_services::peer::PendingPeerLinks` in this scope
- crates/shell/src/module_host.rs:155: cannot find type `makepad_ai_services::ui_port::PendingUiPorts` in this scope
- crates/shell/src/module_host.rs:176: cannot find type `makepad_ai_services::peer::PendingPeerLinks` in this scope
- crates/shell/src/module_host.rs:180: cannot find type `makepad_ai_services::ui_port::PendingUiPorts` in this scope
- crates/shell/src/pane_links.rs:58: cannot find type `PendingServiceLinks` in this scope
- crates/shell/src/pane_links.rs:68: cannot find type `PendingServiceLinks` in this scope
- crates/shell/src/pane_links.rs:80: cannot find type `PendingServiceLinks` in this scope

## module-host (15)
- crates/shell/src/lib.rs:186: unresolved import `makepad_app_module::CloseDecision`
- crates/shell/src/lib.rs:186: unresolved import `makepad_app_module::ModuleCloseAction`
- crates/shell/src/lib.rs:186: unresolved import `makepad_app_module::WindowRequest`
- crates/shell/src/lib.rs:1876: failed to resolve: use of undeclared type `CloseDecision`
- crates/shell/src/lib.rs:2986: failed to resolve: use of undeclared type `WindowRequest`
- crates/shell/src/lib.rs:3023: failed to resolve: use of undeclared type `WindowRequest`
- crates/shell/src/lib.rs:6473: failed to resolve: use of undeclared type `ModuleCloseAction`
- crates/shell/src/lib.rs:6473: cannot find type `ModuleCloseAction` in this scope
- crates/shell/src/module_host.rs:312: cannot find type `ModuleWindows` in this scope
- crates/shell/src/module_host.rs:473: cannot find type `CloseDecision` in this scope
- crates/shell/src/module_host.rs:633: failed to resolve: use of undeclared type `ModuleWindows`
- crates/shell/src/module_host.rs:835: cannot find type `CloseDecision` in this scope
- crates/shell/src/module_host.rs:840: cannot find type `CloseDecision` in this scope
- crates/shell/src/module_host.rs:859: failed to resolve: use of undeclared type `CloseDecision`
- crates/shell/src/module_host.rs:1048: cannot find type `WindowRequest` in this scope

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

## readback (10)
- crates/shell/src/studio/mod.rs:624: no field `stride` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:628: no field `stride` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:634: no field `origin` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:635: failed to resolve: use of undeclared type `ReadbackOrigin`
- crates/shell/src/studio/mod.rs:636: failed to resolve: use of undeclared type `ReadbackOrigin`
- crates/shell/src/studio/mod.rs:638: no field `stride` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:638: no field `stride` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:639: no field `channel_order` on type `&TextureReadback`
- crates/shell/src/studio/mod.rs:640: failed to resolve: use of undeclared type `ReadbackChannelOrder`
- crates/shell/src/studio/mod.rs:641: failed to resolve: use of undeclared type `ReadbackChannelOrder`

## thread-lane (8)
- crates/shell/src/android_integration.rs:8: unresolved import `makepad_widgets::makepad_platform::thread::Lane`
- crates/shell/src/clients.rs:36: unresolved import `makepad_widgets::makepad_platform::thread::Lane`
- crates/shell/src/lib.rs:151: unresolved import `makepad_widgets::makepad_platform::thread::Lane`
- crates/shell/src/android_integration.rs:1027: failed to resolve: use of undeclared type `Lane`
- crates/shell/src/clients.rs:798: failed to resolve: use of undeclared type `Lane`
- crates/shell/src/clients.rs:827: failed to resolve: use of undeclared type `Lane`
- crates/shell/src/lib.rs:3494: failed to resolve: use of undeclared type `Lane`
- crates/shell/src/studio/mod.rs:430: failed to resolve: use of undeclared type `Lane`

## android (7)
- crates/ai-host/src/qr.rs:163: no method named `android_integration` found for `&mut Cx` in the current scope
- crates/shell/src/android_integration.rs:370: no method named `android_integration` found for `&mut Cx` in the current scope
- crates/shell/src/android_integration.rs:626: no method named `android_integration` found for `&mut Cx` in the current scope
- crates/shell/src/android_integration.rs:689: no method named `android_integration` found for `&mut Cx` in the current scope
- crates/shell/src/mobile_surface.rs:538: no method named `android_integration` found for `&mut Cx2d` in the current scope
- crates/shell/src/mobile_surface.rs:586: no method named `android_integration` found for `&mut Cx2d` in the current scope
- crates/shell/src/mobile_surface.rs:610: no method named `android_integration` found for `&mut Cx2d` in the current scope

## script-vm-trust (7)
- crates/shell/src/glance_style.rs:36: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/glance_style.rs:49: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/module_host.rs:667: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/module_host.rs:738: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/module_host.rs:1125: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/module_host.rs:1204: no method named `with_script_vm_id_trusted` found for `&mut Cx` in the current scope
- crates/shell/src/module_view.rs:563: no method named `with_script_vm_id_trusted` found for `&mut Cx2d` in the current scope

## native-qr (4)
- crates/ai-host/src/qr.rs:181: unresolved import `makepad_widgets::makepad_platform::event::NativeQrCancelled`
- crates/ai-host/src/qr.rs:181: unresolved import `makepad_widgets::makepad_platform::event::NativeQrScanned`
- crates/ai-host/src/qr.rs:184: cannot find type `NativeQrScanned` in this scope
- crates/ai-host/src/qr.rs:188: cannot find type `NativeQrCancelled` in this scope

## splash-host (4)
- crates/shell/src/glance_card.rs:1282: cannot find value `splash_host::take_splash_host_requests_for` in this scope
- crates/shell/src/glance_card.rs:1288: cannot find value `splash_host::take_splash_host_requests_for` in this scope
- crates/shell/src/glance_card.rs:1300: cannot find value `splash_host::take_splash_host_requests_for` in this scope
- crates/shell/src/studio/mod.rs:157: cannot find value `makepad_widgets::splash_host::take_splash_host_requests_for` in this scope

## screen-origin (3)
- crates/shell/src/lib.rs:1265: missing field `screen_origin` in initializer of `StudioToApp`
- crates/shell/src/mobile_app.rs:160: missing field `screen_origin` in initializer of `StudioToApp`
- crates/shell/src/run_view.rs:643: missing field `screen_origin` in initializer of `StudioToApp`

## splash-policy (2)
- crates/shell/src/glance_card.rs:1348: no method named `set_policy` found for `&SplashRef` in the current scope
- crates/shell/src/studio/mod.rs:543: no method named `set_policy` found for `SplashRef` in the current scope

## ime-state-query (2)
- crates/shell/src/lib.rs:7279: no function or associated item named `TextInputStateQuery` found for enum `Event` in the current scope
- crates/shell/src/system_chat/view.rs:1319: no function or associated item named `TextInputStateQuery` found for enum `Event` in the current scope

## terminal-ai (2)
- crates/shell/src/module_host.rs:1490: failed to resolve: use of undeclared type `makepad_terminal::ai`
- crates/shell/src/module_host.rs:1495: failed to resolve: use of undeclared type `makepad_terminal::ai`

## modal-bounds (2)
- crates/shell/src/module_view.rs:577: cannot find type `ModalBounds` in this scope
- crates/shell/src/module_view.rs:644: cannot find type `ModalBounds` in this scope

