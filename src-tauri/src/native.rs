//! Bridge to the Kotlin `VoicePlugin` (Android Keystore, foreground service, runtime permissions).
//! On desktop builds the same API is backed by no-op / software implementations so the
//! app can be developed with `tauri dev`.

use serde::{Deserialize, Serialize};
use tauri::{
    Manager, Wry,
    plugin::{Builder, TauriPlugin},
};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFacts {
    pub avail_mb: u64,
    pub low_memory: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub microphone: String,
    pub notifications: String,
    #[serde(default)]
    pub calendar: String,
    #[serde(default)]
    pub contacts: String,
}

#[derive(Debug, Clone)]
pub struct WrappedKey {
    pub blob: Vec<u8>,
    pub security_level: String,
}

pub struct NativeBridge {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<Wry>,
}

#[cfg(target_os = "android")]
mod imp {
    use super::*;
    use crate::error::msg;
    use base64::{Engine, engine::general_purpose::STANDARD as B64};

    #[derive(Serialize)]
    struct DataArgs {
        data: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WrapResponse {
        data: String,
        security_level: String,
    }

    #[derive(Deserialize)]
    struct DataResponse {
        data: String,
    }

    #[derive(Serialize)]
    struct PermArgs {
        permissions: Vec<&'static str>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ServiceArgs {
        title: String,
        text: String,
    }

    impl NativeBridge {
        pub fn wrap_key(&self, raw: &[u8]) -> Result<WrappedKey> {
            let r: WrapResponse = self
                .handle
                .run_mobile_plugin("wrapKey", DataArgs { data: B64.encode(raw) })
                .map_err(|e| msg(format!("keystore wrap failed: {e}")))?;
            Ok(WrappedKey {
                blob: B64.decode(r.data).map_err(|e| msg(e.to_string()))?,
                security_level: r.security_level,
            })
        }

        pub fn unwrap_key(&self, blob: &[u8]) -> Result<Vec<u8>> {
            let r: DataResponse = self
                .handle
                .run_mobile_plugin("unwrapKey", DataArgs { data: B64.encode(blob) })
                .map_err(|e| msg(format!("keystore unwrap failed: {e}")))?;
            B64.decode(r.data).map_err(|e| msg(e.to_string()))
        }

        pub fn key_security_level(&self) -> Result<String> {
            let r: serde_json::Value = self
                .handle
                .run_mobile_plugin("keySecurityLevel", ())
                .map_err(|e| msg(e.to_string()))?;
            Ok(r["securityLevel"].as_str().unwrap_or("unknown").to_string())
        }

        pub fn start_service(&self, title: &str, text: &str) -> Result<()> {
            self.handle
                .run_mobile_plugin::<serde_json::Value>(
                    "startListeningService",
                    ServiceArgs { title: title.into(), text: text.into() },
                )
                .map_err(|e| msg(format!("foreground service: {e}")))?;
            Ok(())
        }

        pub fn stop_service(&self) -> Result<()> {
            self.handle
                .run_mobile_plugin::<serde_json::Value>("stopListeningService", ())
                .map_err(|e| msg(e.to_string()))?;
            Ok(())
        }

        pub fn refresh_widgets(&self) {
            let _ = self.handle.run_mobile_plugin::<serde_json::Value>("refreshWidgets", ());
        }

        pub fn check_permissions(&self) -> Result<PermissionStatus> {
            self.handle
                .run_mobile_plugin("checkPermissions", ())
                .map_err(|e| msg(e.to_string()))
        }

        pub fn request_permissions(&self) -> Result<PermissionStatus> {
            self.handle
                .run_mobile_plugin(
                    "requestPermissions",
                    PermArgs { permissions: vec!["microphone", "notifications"] },
                )
                .map_err(|e| msg(e.to_string()))
        }

        fn plugin_json(&self, cmd: &str, payload: impl Serialize) -> Result<String> {
            #[derive(Deserialize)]
            struct Out {
                json: String,
            }
            let out: Out = self.handle.run_mobile_plugin(cmd, payload).map_err(|e| msg(e.to_string()))?;
            Ok(out.json)
        }

