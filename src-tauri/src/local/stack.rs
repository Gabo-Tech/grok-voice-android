//! Offline-first defaults and the setup tour.
//!
//! A new install hears English with Moonshine, thinks with SmolLM2, and speaks
//! with Phonon 31m. The setup tour downloads that pack only. Other languages,
//! Piper, and gated Phonon models stay optional in Settings. An existing file
//! keeps its voice, model, engine, and provider preference.

use serde::Serialize;

use super::phonon;
use super::speech;
use crate::settings::Settings;

pub const STEP_LANGUAGE: &str = "language";
pub const STEP_PACKS: &str = "packs";
pub const STEP_PACKS_RUN: &str = "packs-run";
pub const STEP_PREVIEW: &str = "preview";
pub const STEP_CLOUD: &str = "cloud";
pub const STEP_DONE: &str = "done";
pub const STEP_SKIPPED: &str = "skipped";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPiece {
    pub id: String,
    pub kind: String,
    pub bytes: u64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePlan {
    pub lang: String,
    pub ui_locale: String,
    /// `moonshine`, `nemo`, or `none`.
    pub hearing: String,
    pub brain: String,
    /// `phonon`, `piper`, or `android`.
    pub voice: String,
    /// Stored engine choice for this language.
    pub engine: String,
    pub phonon_model: String,
    pub offer_hf: bool,
    pub hf_model: String,
    pub pieces: Vec<PlanPiece>,
    pub bytes: u64,
    pub note: String,
    pub optional: Option<PlanPiece>,
}

pub fn normalize_step(raw: &str) -> String {
    match raw.trim() {
        STEP_LANGUAGE | STEP_PACKS | STEP_PACKS_RUN | STEP_PREVIEW | STEP_CLOUD | STEP_DONE
        | STEP_SKIPPED => raw.trim().to_string(),
        _ => String::new(),
    }
}

/// Brand-new settings. The offline voice is English Phonon 31m.
pub fn offline_first() -> Settings {
    Settings {
        prefer_local_on_wake: true,
        speech_engines: "en=phonon".into(),
        phonon_models: "en=phonon-31m-en".into(),
        setup_step: STEP_LANGUAGE.into(),
        ..Settings::default()
    }
}

/// Missing file becomes the offline stack. An existing file only gains a setup step.
pub fn migrate_loaded(existed: bool, mut settings: Settings) -> Settings {
    if !existed {
        return offline_first();
    }
    if normalize_step(&settings.setup_step).is_empty() {
        settings.setup_step = if settings.onboarded {
            STEP_DONE.into()
        } else {
            STEP_LANGUAGE.into()
        };
    } else {
        settings.setup_step = normalize_step(&settings.setup_step);
    }
    settings
}

/// The pack the first-run tour downloads. Always English, with no token.
pub fn setup_pack() -> OfflinePlan {
    let mut plan = offline_plan("en");
    plan.offer_hf = false;
    plan.hf_model.clear();
    plan.optional = None;
    plan.note = "The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.".into();
    plan
}

pub fn offline_plan(raw: &str) -> OfflinePlan {
    let lang = speech::speech_lang(raw);
    let lang = if lang == "other" { "en" } else { lang };
    let ui_locale = match lang {
        "es" => "es",
        "de" => "de",
        "pt" => "pt",
        "ca" => "ca-ES-valencia",
        _ => "en",
    };
    let (hearing, voice, engine, phonon_model, offer_hf, hf_model, note, pack_ids) = match lang {
        "en" => (
            "moonshine",
            "phonon",
            "phonon",
            "phonon-31m-en",
            false,
            "",
            "English uses Moonshine and Phonon. No account and no token.",
            vec!["stt", "llm"],
        ),
        "es" => (
            "nemo",
            "piper",
            "piper",
            "",
            true,
            "phonon-40m-es",
            "Spanish and German listen on this phone and speak with Piper. A Hugging Face token adds Phonon.",
            vec!["stt-es", "llm", "tts-es"],
        ),
        "de" => (
            "nemo",
            "piper",
            "piper",
            "",
            true,
            "phonon-40m-de",
            "Spanish and German listen on this phone and speak with Piper. A Hugging Face token adds Phonon.",
            vec!["stt-de", "llm", "tts-de"],
        ),
        "pt" => (
            "none",
            "android",
            "auto",
            "",
            true,
            "phonon-40m-pt",
            "Portuguese speaks with the Android voice until Phonon is downloaded with a Hugging Face token. Hearing you needs a cloud key.",
            vec![],
        ),
        _ => (
            "none",
            "android",
            "android",
            "",
            false,
            "",
            "Valencian speaks with the Android voice. There is no Phonon model and no Piper voice. Hearing you needs a cloud key.",
            vec![],
        ),
    };
    let mut pieces = Vec::new();
    for id in pack_ids {
        let Some(bytes) = super::packs::download_size(id) else {
            continue;
        };
        pieces.push(PlanPiece {
            id: id.into(),
            kind: "pack".into(),
            bytes,
            label: pack_label(id).into(),
        });
    }
    if !phonon_model.is_empty() {
        if let Some(model) = phonon::model_by_id(phonon_model) {
            pieces.push(PlanPiece {
                id: model.id.into(),
                kind: "phonon".into(),
                bytes: model.download_bytes(),
                label: model.label.into(),
            });
        }
    }
    let bytes = pieces.iter().map(|piece| piece.bytes).sum();
    let optional = if hf_model.is_empty() {
        None
    } else {
        phonon::model_by_id(hf_model).map(|model| PlanPiece {
            id: model.id.into(),
            kind: "phonon".into(),
            bytes: model.download_bytes(),
            label: model.label.into(),
        })
    };
    OfflinePlan {
        lang: lang.into(),
        ui_locale: ui_locale.into(),
        hearing: hearing.into(),
        brain: "smollm2".into(),
        voice: voice.into(),
        engine: engine.into(),
        phonon_model: phonon_model.into(),
        offer_hf,
        hf_model: hf_model.into(),
        pieces,
        bytes,
        note: note.into(),
        optional,
    }
}

fn pack_label(id: &str) -> &'static str {
    match id {
        "stt" => "Speech to text",
        "stt-es" => "Spanish speech to text",
        "stt-de" => "German speech to text",
        "tts" => "Voice",
        "tts-es" => "Spanish voice",
        "tts-de" => "German voice",
        "llm" => "SmolLM2 135M Instruct",
        _ => "Language model",
    }
}

