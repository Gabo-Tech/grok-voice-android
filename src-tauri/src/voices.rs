//! Built-in realtime voices.
//!
//! Grok / xAI ids are the voice table on
//! <https://docs.x.ai/developers/model-capabilities/audio/voice> (the same roster
//! Speech to Speech accepts on `session.update`). OpenAI ids are the built-in
//! Realtime session voices (`alloy`, `ash`, `ballad`, `coral`, `echo`, `sage`,
//! `shimmer`, `verse`, `marin`, `cedar`). An xAI id is never forwarded to OpenAI,
//! and an OpenAI id is never forwarded to xAI. Unknown stored values become the
//! provider default: `eve` for Grok, `marin` for OpenAI.

pub const XAI_DEFAULT: &str = "eve";
pub const OPENAI_DEFAULT: &str = "marin";

/// Doc order from the xAI voice table.
pub const XAI_VOICES: &[&str] = &[
    "carina", "zagan", "helix", "orion", "luna", "iris", "altair", "zenith", "perseus", "helios", "lux", "kepler",
    "rigel", "cosmo", "celeste", "ursa", "sirius", "lumen", "castor", "naksh", "atlas", "aurora", "liora", "ara",
    "eve", "leo", "rex", "sal",
];

/// OpenAI Realtime built-in voices. TTS-only names such as `fable` are not included.
pub const OPENAI_VOICES: &[&str] = &[
    "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse", "marin", "cedar",
];

pub fn xai_voice(raw: &str) -> String {
    canonical(raw, XAI_VOICES).unwrap_or(XAI_DEFAULT).to_string()
}

pub fn openai_voice(raw: &str) -> String {
    known_openai(raw).unwrap_or(OPENAI_DEFAULT).to_string()
}

pub fn known_openai(raw: &str) -> Option<&'static str> {
    canonical(raw, OPENAI_VOICES)
}

/// Talk may say either a Grok name or an OpenAI name in `voice`.
/// Grok names update the Grok voice. OpenAI names update only the OpenAI voice.
pub fn assign_voice(raw: &str) -> Result<VoiceChoice, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Say which voice to use.".into());
    }
    if let Some(id) = canonical(trimmed, XAI_VOICES) {
        return Ok(VoiceChoice::Xai(id.to_string()));
    }
    if let Some(id) = canonical(trimmed, OPENAI_VOICES) {
        return Ok(VoiceChoice::OpenAi(id.to_string()));
    }
    Err(
        "That voice is not available. Grok voices include eve, ara, rex, sal, leo, and the other built-in voices. OpenAI voices are alloy, ash, ballad, coral, echo, sage, shimmer, verse, marin, and cedar."
            .into(),
    )
}

pub enum VoiceChoice {
    Xai(String),
    OpenAi(String),
}

fn canonical<'a>(raw: &str, voices: &'a [&str]) -> Option<&'a str> {
    let folded = raw.trim().to_ascii_lowercase();
    voices.iter().copied().find(|id| *id == folded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_match_the_documented_builtin_rosters() {
        assert_eq!(XAI_VOICES.len(), 28);
        assert!(XAI_VOICES.contains(&"eve"));
        assert!(XAI_VOICES.contains(&"ara"));
        assert!(XAI_VOICES.contains(&"sal"));
        assert!(!XAI_VOICES.contains(&"marin"));
        assert_eq!(OPENAI_VOICES.len(), 10);
        assert!(OPENAI_VOICES.contains(&"marin"));
        assert!(OPENAI_VOICES.contains(&"cedar"));
        assert!(!OPENAI_VOICES.contains(&"eve"));
        assert!(!OPENAI_VOICES.contains(&"fable"));
        assert!(!OPENAI_VOICES.contains(&"nova"));
    }

    #[test]
    fn unknown_stored_voices_fall_back_without_crossing_providers() {
        assert_eq!(xai_voice("Eve"), "eve");
        assert_eq!(xai_voice("  Ara "), "ara");
        assert_eq!(xai_voice("marin"), "eve");
        assert_eq!(xai_voice("custom-voice"), "eve");
        assert_eq!(xai_voice(""), "eve");
        assert_eq!(openai_voice("Cedar"), "cedar");
        assert_eq!(openai_voice("eve"), "marin");
        assert_eq!(openai_voice("fable"), "marin");
        assert!(matches!(assign_voice("leo"), Ok(VoiceChoice::Xai(id)) if id == "leo"));
        assert!(matches!(assign_voice("verse"), Ok(VoiceChoice::OpenAi(id)) if id == "verse"));
        assert!(assign_voice("not-a-voice").is_err());
    }
}
