//! Which on-device voice speaks: Phonon when that language has a model, otherwise Piper,
//! and the Android voice after that. Valencian has no Phonon model, so it never selects one.

use std::path::Path;

use serde::Serialize;

use super::packs::piper_ready;
use super::phonon::{self, ModelSpec};

pub const LANGS: &[&str] = &["en", "es", "de", "pt", "ca"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineChoice {
    Auto,
    Piper,
    Phonon,
    /// Alerts use the Android voice. Talk on this phone still needs Phonon or Piper.
    Android,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpokenEngine {
    Phonon,
    Piper,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoicePlan {
    pub lang: &'static str,
    pub choice: EngineChoice,
    /// Try these in order. System is last when nothing on the phone can speak.
    pub steps: Vec<SpokenEngine>,
    pub model_id: Option<&'static str>,
}

impl VoicePlan {
    pub fn primary(&self) -> SpokenEngine {
        self.steps.first().copied().unwrap_or(SpokenEngine::System)
    }
}

/// App language for a voice. `ca-ES-valencia` and a bare `ca` are Valencian.
/// Anything else without a pack stays `other` and uses the Android voice.
pub fn speech_lang(raw: &str) -> &'static str {
    let tag = raw.trim().to_ascii_lowercase().replace('_', "-");
    if tag.is_empty() {
        return "en";
    }
    if tag.contains("valencia") || tag == "ca" || tag.starts_with("ca-") {
        return "ca";
    }
    if tag.starts_with("pt") {
        return "pt";
    }
    if tag.starts_with("de") {
        return "de";
    }
    if tag.starts_with("es") {
        return "es";
    }
    if tag.starts_with("en") {
        return "en";
    }
    "other"
}

pub fn notice_lang(locale: &str, phone_lang: &str) -> &'static str {
    let tag = if locale.trim().is_empty() {
        phone_lang
    } else {
        locale
    };
    speech_lang(tag)
}

pub fn parse_choice(raw: &str) -> EngineChoice {
    match raw.trim().to_ascii_lowercase().as_str() {
        "piper" => EngineChoice::Piper,
        "phonon" => EngineChoice::Phonon,
        "android" | "system" => EngineChoice::Android,
        _ => EngineChoice::Auto,
    }
}

pub fn choice_label(choice: EngineChoice) -> &'static str {
    match choice {
        EngineChoice::Auto => "auto",
        EngineChoice::Piper => "piper",
        EngineChoice::Phonon => "phonon",
        EngineChoice::Android => "android",
    }
}

pub fn engine_label(engine: SpokenEngine) -> &'static str {
    match engine {
        SpokenEngine::Phonon => "phonon",
        SpokenEngine::Piper => "piper",
        SpokenEngine::System => "system",
    }
}