/// Sets the screen language only. The offline voice stays whatever is already saved.
pub fn apply_setup_language(settings: &Settings, raw: &str) -> Settings {
    let mut next = settings.clone();
    next.locale = offline_plan(raw).ui_locale;
    next
}

/// Opt in to the gated 40m model. Piper stays the fallback when it is installed.
pub fn with_gated_phonon(settings: &Settings, raw: &str) -> Option<Settings> {
    let lang = speech::speech_lang(raw);
    let id = match lang {
        "es" => "phonon-40m-es",
        "de" => "phonon-40m-de",
        "pt" => "phonon-40m-pt",
        _ => return None,
    };
    let mut next = settings.clone();
    next.speech_engines = speech::set_pair(&next.speech_engines, lang, "phonon");
    next.phonon_models = speech::set_pair(&next.phonon_models, lang, id);
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::attempt_order;

    #[test]
    fn setup_downloads_the_english_pack_for_every_screen_language() {
        let english = setup_pack();
        assert_eq!(english.hearing, "moonshine");
        assert_eq!(english.brain, "smollm2");
        assert_eq!(english.voice, "phonon");
        assert_eq!(english.engine, "phonon");
        assert_eq!(english.lang, "en");
        assert_eq!(english.phonon_model, "phonon-31m-en");
        assert!(!english.offer_hf);
        assert!(english.hf_model.is_empty());
        assert!(english.optional.is_none());
        let ids: Vec<&str> = english.pieces.iter().map(|piece| piece.id.as_str()).collect();
        assert_eq!(ids, vec!["stt", "llm", "phonon-31m-en"]);
        assert!(english.bytes > 400_000_000);
        let phonon = phonon::model_by_id("phonon-31m-en").unwrap().download_bytes();
        assert_eq!(phonon, 65_051_089);
        assert!(english.note.contains("English"));
        assert!(!english.note.to_lowercase().contains("token required"));
    }

    #[test]
    fn fallback_order_mixes_local_and_cloud_without_inventing_a_provider() {
        assert_eq!(attempt_order(&["local"], true, false), vec![0]);
        let mixed = ["xai", "local", "openai"];
        assert_eq!(attempt_order(&mixed, false, true), vec![0, 1, 2]);
        assert_eq!(attempt_order(&mixed, true, true), vec![1, 0, 2]);
        assert_eq!(attempt_order(&mixed, true, false), vec![0, 1, 2]);
        assert_eq!(attempt_order(&["openai", "xai"], true, true), vec![0, 1]);
    }

    #[test]
    fn migration_keeps_a_saved_cloud_setup_and_new_files_start_offline() {
        let fresh = migrate_loaded(false, Settings::default());
        assert!(fresh.prefer_local_on_wake);
        assert_eq!(fresh.setup_step, "language");
        assert!(!fresh.onboarded);
        assert_eq!(fresh.speech_engines, "en=phonon");
        assert_eq!(fresh.phonon_models, "en=phonon-31m-en");
        assert_eq!(fresh.voice, Settings::default().voice);
        assert_eq!(fresh.model, "grok-voice-latest");

        let saved = Settings {
            onboarded: true,
            prefer_local_on_wake: false,
            voice: "ara".into(),
            openai_voice: "coral".into(),
            model: "grok-voice-latest".into(),
            speech_engines: "es=phonon".into(),
            phonon_models: "es=phonon-40m-es".into(),
            local_speech: "es".into(),
            locale: "es".into(),
            daily_budget_minutes: 20,
            ..Settings::default()
        };
        let migrated = migrate_loaded(true, saved.clone());
        assert_eq!(migrated.setup_step, "done");
        assert!(!migrated.prefer_local_on_wake);
        assert_eq!(migrated.voice, "ara");
        assert_eq!(migrated.openai_voice, "coral");
        assert_eq!(migrated.speech_engines, "es=phonon");
        assert_eq!(migrated.phonon_models, "es=phonon-40m-es");
        assert_eq!(migrated.local_speech, "es");
        assert_eq!(migrated.daily_budget_minutes, 20);
        assert!(migrated.onboarded);

        let mid = migrate_loaded(
            true,
            Settings {
                onboarded: false,
                prefer_local_on_wake: false,
                ..Settings::default()
            },
        );
        assert_eq!(mid.setup_step, "language");
        assert!(!mid.prefer_local_on_wake);
        assert!(mid.speech_engines.is_empty());
    }

    #[test]
    fn applying_a_language_changes_the_screen_and_keeps_the_english_voice() {
        let fresh = apply_setup_language(&offline_first(), "español");
        assert_eq!(fresh.locale, "es");
        assert_eq!(fresh.local_speech, "en");
        assert_eq!(fresh.speech_engines, "en=phonon");
        assert_eq!(fresh.phonon_models, "en=phonon-31m-en");
        assert!(fresh.prefer_local_on_wake);
        assert_eq!(fresh.voice, Settings::default().voice);

        let custom = Settings {
            speech_engines: "es=phonon".into(),
            phonon_models: "es=phonon-40m-es".into(),
            local_speech: "es".into(),
            voice: "rex".into(),
            prefer_local_on_wake: false,
            onboarded: true,
            ..Settings::default()
        };
        let kept = apply_setup_language(&custom, "de");
        assert_eq!(kept.locale, "de");
        assert_eq!(kept.local_speech, "es");
        assert_eq!(kept.speech_engines, "es=phonon");
        assert_eq!(kept.phonon_models, "es=phonon-40m-es");
        assert_eq!(kept.voice, "rex");
        assert!(!kept.prefer_local_on_wake);

        let gated = with_gated_phonon(&Settings::default(), "es").unwrap();
        assert_eq!(gated.speech_engines, "es=phonon");
        assert_eq!(gated.phonon_models, "es=phonon-40m-es");
        assert!(with_gated_phonon(&Settings::default(), "ca-ES-valencia").is_none());

        let valencian = apply_setup_language(&offline_first(), "ca-ES-valencia");
        assert_eq!(valencian.locale, "ca-ES-valencia");
        assert_eq!(valencian.local_speech, "en");
        assert_eq!(valencian.speech_engines, "en=phonon");
    }

    #[test]
    fn setup_copy_is_translated() {
        let raw = include_str!("../../../src/lib/setup-i18n.json");
        let value: serde_json::Value = serde_json::from_str(raw).expect("setup i18n");
        let locales = ["es", "de", "pt", "ca-ES-valencia"];
        let mut keys: Option<Vec<String>> = None;
        for locale in locales {
            let object = value.get(locale).and_then(|item| item.as_object()).expect(locale);
            let mut these: Vec<String> = object.keys().cloned().collect();
            these.sort();
            assert!(these.len() > 20, "{locale}");
            for (key, text) in object {
                let translated = text.as_str().unwrap_or("");
                assert!(!translated.trim().is_empty(), "{locale} {key}");
                let name = matches!(
                    key.as_str(),
                    "Phonon"
                        | "Piper"
                        | "Moonshine"
                        | "NeMo"
                        | "SmolLM2"
                        | "gpt-realtime"
                        | "xAI"
                        | "OpenAI"
                );
                if !name {
                    assert_ne!(translated, key, "{locale} left {key} in English");
                }
            }
            for key in [
                "Voice and AI",
                "Offline-ready",
                "Not offline yet",
                "Use Wi-Fi if you can. These files stay on the phone. If the download stops, setup continues from here.",
                "On-device voice packs are not downloaded yet. Download them in Settings, under Voice and AI.",
                "The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.",
                "This chooses the screen language. The offline voice stays English. Other languages can be added later in Voice and AI.",
            ] {
                assert!(object.contains_key(key), "{locale} missing {key}");
            }
            if let Some(expected) = &keys {
                assert_eq!(&these, expected, "{locale} keys differ");
            } else {
                keys = Some(these);
            }
        }
    }
}
