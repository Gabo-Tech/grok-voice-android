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
pub use session::run_session;
pub use web::lookup as public_search;

/// What to do with one vault account when a Talk session starts.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    Local,
    Xai,
    Skip(String),
}

pub fn route(provider: &str, packs_ready: bool) -> Route {
    match provider {
        "local" if packs_ready => Route::Local,
        "local" => Route::Skip(
            "On-device voice packs are not downloaded yet. Download them in Settings, under Voice keys.".into(),
        ),
        "xai" => Route::Xai,
        other => Route::Skip(format!(
            "{other} is saved on this phone, and its voice connection is not ready yet"
        )),
    }
}

/// About 1.4 GB free keeps Moonshine, Qwen2.5-0.5B Q4, and Piper from getting the process killed.
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
    fn other_providers_stay_unready() {
        let Route::Skip(reason) = route("openai", true) else {
            panic!("expected a skip");
        };
        assert!(reason.contains("not ready yet"));
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