fn pairs(map: &str) -> Vec<(&str, &str)> {
    map.split(',')
        .filter_map(|part| {
            let part = part.trim();
            let (key, value) = part.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

pub fn choice_for(map: &str, lang: &str) -> EngineChoice {
    let lang = speech_lang(lang);
    pairs(map)
        .into_iter()
        .find(|(key, _)| speech_lang(key) == lang)
        .map(|(_, value)| parse_choice(value))
        .unwrap_or(EngineChoice::Auto)
}

pub fn model_pref(map: &str, lang: &str) -> Option<String> {
    let lang = speech_lang(lang);
    pairs(map).into_iter().find_map(|(key, value)| {
        if speech_lang(key) != lang {
            return None;
        }
        let model = phonon::model_by_id(value)?;
        model.speaks(lang).then(|| model.id.to_string())
    })
}

pub fn voice_pref(map: &str, lang: &str) -> String {
    let lang = speech_lang(lang);
    pairs(map)
        .into_iter()
        .find(|(key, _)| speech_lang(key) == lang)
        .map(|(_, value)| value.to_string())
        .filter(|value| phonon::valid_voice_id(value))
        .unwrap_or_default()
}

pub fn set_pair(map: &str, lang: &str, value: &str) -> String {
    let lang = speech_lang(lang);
    if !LANGS.contains(&lang) {
        return stored_pairs(map, |_, stored| Some(stored.to_string()));
    }
    let mut next = String::new();
    let mut wrote = false;
    for id in LANGS {
        let stored = if *id == lang {
            wrote = true;
            value.trim()
        } else {
            pairs(map)
                .into_iter()
                .find(|(key, _)| speech_lang(key) == *id)
                .map(|(_, stored)| stored)
                .unwrap_or("")
        };
        if stored.is_empty() {
            continue;
        }
        if !next.is_empty() {
            next.push(',');
        }
        next.push_str(id);
        next.push('=');
        next.push_str(stored);
    }
    if !wrote && !value.trim().is_empty() {
        if !next.is_empty() {
            next.push(',');
        }
        next.push_str(lang);
        next.push('=');
        next.push_str(value.trim());
    }
    next
}

fn stored_pairs(map: &str, keep: impl Fn(&str, &str) -> Option<String>) -> String {
    let mut next = String::new();
    for id in LANGS {
        let Some(stored) = pairs(map)
            .into_iter()
            .find(|(key, _)| speech_lang(key) == *id)
            .map(|(_, value)| value.to_string())
        else {
            continue;
        };
        let Some(value) = keep(id, &stored) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        if !next.is_empty() {
            next.push(',');
        }
        next.push_str(id);
        next.push('=');
        next.push_str(&value);
    }
    next
}

pub fn stored_engines(map: &str) -> String {
    stored_pairs(map, |_, value| {
        Some(choice_label(parse_choice(value)).to_string())
    })
}

pub fn stored_models(map: &str) -> String {
    stored_pairs(map, |lang, value| {
        let model = phonon::model_by_id(value)?;
        model.speaks(lang).then(|| model.id.to_string())
    })
}

pub fn stored_voices(map: &str) -> String {
    stored_pairs(map, |_, value| {
        phonon::valid_voice_id(value).then(|| value.to_string())
    })
}

pub fn models_for(lang: &str) -> Vec<&'static ModelSpec> {
    let lang = speech_lang(lang);
    phonon::MODELS
        .iter()
        .filter(|model| model.speaks(lang))
        .collect()
}

pub fn default_model(lang: &str) -> Option<&'static ModelSpec> {
    let lang = speech_lang(lang);
    match lang {
        "en" => phonon::model_by_id("phonon-31m-en"),
        "es" => phonon::model_by_id("phonon-40m-es"),
        "de" => phonon::model_by_id("phonon-40m-de"),
        "pt" => phonon::model_by_id("phonon-40m-pt"),
        _ => None,
    }
}

fn covers<'a>(installed: &[&'a str], lang: &str) -> Vec<&'static ModelSpec> {
    installed
        .iter()
        .filter_map(|id| phonon::model_by_id(id))
        .filter(|model| model.speaks(lang))
        .collect()
}

fn pick_model(
    lang: &str,
    preferred: Option<&str>,
    installed: &[&str],
) -> Option<&'static ModelSpec> {
    let ready = covers(installed, lang);
    if let Some(id) = preferred {
        if let Some(model) = ready.iter().find(|model| model.id == id) {
            return Some(*model);
        }
    }
    if let Some(fallback) = default_model(lang) {
        if ready.iter().any(|model| model.id == fallback.id) {
            return Some(fallback);
        }
    }
    ready
        .into_iter()
        .find(|model| !model.multilingual)
        .or_else(|| {
            phonon::model_by_id("phonon-40m-multilingual")
                .filter(|model| model.speaks(lang) && installed.contains(&model.id))
        })
}

