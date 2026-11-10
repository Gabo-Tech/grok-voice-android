//! On-device Talk: Moonshine or a NeMo transducer for speech, a small GGUF chat model, and Piper TTS.
//! Utterance mode, not full duplex. The phone tools stay on `actions::run`.

mod engine;
#[cfg(target_os = "android")]
pub(crate) use engine::{notice_sample_rate, synthesize_notice};
mod gguf;
mod mode;
mod packs;
mod session;
mod turn;
mod vad;
mod web;

pub use mode::public_search_enabled;
pub use packs::{
    PackInfo, delete_pack, download_custom_model, download_pack, import_custom_file, model_dir,
    pack_status, packs_ready, select_model,
};
pub use session::{answer_text, run_session};
pub use web::lookup as public_search;
pub use web::{OFFLINE, network_block};

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
/// The saved list is the order. When packs are ready and the user asked to
/// prefer On device, a saved local account moves to the front for every Talk
/// start, including a tap, a typed message, and the wake word. A local account
/// that is not in the list is never inserted.
pub fn attempt_order(providers: &[&str], prefer_local: bool, packs_ready: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..providers.len()).collect();
    if !(prefer_local && packs_ready) {
        return order;
    }
    let Some(local_at) = providers.iter().position(|provider| *provider == "local") else {
        return order;
    };
    order.retain(|index| *index != local_at);
    order.insert(0, local_at);
    order
}

/// Typed Talk uses the same first provider as a voice session.
pub fn text_uses_local(providers: &[&str], prefer_local: bool, packs_ready: bool) -> bool {
    if !packs_ready {
        return false;
    }
    let order = attempt_order(providers, prefer_local, packs_ready);
    order
        .first()
        .is_some_and(|index| providers[*index] == "local")
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

/// About 1.4 GB free. Moonshine Base weighs more than Tiny, and SmolLM2 135M Q8
/// weighs less than 360M Q8, so the same headroom still covers speech, chat, and Piper.
/// A larger language model needs more free memory, about the size of its file on top
/// of speech and voice. The floor stays here so the default model is not blocked.
pub const MIN_FREE_MB: u64 = 1400;

pub fn memory_enough(avail_mb: u64, low_memory: bool) -> bool {
    memory_enough_for(avail_mb, low_memory, 0)
}

/// Free MB required before llama.cpp loads `model_bytes`.
///
/// `MIN_FREE_MB` already covers speech, Piper, and the default SmolLM2 135M
/// Q8_0 file (144_811_360 bytes). A larger chosen GGUF needs the extra file
/// size on top, so the load fails with a message instead of being killed.
pub fn required_free_mb(model_bytes: u64) -> u64 {
    const COVERED_BYTES: u64 = 144_811_360;
    let extra = mb(model_bytes).saturating_sub(mb(COVERED_BYTES));
    MIN_FREE_MB + extra
}

pub fn memory_enough_for(avail_mb: u64, low_memory: bool, model_bytes: u64) -> bool {
    !low_memory && (avail_mb == 0 || avail_mb >= required_free_mb(model_bytes))
}

fn mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
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
        assert_eq!(attempt_order(&providers, false, true), vec![0, 1, 2]);
        assert_eq!(provider_label("openai"), "OpenAI");
        assert_eq!(provider_label("xai"), "xAI");
        assert_eq!(provider_label("local"), "Local");
    }

    #[test]
    fn prefer_local_moves_a_saved_local_account_first_for_every_talk_start() {
        let providers = ["openai", "xai", "local"];
        assert_eq!(attempt_order(&providers, true, true), vec![2, 0, 1]);
        assert!(text_uses_local(&providers, true, true));
        assert!(!text_uses_local(&providers, false, true));
        assert!(text_uses_local(&["local", "xai"], false, true));
    }

    #[test]
    fn prefer_local_does_not_invent_a_local_account() {
        let providers = ["openai", "xai"];
        assert_eq!(attempt_order(&providers, true, true), vec![0, 1]);
        assert!(!text_uses_local(&providers, true, true));
        let with_local = ["openai", "local", "xai"];
        assert_eq!(attempt_order(&with_local, false, true), vec![0, 1, 2]);
        assert_eq!(attempt_order(&with_local, true, false), vec![0, 1, 2]);
        assert!(!text_uses_local(&with_local, true, false));
    }

    #[test]
    fn connect_failures_try_the_next_key() {
        assert!(retries_on_connect(
            "OpenAI rejected the request (401): invalid api key"
        ));
        assert!(retries_on_connect(
            "timed out connecting to wss://api.openai.com"
        ));
        assert!(retries_on_connect("HTTP 429 rate limit"));
        assert!(retries_on_connect("model gpt-realtime was not found"));
        assert!(!retries_on_connect("ended by user"));
        let message = exhausted_message(
            &["OpenAI".into(), "xAI".into(), "Local".into()],
            "timed out connecting",
        );
        assert!(message.contains("OpenAI, then xAI, then Local"));
        assert!(message.contains("timed out"));
    }

    #[test]
    fn fallback_order_prefers_a_ready_local_account() {
        let accounts = [("local", true), ("xai", true)];
        let chosen = accounts
            .iter()
            .find(|(provider, ready)| !matches!(route(provider, *ready), Route::Skip(_)));
        assert_eq!(chosen.map(|(provider, _)| *provider), Some("local"));
    }

    #[test]
    fn fallback_order_passes_a_missing_local_pack_to_xai() {
        let accounts = [("local", false), ("xai", true)];
        let chosen = accounts
            .iter()
            .find_map(|(provider, ready)| match route(provider, *ready) {
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

    #[test]
    fn a_larger_language_model_needs_its_extra_file_size() {
        assert_eq!(required_free_mb(0), MIN_FREE_MB);
        assert_eq!(required_free_mb(144_811_360), MIN_FREE_MB);
        let gemma = required_free_mb(806_058_240);
        assert!(gemma > MIN_FREE_MB);
        assert!(memory_enough_for(gemma, false, 806_058_240));
        assert!(!memory_enough_for(MIN_FREE_MB, false, 806_058_240));
        assert!(memory_enough_for(0, false, 806_058_240));
        assert!(!memory_enough_for(8_000, true, 144_811_360));
    }
}
