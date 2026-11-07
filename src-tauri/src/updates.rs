//! Daily check for a newer published GitHub release.
//!
//! Talks only to `api.github.com` for this repo. A newer stable release is shown
//! in the app and as a notification. Nothing is downloaded until the user taps.

use std::{fs, io::Read, path::PathBuf, sync::RwLock, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{diagnostics::Hub, native::NativeBridge, settings::SettingsStore};

const LATEST_URL: &str =
    "https://api.github.com/repos/Gabo-Tech/grok-voice-android/releases/latest";

/// After a successful response, wait a day before asking GitHub again.
pub const SUCCESS_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;
/// A failed or offline attempt waits this long. Shorter than a day, still not a poll.
pub const FAILURE_INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;
const MIN_SLEEP_MS: u64 = 60 * 1000;
const MAX_BODY: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Offer {
    version: String,
    tag: String,
    url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct UpdateState {
    last_attempt_ms: u64,
    last_ok: bool,
    version: String,
    tag: String,
    url: String,
    dismissed_version: String,
    notified_version: String,
    notified_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub installed: String,
    pub version: String,
    pub tag: String,
    pub url: String,
}

impl UpdateStatus {
    pub fn hidden(installed: impl Into<String>) -> Self {
        Self {
            installed: installed.into(),
            version: String::new(),
            tag: String::new(),
            url: String::new(),
        }
    }
}

pub struct UpdateStore {
    path: PathBuf,
    inner: RwLock<UpdateState>,
}

impl UpdateStore {
    pub fn load(path: PathBuf) -> Self {
        let inner = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: RwLock::new(inner),
        }
    }

    fn snapshot(&self) -> UpdateState {
        self.inner.read().unwrap().clone()
    }

    fn save(&self, state: &UpdateState) {
        if let Ok(bytes) = serde_json::to_vec_pretty(state) {
            let _ = crate::files::write_private(&self.path, &bytes);
        }
        *self.inner.write().unwrap() = state.clone();
    }

    fn record_success(&self, now: u64, offer: Option<Offer>, installed: &str) {
        let mut state = self.snapshot();
        state.last_attempt_ms = now;
        state.last_ok = true;
        if let Some(offer) =
            offer.filter(|offer| is_newer(&offer.tag, installed) && update_url_allowed(&offer.url))
        {
            state.version = offer.version;
            state.tag = offer.tag;
            state.url = offer.url;
        } else {
            state.version.clear();
            state.tag.clear();
            state.url.clear();
        }
        self.save(&state);
    }

    fn record_failure(&self, now: u64) {
        let mut state = self.snapshot();
        state.last_attempt_ms = now;
        state.last_ok = false;
        self.save(&state);
    }
}

pub fn run(app: AppHandle) {
    loop {
        let state = app.state::<UpdateStore>().snapshot();
        let now = crate::diagnostics::now_ms();
        if should_check_now(state.last_attempt_ms, state.last_ok, now) {
            check_once(&app);
        }
        let state = app.state::<UpdateStore>().snapshot();
        let pause = wait_ms(
            state.last_attempt_ms,
            state.last_ok,
            crate::diagnostics::now_ms(),
        );
        std::thread::sleep(Duration::from_millis(pause));
    }
}

fn check_once(app: &AppHandle) {
    let now = crate::diagnostics::now_ms();
    let installed = installed_version(app);
    match fetch_latest(app) {
        Ok(offer) => app
            .state::<UpdateStore>()
            .record_success(now, offer, &installed),
        Err(reason) => {
            if let Some(hub) = app.try_state::<std::sync::Arc<Hub>>() {
                hub.info(format!("update check skipped ({})", short_reason(&reason)));
            }
            app.state::<UpdateStore>().record_failure(now);
        }
    }
    let status = sync_notice(app);
    if let Some(hub) = app.try_state::<std::sync::Arc<Hub>>() {
        if !status.version.is_empty() {
            hub.info(format!("update {} is published", status.version));
        }
        hub.emit("voice://update", &status);
    }
}

pub fn sync_notice(app: &AppHandle) -> UpdateStatus {
    let installed = installed_version(app);
    let store = app.state::<UpdateStore>();
    let state = store.snapshot();
    let status = visible(&state, &installed);
    if status.version.is_empty() {
        // An empty cache means we have not posted yet. Cancel only when a saved
        // offer is no longer something we should show, so a check in flight is kept.
        if !state.version.is_empty()
            && (!should_notify(&installed, &state.tag, &state.dismissed_version)
                || !update_url_allowed(&state.url))
        {
            if let Some(bridge) = app.try_state::<NativeBridge>() {
                let _ = bridge.cancel_update_notice();
            }
        }
        return status;
    }
    let now = crate::diagnostics::now_ms();
    if should_post_notification(
        &state.notified_version,
        state.notified_at_ms,
        &state.version,
        now,
    ) && post_notice(app, &state.version, &state.url).is_ok()
    {
        let mut next = state;
        next.notified_version = next.version.clone();
        next.notified_at_ms = now;
        store.save(&next);
    }
    status
}

pub fn dismiss(app: &AppHandle) -> UpdateStatus {
    let installed = installed_version(app);
    let store = app.state::<UpdateStore>();
    let mut state = store.snapshot();
    if release_names(&state.version).is_some() {
        state.dismissed_version = state.version.clone();
        store.save(&state);
    }
    if let Some(bridge) = app.try_state::<NativeBridge>() {
        let _ = bridge.cancel_update_notice();
    }
    visible(&store.snapshot(), &installed)
}

fn installed_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn notice_locale(app: &AppHandle) -> String {
    let raw = app
        .try_state::<SettingsStore>()
        .map(|store| store.get().locale)
        .unwrap_or_default();
    if raw.trim().is_empty() {
        String::new()
    } else {
        crate::i18n::normalize(&raw).to_string()
    }
}

fn post_notice(app: &AppHandle, version: &str, url: &str) -> Result<(), ()> {
    if !update_url_allowed(url) {
        return Err(());
    }
    let Some((_, bare)) = release_names(version) else {
        return Err(());
    };
    let Some(bridge) = app.try_state::<NativeBridge>() else {
        return Err(());
    };
    bridge
        .notify_update(&notice_locale(app), &bare, url)
        .map_err(|_| ())
}

fn visible(state: &UpdateState, installed: &str) -> UpdateStatus {
    let version = state.version.trim();
    let show = should_notify(installed, &state.tag, &state.dismissed_version)
        && update_url_allowed(&state.url)
        && release_names(version).is_some_and(|(_, bare)| bare == version);
    if show {
        UpdateStatus {
            installed: installed.to_string(),
            version: version.to_string(),
            tag: state.tag.clone(),
            url: state.url.clone(),
        }
    } else {
        UpdateStatus::hidden(installed)
    }
}

fn fetch_latest(app: &AppHandle) -> Result<Option<Offer>, String> {
    if !device_online(app) {
        return Err("offline".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(crate::http::github_api_redirect())
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(LATEST_URL)
        .header(reqwest::header::USER_AGENT, user_agent())
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let final_url = response.url().clone();
    if !crate::http::https_url_allowed(final_url.as_str(), &["api.github.com"]) {
        return Err("unexpected host".into());
    }
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("http {}", status.as_u16()));
    }
    let mut buf = Vec::new();
    response
        .take(MAX_BODY + 1)
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    if buf.len() as u64 > MAX_BODY {
        return Err("response too large".into());
    }
    parse_latest_release(&buf)
}

fn device_online(app: &AppHandle) -> bool {
    let Some(bridge) = app.try_state::<NativeBridge>() else {
        return true;
    };
    match bridge.device_facts() {
        Ok(facts) => facts.online,
        Err(_) => true,
    }
}

fn user_agent() -> String {
    format!(
        "GrokVoice/{} (https://github.com/Gabo-Tech/grok-voice-android)",
        env!("CARGO_PKG_VERSION")
    )
}

fn short_reason(reason: &str) -> String {
    reason
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(120)
        .collect()
}

fn parse_latest_release(body: &[u8]) -> Result<Option<Offer>, String> {
    let body = std::str::from_utf8(body).map_err(|_| "not utf-8".to_string())?;
    let release: GhRelease = serde_json::from_str(body).map_err(|_| "not a release".to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some((tag, version)) = release_names(&release.tag_name) else {
        return Ok(None);
    };
    let asset = format!("GrokVoice-{version}.apk");
    let present = release.assets.iter().any(|item| item.name == asset);
    let url = if present {
        format!("https://github.com/Gabo-Tech/grok-voice-android/releases/download/{tag}/{asset}")
    } else {
        format!("https://github.com/Gabo-Tech/grok-voice-android/releases/tag/{tag}")
    };
    if !update_url_allowed(&url) {
        return Ok(None);
    }
    Ok(Some(Offer { version, tag, url }))
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
}

/// `v0.1.25` and `0.1.25` become up to four numeric parts. Anything else is malformed.
fn parse_version(raw: &str) -> Option<[u64; 4]> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 32 {
        return None;
    }
    let bare = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    if bare.is_empty() {
        return None;
    }
    let mut parts = [0u64; 4];
    let mut count = 0usize;
    for piece in bare.split('.') {
        if count >= 4
            || piece.is_empty()
            || piece.len() > 6
            || !piece.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        parts[count] = piece.parse().ok()?;
        count += 1;
    }
    if count == 0 { None } else { Some(parts) }
}

/// Tag kept for the download path, and the versionName used in `GrokVoice-<version>.apk`.
fn release_names(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 32 || trimmed.starts_with('V') {
        return None;
    }
    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    parse_version(trimmed)?;
    let tag = if trimmed.starts_with('v') {
        format!("v{version}")
    } else {
        version.to_string()
    };
    Some((tag, version.to_string()))
}

fn is_newer(published: &str, installed: &str) -> bool {
    match (parse_version(published), parse_version(installed)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

pub fn should_notify(installed: &str, published: &str, dismissed: &str) -> bool {
    if !is_newer(published, installed) {
        return false;
    }
    if dismissed.trim().is_empty() {
        return true;
    }
    match (parse_version(published), parse_version(dismissed)) {
        (Some(latest), Some(gone)) => latest > gone,
        (Some(_), None) => true,
        _ => false,
    }
}

pub fn should_check_now(last_attempt_ms: u64, last_ok: bool, now_ms: u64) -> bool {
    if last_attempt_ms == 0 || now_ms < last_attempt_ms {
        return true;
    }
    let interval = if last_ok {
        SUCCESS_INTERVAL_MS
    } else {
        FAILURE_INTERVAL_MS
    };
    now_ms - last_attempt_ms >= interval
}

pub fn should_post_notification(
    notified_version: &str,
    notified_at_ms: u64,
    version: &str,
    now_ms: u64,
) -> bool {
    if version.is_empty() {
        return false;
    }
    if notified_version != version || notified_at_ms == 0 || now_ms < notified_at_ms {
        return true;
    }
    now_ms - notified_at_ms >= SUCCESS_INTERVAL_MS
}

fn wait_ms(last_attempt_ms: u64, last_ok: bool, now_ms: u64) -> u64 {
    if should_check_now(last_attempt_ms, last_ok, now_ms) {
        return MIN_SLEEP_MS;
    }
    let interval = if last_ok {
        SUCCESS_INTERVAL_MS
    } else {
        FAILURE_INTERVAL_MS
    };
    let elapsed = now_ms.saturating_sub(last_attempt_ms);
    interval
        .saturating_sub(elapsed)
        .clamp(MIN_SLEEP_MS, SUCCESS_INTERVAL_MS)
}

/// Links the app may open for an update. Built here; never taken from the API body.
pub fn update_url_allowed(raw: &str) -> bool {
    if raw.len() > 200
        || raw
            .chars()
            .any(|c| c.is_whitespace() || c == '\\' || c == '@')
    {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    if url.query().is_some() || url.fragment().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let path = url.path();
    if path.contains("..") {
        return false;
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if host == "github.com" {
        if parts.len() < 4
            || parts[0] != "Gabo-Tech"
            || parts[1] != "grok-voice-android"
            || parts[2] != "releases"
        {
            return false;
        }
        if parts.len() == 5
            && parts[3] == "tag"
            && release_names(parts[4]).is_some_and(|(tag, _)| tag == parts[4])
        {
            return true;
        }
        if parts.len() == 6 && parts[3] == "download" {
            if let Some((tag, bare)) = release_names(parts[4]) {
                return tag == parts[4] && parts[5] == format!("GrokVoice-{bare}.apk");
            }
        }
        return false;
    }
    if host == "gabo-tech.github.io" {
        return parts.len() == 1 && parts[0] == "grok-voice-android";
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apk(version: &str) -> String {
        format!(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/download/v{version}/GrokVoice-{version}.apk"
        )
    }

    #[test]
    fn newer_equal_older_and_malformed_tags() {
        assert!(is_newer("v0.1.25", "0.1.24"));
        assert!(!is_newer("v0.1.24", "0.1.24"));
        assert!(!is_newer("0.1.24", "v0.1.24"));
        assert!(!is_newer("v0.1.23", "0.1.24"));
        assert!(is_newer("v0.1.100", "0.1.24"));
        assert!(!is_newer("v0.1.9", "0.1.24"));
        assert!(is_newer("v0.2", "0.1.24"));
        assert_eq!(parse_version("v0.1.24"), parse_version("0.1.24"));
        assert_eq!(parse_version("V0.1.24"), parse_version("0.1.24"));
        for bad in [
            "",
            "v",
            "latest",
            "v0.1.24-beta",
            "1.2.3-rc1",
            "v0..1",
            "v0.1.24.0.1",
            "not-a-version",
        ] {
            assert!(parse_version(bad).is_none(), "{bad}");
            assert!(!is_newer(bad, "0.1.24"), "{bad}");
        }
        assert!(!is_newer("v0.1.25", "not-a-version"));
    }

    #[test]
    fn notifies_only_for_a_newer_undismissed_release() {
        assert!(should_notify("0.1.24", "v0.1.25", ""));
        assert!(!should_notify("0.1.24", "v0.1.24", ""));
        assert!(!should_notify("0.1.24", "v0.1.23", ""));
        assert!(!should_notify("0.1.24", "v0.1.24-beta", ""));
        assert!(!should_notify("0.1.24", "latest", ""));
        assert!(!should_notify("0.1.24", "v0.1.25", "0.1.25"));
        assert!(!should_notify("0.1.24", "v0.1.25", "v0.1.25"));
        assert!(should_notify("0.1.24", "v0.1.26", "0.1.25"));
        assert!(should_notify("0.1.24", "v0.1.25", "nope"));
    }

    #[test]
    fn checks_on_first_launch_then_waits() {
        let start = 1_700_000_000_000u64;
        assert!(should_check_now(0, false, start));
        assert!(!should_check_now(
            start,
            true,
            start + SUCCESS_INTERVAL_MS - 1
        ));
        assert!(should_check_now(start, true, start + SUCCESS_INTERVAL_MS));
        assert!(!should_check_now(
            start,
            false,
            start + FAILURE_INTERVAL_MS - 1
        ));
        assert!(should_check_now(start, false, start + FAILURE_INTERVAL_MS));
        assert!(should_check_now(start, true, start - 1));
        assert_eq!(wait_ms(start, true, start), SUCCESS_INTERVAL_MS);
        assert_eq!(wait_ms(start, false, start), FAILURE_INTERVAL_MS);
        assert_eq!(wait_ms(0, false, start), MIN_SLEEP_MS);
    }

    #[test]
    fn notification_is_once_per_version_until_the_next_day() {
        let now = 50_000u64;
        assert!(!should_post_notification("0.1.25", now, "", now));
        assert!(should_post_notification("", 0, "0.1.25", now));
        assert!(should_post_notification("0.1.24", now, "0.1.25", now));
        assert!(!should_post_notification(
            "0.1.25",
            now,
            "0.1.25",
            now + 1_000
        ));
        assert!(should_post_notification(
            "0.1.25",
            now,
            "0.1.25",
            now + SUCCESS_INTERVAL_MS
        ));
    }

    #[test]
    fn release_json_builds_our_apk_url_and_ignores_foreign_links() {
        let body = br#"{
            "tag_name": "v0.1.25",
            "draft": false,
            "prerelease": false,
            "html_url": "https://evil.example/phish",
            "assets": [{
                "name": "GrokVoice-0.1.25.apk",
                "browser_download_url": "https://evil.example/GrokVoice.apk"
            }]
        }"#;
        let offer = parse_latest_release(body).unwrap().unwrap();
        assert_eq!(offer.version, "0.1.25");
        assert_eq!(offer.tag, "v0.1.25");
        assert_eq!(offer.url, apk("0.1.25"));
        assert!(update_url_allowed(&offer.url));
        assert!(!update_url_allowed("https://evil.example/GrokVoice.apk"));
    }

    #[test]
    fn missing_asset_uses_the_release_page() {
        let body = br#"{"tag_name":"v0.1.25","draft":false,"prerelease":false,"assets":[{"name":"notes.txt"}]}"#;
        let offer = parse_latest_release(body).unwrap().unwrap();
        assert_eq!(
            offer.url,
            "https://github.com/Gabo-Tech/grok-voice-android/releases/tag/v0.1.25"
        );
    }

    #[test]
    fn draft_prerelease_and_bad_tags_are_not_updates() {
        assert!(
            parse_latest_release(br#"{"tag_name":"v0.1.25","draft":true,"prerelease":false}"#)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_latest_release(br#"{"tag_name":"v0.1.25","draft":false,"prerelease":true}"#)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_latest_release(br#"{"tag_name":"latest","draft":false,"prerelease":false}"#)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_latest_release(br#"{"tag_name":"v0.1.25-beta","prerelease":false}"#)
                .unwrap()
                .is_none()
        );
        assert!(parse_latest_release(b"not-json").is_err());
    }

    #[test]
    fn update_links_stay_on_this_repo() {
        assert!(update_url_allowed(&apk("0.1.25")));
        assert!(update_url_allowed(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/tag/v0.1.25"
        ));
        assert!(update_url_allowed(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/download/0.1.25/GrokVoice-0.1.25.apk"
        ));
        assert!(update_url_allowed(
            "https://gabo-tech.github.io/grok-voice-android"
        ));
        assert!(!update_url_allowed(
            "http://github.com/Gabo-Tech/grok-voice-android/releases/tag/v0.1.25"
        ));
        assert!(!update_url_allowed(
            "https://user:pw@github.com/Gabo-Tech/grok-voice-android/releases/tag/v0.1.25"
        ));
        assert!(!update_url_allowed(
            "https://github.com.evil.test/Gabo-Tech/grok-voice-android/releases/tag/v0.1.25"
        ));
        assert!(!update_url_allowed(
            "https://github.com/other/grok-voice-android/releases/tag/v0.1.25"
        ));
        assert!(!update_url_allowed(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/download/v0.1.25/Other.apk"
        ));
        assert!(!update_url_allowed(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/download/v0.1.25/GrokVoice-0.1.25.apk?evil=1"
        ));
        assert!(!update_url_allowed("https://evil.example/update.apk"));
    }

    #[test]
    fn dismissed_or_tampered_state_stays_hidden() {
        let url = apk("0.1.25");
        let state = UpdateState {
            version: "0.1.25".into(),
            tag: "v0.1.25".into(),
            url: url.clone(),
            ..UpdateState::default()
        };
        assert_eq!(visible(&state, "0.1.24").version, "0.1.25");
        assert!(visible(&state, "0.1.25").version.is_empty());
        let dismissed = UpdateState {
            dismissed_version: "0.1.25".into(),
            ..state.clone()
        };
        assert!(visible(&dismissed, "0.1.24").version.is_empty());
        let next = UpdateState {
            version: "0.1.26".into(),
            tag: "v0.1.26".into(),
            url: apk("0.1.26"),
            dismissed_version: "0.1.25".into(),
            ..UpdateState::default()
        };
        assert_eq!(visible(&next, "0.1.24").version, "0.1.26");
        let tampered = UpdateState {
            url: "https://evil.example/app.apk".into(),
            ..state
        };
        assert!(visible(&tampered, "0.1.24").version.is_empty());
    }
}