/// The order Talk and spoken alerts try. A missing Phonon model is a fallback,
/// not an error: Piper is next, then the Android voice.
pub fn plan(
    lang: &str,
    choice: EngineChoice,
    preferred: Option<&str>,
    piper: bool,
    installed: &[&str],
) -> VoicePlan {
    let lang = speech_lang(lang);
    if choice == EngineChoice::Android {
        return VoicePlan {
            lang,
            choice,
            steps: vec![SpokenEngine::System],
            model_id: None,
        };
    }
    let model = pick_model(lang, preferred, installed);
    let mut steps = Vec::new();
    let want_phonon = matches!(choice, EngineChoice::Auto | EngineChoice::Phonon);
    let want_piper = match choice {
        EngineChoice::Piper => true,
        EngineChoice::Auto => true,
        EngineChoice::Phonon => model.is_none() || true,
        EngineChoice::Android => false,
    };
    if want_phonon {
        if model.is_some() {
            steps.push(SpokenEngine::Phonon);
        }
    }
    let allow_piper = match choice {
        EngineChoice::Piper => true,
        EngineChoice::Auto => true,
        EngineChoice::Phonon => true,
        EngineChoice::Android => false,
    };
    if allow_piper && want_piper && piper {
        steps.push(SpokenEngine::Piper);
    }
    // Explicit Piper does not speak with Phonon, even when a model is on the phone.
    if choice == EngineChoice::Piper {
        steps.retain(|step| *step != SpokenEngine::Phonon);
    }
    if steps.is_empty() || *steps.last().unwrap() != SpokenEngine::System {
        steps.push(SpokenEngine::System);
    }
    let model_id = steps
        .contains(&SpokenEngine::Phonon)
        .then(|| model.map(|spec| spec.id))
        .flatten();
    VoicePlan {
        lang,
        choice,
        steps,
        model_id,
    }
}

pub fn plan_in(root: &Path, lang: &str, engines: &str, models: &str) -> VoicePlan {
    let lang = speech_lang(lang);
    let installed = phonon::installed_ids(root);
    let installed_ref: Vec<&str> = installed.iter().map(String::as_str).collect();
    plan(
        lang,
        choice_for(engines, lang),
        model_pref(models, lang).as_deref(),
        piper_ready(root, lang),
        &installed_ref,
    )
}

pub fn status_note(plan: &VoicePlan) -> &'static str {
    let phonon = plan.steps.contains(&SpokenEngine::Phonon);
    let piper = plan.steps.contains(&SpokenEngine::Piper);
    if phonon && piper {
        return "Will use Phonon. If Phonon fails, Piper speaks, then the Android voice.";
    }
    if phonon {
        return "Will use Phonon. If Phonon fails, the Android voice speaks.";
    }
    if piper {
        return "Will use Piper. If Piper fails, the Android voice speaks.";
    }
    if plan.lang == "ca" {
        return "Valencian has no Phonon model and no Piper voice. This phone uses the Android voice.";
    }
    if plan.lang == "pt" {
        return "Portuguese has no Piper voice. Phonon speaks it after the model is on this phone. That model needs a Hugging Face token. Until then, the Android voice speaks.";
    }
    if plan.choice == EngineChoice::Phonon {
        return "Phonon is not downloaded for this language yet. This phone uses the Android voice until then.";
    }
    "Will use the Android voice."
}

pub fn preview_line(lang: &str) -> &'static str {
    match speech_lang(lang) {
        "es" => "Hola. Esta es la voz de este teléfono.",
        "de" => "Hallo. Das ist die Stimme auf diesem Telefon.",
        "pt" => "Olá. Esta é a voz neste telefone.",
        "ca" => "Hola. Esta és la veu d'este telèfon.",
        _ => "Hello. This is the voice on this phone.",
    }
}

