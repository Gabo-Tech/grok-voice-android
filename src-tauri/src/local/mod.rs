//! On-device Talk: Moonshine STT, a small GGUF chat model, and Piper TTS.
//! Utterance mode, not full duplex. The phone tools stay on `actions::run`.

mod engine;
mod mode;
mod packs;
mod session;
mod turn;
mod vad;
mod web;

pub use mode::public_search_enabled;
pub use packs::{delete_pack, download_pack, model_dir, pack_status, packs_ready, PackInfo};
pub use session::{answer_text, run_session};
pub use web::lookup as public_search;
pub use web::{network_block, OFFLINE};

/// What to do with one vault account when a Talk session starts.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    Local,
    Xai,
    OpenAi,
    Skip(String),
}

pub fn route(provider: &str, packs_ready: bool) -> Route {
    match provider {
        "local" if packs_ready => Route::Local,
        "local" => Route::Skip(
            "On-device voice packs are not downloaded yet. Download them in Settings, under Voice keys.".into(),
        ),
        "xai" => Route::Xai,
        "openai" => Route::OpenAi,
        other => Route::Skip(format!(
            "{other} is saved on this phone, and its voice connection is not ready yet"
        )),
    }
}

/// Talk badge label. Secrets never appear here.
pub fn provider_label(provider: &str) -> &'static str {
    match provider {
        "local" => "Local",
        "xai" => "xAI",
        "openai" => "OpenAI",
        _ => "Voice",
    }
}

/// Indexes to try, in order.
///
/// The saved list is the order. A wake session can move a saved local account
/// to the front when packs are ready and the user asked for that. A local
/// account that is not in the list is never inserted.
pub fn attempt_order(providers: &[&str], wake: bool, prefer_local: bool, packs_ready: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..providers.len()).collect();
    if !(wake && prefer_local && packs_ready) {
        return order;
    }
    let Some(local_at) = providers.iter().position(|provider| *provider == "local") else {
        return order;
    };
    order.retain(|index| *index != local_at);
    order.insert(0, local_at);
    order
}

/// Auth, network, rate-limit, and model failures that should try the next key
/// before the session has produced audio.
pub fn retries_on_connect(message: &str) -> bool {
    let message = message.to_lowercase();
    [
        "401",
        "403",
        "408",
        "429",
        "404",
        "500",
        "502",
        "503",
        "504",
        "unauthorized",
        "invalid api key",
        "incorrect api key",
        "authentication",
        "rejected the request",
        "rate limit",
        "rate-limit",
        "too many requests",
        "quota",
        "timed out",
        "timeout",
        "connection",
        "network",
        "dns",
        "offline",
        "unreachable",
        "model",
        "not found",
        "does not exist",
        "handshake",
        "websocket",
        "tls",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub fn exhausted_message(labels: &[String], last: &str) -> String {
    let who = if labels.is_empty() {
        "No voice provider connected.".to_string()
    } else {
        format!("Couldn't reach {}.", labels.join(", then "))
    };
    let detail: String = last.trim().chars().take(160).collect();
    if detail.is_empty() || who.contains(&detail) {
        who
    } else {
        format!("{who} {detail}")
    }
}

/// About 1.4 GB free keeps Moonshine, SmolLM2 360M, and Piper from getting the process killed.
pub const MIN_FREE_MB: u64 = 1400;

pub fn memory_enough(avail_mb: u64, low_memory: bool) -> bool {
    !low_memory && (avail_mb == 0 || avail_mb >= MIN_FREE_MB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_is_used_when_packs_are_ready() {
        assert_eq!(route("local", true), Route::Local);
        assert_eq!(route("xai", false), Route::Xai);
    }

    #[test]
    fn missing_packs_skip_local_without_the_old_stub() {
        let Route::Skip(reason) = route("local", false) else {
            panic!("expected a skip");
        };
        assert!(!reason.to_lowercase().contains("connection is not ready"));
        assert!(reason.contains("not downloaded"));
    }

    #[test]
    fn openai_connects_and_unknown_providers_stay_unready() {
        assert_eq!(route("openai", false), Route::OpenAi);
        assert_eq!(route("openai", true), Route::OpenAi);
        let Route::Skip(reason) = route("azure", true) else {
            panic!("expected a skip");
        };
        assert!(reason.contains("not ready yet"));
    }

    #[test]
    fn failover_follows_the_saved_order() {
        let providers = ["openai", "xai", "local"];
        assert_eq!(attempt_order(&providers, false, false, true), vec![0, 1, 2]);
        assert_eq!(provider_label("openai"), "OpenAI");
        assert_eq!(provider_label("xai"), "xAI");
        assert_eq!(provider_label("local"), "Local");
    }

    #[test]
    fn wake_preference_moves_a_saved_local_account_first() {
        let providers = ["openai", "xai", "local"];
        assert_eq!(attempt_order(&providers, true, true, true), vec![2, 0, 1]);
    }

    #[test]
    fn wake_preference_does_not_invent_a_local_account() {
        let providers = ["openai", "xai"];
        assert_eq!(attempt_order(&providers, true, true, true), vec![0, 1]);
        let with_local = ["openai", "local", "xai"];
        assert_eq!(attempt_order(&with_local, false, true, true), vec![0, 1, 2]);
        assert_eq!(attempt_order(&with_local, true, false, true), vec![0, 1, 2]);
        assert_eq!(attempt_order(&with_local, true, true, false), vec![0, 1, 2]);
    }

    #[test]
    fn connect_failures_try_the_next_key() {
        assert!(retries_on_connect("OpenAI rejected the request (401): invalid api key"));
        assert!(retries_on_connect("timed out connecting to wss://api.openai.com"));
        assert!(retries_on_connect("HTTP 429 rate limit"));
        assert!(retries_on_connect("model gpt-realtime was not found"));
        assert!(!retries_on_connect("ended by user"));
        let message = exhausted_message(&["OpenAI".into(), "xAI".into(), "Local".into()], "timed out connecting");
        assert!(message.contains("OpenAI, then xAI, then Local"));
        assert!(message.contains("timed out"));
    }

    #[test]
    fn fallback_order_prefers_a_ready_local_account() {
        let accounts = [("local", true), ("xai", true)];
        let chosen = accounts.iter().find(|(provider, ready)| !matches!(route(provider, *ready), Route::Skip(_)));
        assert_eq!(chosen.map(|(provider, _)| *provider), Some("local"));
    }

    #[test]
    fn fallback_order_passes_a_missing_local_pack_to_xai() {
        let accounts = [("local", false), ("xai", true)];
        let chosen = accounts.iter().find_map(|(provider, ready)| match route(provider, *ready) {
            Route::Skip(_) => None,
            _ => Some(*provider),
        });
        assert_eq!(chosen, Some("xai"));
    }

    #[test]
    fn memory_bar_fails_closed_on_low_ram() {
        assert!(memory_enough(0, false));
        assert!(memory_enough(1600, false));
        assert!(!memory_enough(900, false));
        assert!(!memory_enough(4000, true));
    }
}