        pub fn request_calendar(&self) -> Result<PermissionStatus> {
            self.handle
                .run_mobile_plugin("requestPermissions", PermArgs { permissions: vec!["calendar"] })
                .map_err(|e| msg(e.to_string()))
        }

        pub fn request_contacts(&self) -> Result<PermissionStatus> {
            self.handle
                .run_mobile_plugin("requestPermissions", PermArgs { permissions: vec!["contacts"] })
                .map_err(|e| msg(e.to_string()))
        }

        pub fn lookup_contacts(&self, name: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                name: &'a str,
            }
            self.plugin_json("lookupContacts", Args { name })
        }

        pub fn list_phone_contacts(&self, query: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                query: &'a str,
            }
            self.plugin_json("listPhoneContacts", Args { query })
        }

        pub fn contact_handles(&self, id: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                id: &'a str,
            }
            self.plugin_json("contactHandles", Args { id })
        }

        pub fn open_phone_contact(&self, id: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                id: &'a str,
            }
            self.plugin_json("openPhoneContact", Args { id })
        }

        pub fn set_alarm(&self, hour: u32, minute: u32, label: &str, id: i32, at_ms: i64) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args<'a> {
                hour: u32,
                minute: u32,
                label: &'a str,
                id: i32,
                at_ms: i64,
            }
            self.plugin_json("setAlarm", Args { hour, minute, label, id, at_ms })
        }

        pub fn set_timer(&self, seconds: u32, label: &str, id: i32, at_ms: i64) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args<'a> {
                seconds: u32,
                label: &'a str,
                id: i32,
                at_ms: i64,
            }
            self.plugin_json("setTimer", Args { seconds, label, id, at_ms })
        }

        pub fn share_file(&self, path: &str, mime: &str, title: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                path: &'a str,
                mime: &'a str,
                title: &'a str,
            }
            self.plugin_json("shareFile", Args { path, mime, title })
        }

        pub fn schedule_item(&self, id: i32, kind: &str, title: &str, note: &str, at_ms: i64) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args<'a> {
                id: i32,
                kind: &'a str,
                title: &'a str,
                note: &'a str,
                at_ms: i64,
            }
            self.plugin_json("scheduleItem", Args { id, kind, title, note, at_ms })
        }

        pub fn cancel_item(&self, id: i32) -> Result<String> {
            #[derive(Serialize)]
            struct Args {
                id: i32,
            }
            self.plugin_json("cancelItem", Args { id })
        }

        pub fn list_calendar(&self, start_ms: i64, end_ms: i64) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                start_ms: i64,
                end_ms: i64,
            }
            self.plugin_json("listCalendarEvents", Args { start_ms, end_ms })
        }

        pub fn create_event(&self, title: &str, start_ms: i64, end_ms: i64, location: &str) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args<'a> {
                title: &'a str,
                start_ms: i64,
                end_ms: i64,
                location: &'a str,
            }
            self.plugin_json("createCalendarEvent", Args { title, start_ms, end_ms, location })
        }

        pub fn update_event(&self, id: &str, title: &str, start_ms: i64, end_ms: i64, location: &str) -> Result<String> {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args<'a> {
                id: &'a str,
                title: &'a str,
                start_ms: i64,
                end_ms: i64,
                location: &'a str,
            }
            self.plugin_json("updateCalendarEvent", Args { id, title, start_ms, end_ms, location })
        }

        pub fn delete_event(&self, id: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                id: &'a str,
            }
            self.plugin_json("deleteCalendarEvent", Args { id })
        }

        pub fn media_control(&self, action: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                action: &'a str,
            }
            self.plugin_json("mediaControl", Args { action })
        }

        pub fn adjust_volume(&self, direction: &str, steps: i32) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                direction: &'a str,
                steps: i32,
            }
            self.plugin_json("adjustVolume", Args { direction, steps })
        }

        pub fn open_hand_off(
            &self,
            kind: &str,
            query: &str,
            url: &str,
            phone: &str,
            text: &str,
            to: &str,
            subject: &str,
            body: &str,
        ) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                kind: &'a str,
                query: &'a str,
                url: &'a str,
                phone: &'a str,
                text: &'a str,
                to: &'a str,
                subject: &'a str,
                body: &'a str,
            }
            self.plugin_json(
                "openHandOff",
                Args { kind, query, url, phone, text, to, subject, body },
            )
        }

        pub fn read_notifications(&self) -> Result<String> {
            self.plugin_json("readNotifications", ())
        }

        pub fn set_dnd(&self, mode: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                mode: &'a str,
            }
            self.plugin_json("setDnd", Args { mode })
        }

        pub fn go_home(&self) -> Result<String> {
            self.plugin_json("goHome", ())
        }

        pub fn phone_status(&self) -> Result<String> {
            self.plugin_json("phoneStatus", ())
        }

        pub fn access_status(&self) -> Result<String> {
            self.plugin_json("accessStatus", ())
        }

        pub fn open_access_settings(&self, kind: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                kind: &'a str,
            }
            self.plugin_json("openAccessSettings", Args { kind })
        }

        pub fn request_history_access(&self) -> Result<PermissionStatus> {
            self.handle
                .run_mobile_plugin("requestPermissions", PermArgs { permissions: vec!["callLog", "sms"] })
                .map_err(|e| msg(e.to_string()))
        }

        pub fn contact_birthday(&self, id: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                id: &'a str,
            }
            self.plugin_json("contactBirthday", Args { id })
        }

        pub fn last_contact(&self, id: &str, phone: &str, name: &str) -> Result<String> {
            #[derive(Serialize)]
            struct Args<'a> {
                id: &'a str,
                phone: &'a str,
                name: &'a str,
            }
            self.plugin_json("lastContact", Args { id, phone, name })
        }

        pub fn device_facts(&self) -> Result<DeviceFacts> {
            self.handle.run_mobile_plugin("deviceFacts", ()).map_err(|e| msg(e.to_string()))
        }

        pub fn speak_fallback(&self, text: &str) -> Result<()> {
            #[derive(Serialize)]
            struct Args<'a> {
                text: &'a str,
            }
            self.handle
                .run_mobile_plugin::<serde_json::Value>("speakFallback", Args { text })
                .map_err(|e| msg(e.to_string()))?;
            Ok(())
        }
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use super::*;

    impl NativeBridge {
        pub fn wrap_key(&self, raw: &[u8]) -> Result<WrappedKey> {
            Ok(WrappedKey { blob: raw.to_vec(), security_level: "software".into() })
        }

        pub fn unwrap_key(&self, blob: &[u8]) -> Result<Vec<u8>> {
            Ok(blob.to_vec())
        }

        pub fn key_security_level(&self) -> Result<String> {
            Ok("software".into())
        }

        pub fn start_service(&self, _title: &str, _text: &str) -> Result<()> {
            Ok(())
        }

        pub fn stop_service(&self) -> Result<()> {
            Ok(())
        }

        pub fn refresh_widgets(&self) {}

        pub fn check_permissions(&self) -> Result<PermissionStatus> {
            Ok(PermissionStatus {
                microphone: "granted".into(),
                notifications: "granted".into(),
                calendar: "granted".into(),
                contacts: "granted".into(),
            })
        }

        pub fn request_permissions(&self) -> Result<PermissionStatus> {
            self.check_permissions()
        }

        pub fn request_contacts(&self) -> Result<PermissionStatus> {
            self.check_permissions()
        }

        pub fn lookup_contacts(&self, name: &str) -> Result<String> {
            Ok(serde_json::json!({
                "ok": false,
                "error": format!("Contacts for {name} are only on the Android phone.")
            })
            .to_string())
        }

        pub fn list_phone_contacts(&self, _query: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Phone contacts are only on the Android phone."}"#.into())
        }

        pub fn contact_handles(&self, _id: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Phone contacts are only on the Android phone."}"#.into())
        }

        pub fn open_phone_contact(&self, _id: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Opening a phone contact only works on Android."}"#.into())
        }

        pub fn request_calendar(&self) -> Result<PermissionStatus> {
            self.check_permissions()
        }

        pub fn set_alarm(&self, _hour: u32, minute: u32, label: &str, _id: i32, _at_ms: i64) -> Result<String> {
            Ok(serde_json::json!({
                "ok": true,
                "via": "local",
                "detail": format!("Alarm \"{label}\" saved for {minute:02}. It rings on the phone.")
            })
            .to_string())
        }

        pub fn set_timer(&self, seconds: u32, label: &str, _id: i32, _at_ms: i64) -> Result<String> {
            Ok(serde_json::json!({
                "ok": true,
                "via": "local",
                "detail": format!("Timer \"{label}\" for {seconds}s saved. It rings on the phone.")
            })
            .to_string())
        }

        pub fn share_file(&self, _path: &str, _mime: &str, _title: &str) -> Result<String> {
            Err(crate::error::msg("Sharing a file works on the Android phone."))
        }

        pub fn schedule_item(&self, _id: i32, _kind: &str, title: &str, _note: &str, _at_ms: i64) -> Result<String> {
            Ok(serde_json::json!({
                "ok": true,
                "via": "local",
                "detail": format!("Saved \"{title}\". It notifies on the phone.")
            })
            .to_string())
        }

        pub fn cancel_item(&self, _id: i32) -> Result<String> {
            Ok(r#"{"ok":true}"#.into())
        }

        pub fn media_control(&self, action: &str) -> Result<String> {
            Ok(serde_json::json!({ "ok": true, "detail": format!("Would send media {action} on the phone.") }).to_string())
        }

        pub fn adjust_volume(&self, direction: &str, _steps: i32) -> Result<String> {
            Ok(serde_json::json!({ "ok": true, "detail": format!("Would turn volume {direction} on the phone.") }).to_string())
        }

        pub fn open_hand_off(
            &self,
            kind: &str,
            _query: &str,
            _url: &str,
            _phone: &str,
            _text: &str,
            _to: &str,
            _subject: &str,
            _body: &str,
        ) -> Result<String> {
            Ok(serde_json::json!({ "ok": true, "detail": format!("Would open {kind} on the phone.") }).to_string())
        }

        pub fn list_calendar(&self, _start_ms: i64, _end_ms: i64) -> Result<String> {
            Ok(r#"{"ok":true,"events":[]}"#.into())
        }

        pub fn create_event(&self, _title: &str, _start_ms: i64, _end_ms: i64, _location: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"The calendar is only on the Android phone."}"#.into())
        }

        pub fn update_event(&self, _id: &str, _title: &str, _start_ms: i64, _end_ms: i64, _location: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"The calendar is only on the Android phone."}"#.into())
        }

        pub fn delete_event(&self, _id: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"The calendar is only on the Android phone."}"#.into())
        }

        pub fn read_notifications(&self) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Notifications are read on the Android phone."}"#.into())
        }

        pub fn set_dnd(&self, _mode: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Do Not Disturb is changed on the Android phone."}"#.into())
        }

        pub fn go_home(&self) -> Result<String> {
            Ok(r#"{"ok":false,"error":"The home screen is only on the Android phone."}"#.into())
        }

        pub fn phone_status(&self) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Battery and the next alarm are read on the Android phone."}"#.into())
        }

        pub fn access_status(&self) -> Result<String> {
            Ok(r#"{"notificationListener":"unknown","dnd":"unknown","callLog":"unknown","sms":"unknown"}"#.into())
        }

        pub fn open_access_settings(&self, _kind: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"System settings open on the Android phone."}"#.into())
        }

        pub fn request_history_access(&self) -> Result<PermissionStatus> {
            self.check_permissions()
        }

        pub fn contact_birthday(&self, _id: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Contact birthdays are read on the Android phone."}"#.into())
        }

        pub fn last_contact(&self, _id: &str, _phone: &str, _name: &str) -> Result<String> {
            Ok(r#"{"ok":false,"error":"Call and text history are read on the Android phone."}"#.into())
        }

        pub fn device_facts(&self) -> Result<DeviceFacts> {
            Ok(DeviceFacts { avail_mb: 0, low_memory: false, online: true })
        }

        pub fn speak_fallback(&self, _text: &str) -> Result<()> {
            Ok(())
        }
    }
}

pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("voicebridge")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let bridge = NativeBridge {
                handle: _api.register_android_plugin("ai.grokvoice.app", "VoicePlugin")?,
            };
            #[cfg(not(target_os = "android"))]
            let bridge = NativeBridge {};
            app.manage(bridge);
            Ok(())
        })
        .build()
}