pub fn piper_name(lang: &str) -> &'static str {
    match speech_lang(lang) {
        "es" => "Piper Davefx",
        "de" => "Piper Thorsten",
        "en" => "Piper Lessac",
        _ => "",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub langs: Vec<LangStatus>,
    pub models: Vec<ModelStatus>,
    pub voices_ready: Vec<String>,
    pub hf_has_token: bool,
    pub hf_hint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LangStatus {
    pub id: String,
    pub choice: String,
    pub resolved: String,
    pub model_id: String,
    pub voice_id: String,
    pub piper_ready: bool,
    pub phonon_ready: bool,
    pub note: String,
    pub piper_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub license: String,
    pub gated: bool,
    pub langs: Vec<String>,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub ready: bool,
}

pub fn status(
    root: &Path,
    engines: &str,
    models: &str,
    voices: &str,
    hf_has_token: bool,
    hf_hint: &str,
) -> VoiceStatus {
    let installed = phonon::installed_ids(root);
    let voices_ready = phonon::installed_voice_ids(root);
    let langs = LANGS
        .iter()
        .map(|lang| {
            let planned = plan_in(root, lang, engines, models);
            LangStatus {
                id: (*lang).to_string(),
                choice: choice_label(choice_for(engines, lang)).to_string(),
                resolved: engine_label(planned.primary()).to_string(),
                model_id: planned.model_id.unwrap_or("").to_string(),
                voice_id: voice_pref(voices, lang),
                piper_ready: piper_ready(root, lang),
                phonon_ready: planned.model_id.is_some(),
                note: status_note(&planned).to_string(),
                piper_name: piper_name(lang).to_string(),
            }
        })
        .collect();
    let models = phonon::MODELS
        .iter()
        .filter(|model| model.langs.iter().any(|lang| LANGS.contains(lang)))
        .map(|model| ModelStatus {
            id: model.id.to_string(),
            label: model.label.to_string(),
            detail: model.detail.to_string(),
            license: model.license.to_string(),
            gated: model.gated,
            langs: model.langs.iter().map(|lang| (*lang).to_string()).collect(),
            download_bytes: model.download_bytes(),
            installed_bytes: phonon::installed_bytes(root, model.id),
            ready: installed.iter().any(|id| id == model.id),
        })
        .collect();
    VoiceStatus {
        langs,
        models,
        voices_ready,
        hf_has_token,
        hf_hint: hf_hint.to_string(),
    }
}

/// Local Talk can start when speech recognition, the language model, and either
/// Piper or a Phonon model for that language are on the phone.
pub fn talk_ready(root: &Path, speech: &str, engines: &str, models: &str) -> bool {
    let lang = speech_lang(speech);
    if !matches!(lang, "en" | "es" | "de") {
        return false;
    }
    if !super::packs::llm_ready(root) {
        return false;
    }
    let heard = match lang {
        "de" => super::packs::pack_ready_public(root, "stt-de"),
        "es" => super::packs::pack_ready_public(root, "stt-es"),
        _ => super::packs::pack_ready_public(root, "stt"),
    };
    if !heard {
        return false;
    }
    let planned = plan_in(root, lang, engines, models);
    planned
        .steps
        .iter()
        .any(|step| matches!(step, SpokenEngine::Phonon | SpokenEngine::Piper))
}

pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let rate = rate.max(1);
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        pcm.extend_from_slice(&crate::audio::resample::f32_to_i16(*sample).to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed<'a>(ids: &'a [&'a str]) -> Vec<&'a str> {
        ids.to_vec()
    }

    #[test]
    fn languages_normalize_before_a_model_is_chosen() {
        assert_eq!(speech_lang("ca-ES-valencia"), "ca");
        assert_eq!(speech_lang("ca_ES_valencia"), "ca");
        assert_eq!(speech_lang("ca"), "ca");
        assert_eq!(speech_lang("pt-BR"), "pt");
        assert_eq!(speech_lang("es-MX"), "es");
        assert_eq!(speech_lang("de-DE"), "de");
        assert_eq!(speech_lang("en-US"), "en");
        assert_eq!(speech_lang(""), "en");
        assert_eq!(speech_lang("fr"), "other");
        assert_eq!(notice_lang("", "pt"), "pt");
        assert_eq!(notice_lang("ca-ES-valencia", "en"), "ca");
    }

    #[test]
    fn valencian_never_selects_phonon_and_falls_back_to_piper() {
        let none = plan("ca-ES-valencia", EngineChoice::Auto, None, false, &[]);
        assert_eq!(none.steps, vec![SpokenEngine::System]);
        assert!(none.model_id.is_none());
        assert!(status_note(&none).contains("Valencian"));
        assert!(models_for("ca").is_empty());
        let piper = plan(
            "ca",
            EngineChoice::Phonon,
            None,
            true,
            &["phonon-40m-multilingual"],
        );
        assert_eq!(piper.steps, vec![SpokenEngine::Piper, SpokenEngine::System]);
        assert!(piper.model_id.is_none());
    }

    #[test]
    fn explicit_android_does_not_speak_phonon_or_piper() {
        let forced = plan(
            "en",
            EngineChoice::Android,
            Some("phonon-31m-en"),
            true,
            &["phonon-31m-en"],
        );
        assert_eq!(forced.steps, vec![SpokenEngine::System]);
        assert!(forced.model_id.is_none());
        assert_eq!(choice_label(parse_choice("android")), "android");
        assert_eq!(parse_choice("system"), EngineChoice::Android);
        let stored = stored_engines("en=android,es=piper");
        assert_eq!(choice_for(&stored, "en"), EngineChoice::Android);
        assert_eq!(choice_for(&stored, "es"), EngineChoice::Piper);
    }

    #[test]
    fn auto_uses_phonon_when_a_model_is_installed_and_piper_after_it() {
        let both = plan(
            "en",
            EngineChoice::Auto,
            None,
            true,
            &installed(&["phonon-31m-en"]),
        );
        assert_eq!(
            both.steps,
            vec![
                SpokenEngine::Phonon,
                SpokenEngine::Piper,
                SpokenEngine::System
            ]
        );
        assert_eq!(both.model_id, Some("phonon-31m-en"));
        let only = plan(
            "en",
            EngineChoice::Auto,
            None,
            false,
            &installed(&["phonon-31m-en"]),
        );
        assert_eq!(only.steps, vec![SpokenEngine::Phonon, SpokenEngine::System]);
        let piper = plan("en", EngineChoice::Auto, None, true, &[]);
        assert_eq!(piper.steps, vec![SpokenEngine::Piper, SpokenEngine::System]);
        assert!(piper.model_id.is_none());
    }

    #[test]
    fn an_explicit_piper_choice_does_not_use_a_downloaded_phonon_model() {
        let plan = plan(
            "en",
            EngineChoice::Piper,
            None,
            true,
            &installed(&["phonon-31m-en"]),
        );
        assert_eq!(plan.steps, vec![SpokenEngine::Piper, SpokenEngine::System]);
        assert!(plan.model_id.is_none());
    }

    #[test]
    fn phonon_without_a_model_falls_back_to_piper_then_the_android_voice() {
        let piper = plan("es", EngineChoice::Phonon, None, true, &[]);
        assert_eq!(piper.steps, vec![SpokenEngine::Piper, SpokenEngine::System]);
        let none = plan("de", EngineChoice::Phonon, None, false, &[]);
        assert_eq!(none.steps, vec![SpokenEngine::System]);
        assert!(status_note(&none).contains("not downloaded"));
    }

    #[test]
    fn portuguese_uses_phonon_only_after_the_model_is_installed() {
        let missing = plan("pt", EngineChoice::Auto, None, false, &[]);
        assert_eq!(missing.steps, vec![SpokenEngine::System]);
        assert!(status_note(&missing).contains("Hugging Face token"));
        let ready = plan(
            "pt-BR",
            EngineChoice::Auto,
            None,
            false,
            &installed(&["phonon-40m-pt"]),
        );
        assert_eq!(ready.model_id, Some("phonon-40m-pt"));
        assert_eq!(
            ready.steps,
            vec![SpokenEngine::Phonon, SpokenEngine::System]
        );
        let shared = plan(
            "pt",
            EngineChoice::Auto,
            None,
            false,
            &installed(&["phonon-40m-multilingual"]),
        );
        assert_eq!(shared.model_id, Some("phonon-40m-multilingual"));
    }

    #[test]
    fn english_31m_is_public_and_other_phonon_models_are_gated() {
        let public = phonon::model_by_id("phonon-31m-en").unwrap();
        assert!(!public.gated);
        assert_eq!(public.license, "Apache-2.0");
        assert!(public.download_bytes() > 60_000_000);
        assert!(phonon::gate_error(public, "").is_none());
        assert!(phonon::auth_header(public, "hf_secret").is_none());
        for id in [
            "phonon-40m-en",
            "phonon-40m-es",
            "phonon-40m-de",
            "phonon-40m-pt",
            "phonon-40m-multilingual",
        ] {
            let model = phonon::model_by_id(id).unwrap();
            assert!(model.gated, "{id}");
            assert_eq!(model.license, "CC-BY-NC-4.0");
            assert!(
                phonon::gate_error(model, "")
                    .unwrap()
                    .contains("Hugging Face token")
            );
            assert!(phonon::gate_error(model, "hf_ok").is_none());
            let header = phonon::auth_header(model, "hf_ok").unwrap();
            assert!(header.starts_with("Bearer "));
            assert!(!header.contains("hf_ok") || header.ends_with("hf_ok"));
        }
        assert!(phonon::model_by_id("phonon-31m-en").unwrap().speaks("en"));
        assert!(!phonon::model_by_id("phonon-31m-en").unwrap().speaks("es"));
        assert!(!phonon::model_by_id("phonon-40m-fr").unwrap().speaks("en"));
    }

    #[test]
    fn a_preferred_model_wins_and_a_language_model_beats_multilingual() {
        let preferred = plan(
            "en",
            EngineChoice::Auto,
            Some("phonon-40m-multilingual"),
            true,
            &installed(&["phonon-31m-en", "phonon-40m-multilingual"]),
        );
        assert_eq!(preferred.model_id, Some("phonon-40m-multilingual"));
        let specific = plan(
            "es",
            EngineChoice::Auto,
            None,
            false,
            &installed(&["phonon-40m-es", "phonon-40m-multilingual"]),
        );
        assert_eq!(specific.model_id, Some("phonon-40m-es"));
        let ignored = plan(
            "es",
            EngineChoice::Auto,
            Some("phonon-31m-en"),
            false,
            &installed(&["phonon-31m-en"]),
        );
        assert!(ignored.model_id.is_none());
    }

    #[test]
    fn engine_maps_keep_one_choice_per_language() {
        let engines = stored_engines("es=phonon,ca=phonon,en=piper,nope=phonon,pt=auto");
        assert_eq!(choice_for(&engines, "es"), EngineChoice::Phonon);
        assert_eq!(choice_for(&engines, "ca-ES-valencia"), EngineChoice::Phonon);
        assert_eq!(choice_for(&engines, "en"), EngineChoice::Piper);
        assert_eq!(choice_for(&engines, "de"), EngineChoice::Auto);
        assert_eq!(choice_for(&engines, "pt"), EngineChoice::Auto);
        let models = stored_models("en=phonon-40m-fr,es=phonon-40m-es,de=not-a-model");
        assert!(model_pref(&models, "en").is_none());
        assert_eq!(model_pref(&models, "es").as_deref(), Some("phonon-40m-es"));
        let voices = stored_voices("en=LMDIYE0Oa377Qh6Z,es=../etc/passwd,de=bad voice");
        assert_eq!(voice_pref(&voices, "en"), "LMDIYE0Oa377Qh6Z");
        assert!(voice_pref(&voices, "es").is_empty());
        let updated = set_pair("en=auto,es=piper", "de", "phonon");
        assert_eq!(
            choice_for(&stored_engines(&updated), "de"),
            EngineChoice::Phonon
        );
        assert_eq!(
            choice_for(&stored_engines(&updated), "es"),
            EngineChoice::Piper
        );
    }

    #[test]
    fn phonon_urls_stay_on_hugging_face_and_english_needs_no_token() {
        let model = phonon::model_by_id("phonon-31m-en").unwrap();
        for file in model.files {
            let url = phonon::file_url(model, file);
            assert!(url.starts_with("https://huggingface.co/gradium/phonon-31m-en/resolve/"));
            assert!(url.contains(file.name));
            assert!(crate::http::https_url_allowed(
                &url,
                &["huggingface.co", "*.huggingface.co", "*.hf.co"]
            ));
        }
        let gated = phonon::model_by_id("phonon-40m-es").unwrap();
        assert!(phonon::file_url(gated, &gated.files[0]).contains("phonon-40m-es"));
        assert_eq!(phonon::refused_status(401), Some(phonon::GATED_REFUSAL));
        assert_eq!(phonon::refused_status(403), Some(phonon::GATED_REFUSAL));
        assert!(phonon::refused_status(200).is_none());
    }

    #[test]
    fn a_ready_checkpoint_is_the_one_auto_mode_speaks_with() {
        let root = std::env::temp_dir().join(format!("gv-phonon-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        phonon::write_placeholder(&root, "phonon-31m-en").unwrap();
        let planned = plan_in(&root, "en", "", "");
        assert_eq!(planned.model_id, Some("phonon-31m-en"));
        assert_eq!(planned.primary(), SpokenEngine::Phonon);
        assert!(!talk_ready(&root, "en", "", ""));
        super::super::packs::write_min_talk(&root, "en");
        assert!(talk_ready(&root, "en", "en=phonon", ""));
        assert!(!talk_ready(&root, "pt", "", ""));
        assert!(!talk_ready(&root, "ca", "", ""));
        let spanish = std::env::temp_dir().join(format!("gv-spanish-ready-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&spanish);
        fn touch(path: &std::path::Path) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, b"x").unwrap();
        }
        let paths = super::super::packs::model_paths_for(&spanish, "es");
        touch(&paths.encoder);
        touch(&paths.uncached);
        touch(&paths.joiner);
        touch(&paths.stt_tokens);
        touch(&paths.tts_model);
        touch(&paths.tts_tokens);
        std::fs::create_dir_all(&paths.tts_data).unwrap();
        touch(&spanish.join("llm").join(super::super::packs::LLM_FILE));
        assert!(
            talk_ready(&spanish, "es", "es=piper", ""),
            "Spanish Talk uses its own speech pack, not English Moonshine"
        );
        assert!(!super::super::packs::pack_ready_public(&spanish, "stt"));
        let _ = std::fs::remove_dir_all(&spanish);
        let piper = plan_in(&root, "en", "en=piper", "");
        assert_eq!(piper.primary(), SpokenEngine::System);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_lines_exist_for_every_app_language() {
        for lang in ["en", "es", "de", "pt", "ca-ES-valencia"] {
            assert!(!preview_line(lang).trim().is_empty(), "{lang}");
        }
        assert!(preview_line("ca").contains("este"));
        let wav = wav_bytes(&[0.0, 0.25, -0.25], 24_000);
        assert!(wav.starts_with(b"RIFF"));
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn new_voice_strings_are_translated() {
        let raw = include_str!("../../../src/lib/phonon-i18n.json");
        let value: serde_json::Value = serde_json::from_str(raw).expect("phonon i18n json");
        let locales = ["es", "de", "pt", "ca-ES-valencia"];
        let mut keys: Option<Vec<String>> = None;
        for locale in locales {
            let object = value
                .get(locale)
                .and_then(|item| item.as_object())
                .expect(locale);
            let mut these: Vec<String> = object.keys().cloned().collect();
            these.sort();
            assert!(these.len() > 20, "{locale}");
            for (key, text) in object {
                assert!(!key.trim().is_empty());
                let translated = text.as_str().unwrap_or("");
                assert!(!translated.trim().is_empty(), "{locale} missing {key}");
                let name = matches!(
                    key.as_str(),
                    "Auto"
                        | "Piper"
                        | "Phonon"
                        | "Piper Lessac"
                        | "Piper Davefx"
                        | "Piper Thorsten"
                );
                if !name {
                    assert_ne!(translated, key, "{locale} left {key} in English");
                }
            }
            for key in [
                "Will use Phonon. If Phonon fails, Piper speaks, then the Android voice.",
                "Will use Phonon. If Phonon fails, the Android voice speaks.",
                "Will use Piper. If Piper fails, the Android voice speaks.",
                "Will use the Android voice.",
                "Valencian has no Phonon model and no Piper voice. This phone uses the Android voice.",
                "Portuguese has no Piper voice. Phonon speaks it after the model is on this phone. That model needs a Hugging Face token. Until then, the Android voice speaks.",
                "Phonon is not downloaded for this language yet. This phone uses the Android voice until then.",
                "This model needs a Hugging Face token. Accept its license on the model page, then save the token in Settings. English Phonon 31m does not need a token.",
                "Hugging Face refused this model. Accept the license on its page and check the token. Nothing was downloaded another way.",
                "The download did not match the expected file.",
                "Voice preview runs in the Android app.",
            ] {
                assert!(
                    object.contains_key(key),
                    "{locale} missing spoken or error string {key}"
                );
            }
            if let Some(expected) = &keys {
                assert_eq!(&these, expected, "{locale} keys differ");
            } else {
                keys = Some(these);
            }
        }
    }
}
