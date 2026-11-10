//! Custom alarm sounds stored with the in-app Time item.
//!
//! A file is an audio clip the user picked. The app copies it into its private
//! `alarm-sounds` folder and plays that copy on the alarm stream.
//!
//! A YouTube choice stores only the video id. Playback uses the official IFrame
//! Player when the alarm fires. This module never fetches, downloads, or
//! extracts media.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;

pub const FILE: &str = "file";
pub const YOUTUBE: &str = "youtube";
pub const DIR: &str = "alarm-sounds";
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;

pub const ERR_YOUTUBE: &str = "That is not a YouTube video link.";
pub const ERR_MISSING: &str = "That audio file is missing. Pick it again.";
pub const ERR_TOO_LARGE: &str = "That audio file is too large.";
pub const ERR_NOT_AUDIO: &str = "Choose an audio file.";
pub const ERR_ALARMS_ONLY: &str = "Custom sound is only for alarms.";
pub const ERR_NOT_SAVED: &str =
    "That audio file is not saved on this phone. Pick it in the alarm form.";
pub const ERR_BOTH: &str = "Choose either a YouTube link or an audio file.";
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub const ERR_PICKER: &str = "Could not open the file picker.";
pub const ERR_READ: &str = "Could not read that audio file.";
/// Shown by the Android picker. Host tests do not call that plugin.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub const ERR_NONE_CHOSEN: &str = "No file was chosen.";
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub const ERR_PLAYING: &str = "An alarm is already playing.";
pub const ERR_PHONE: &str = "Choosing an audio file works on the Android phone.";

const AUDIO_EXT: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "ogg", "opus", "flac", "mp4", "3gp", "webm", "amr", "oga",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundChoice {
    pub kind: String,
    pub reference: String,
    pub label: String,
}

impl SoundChoice {
    pub fn none() -> Self {
        Self {
            kind: String::new(),
            reference: String::new(),
            label: String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty() || self.reference.is_empty()
    }
}

pub fn detail(choice: &SoundChoice) -> String {
    match choice.kind.as_str() {
        YOUTUBE => {
            " Alarm sound is a YouTube video. It plays in this app and is not downloaded.".into()
        }
        FILE => format!(" Alarm sound is {}.", choice.label),
        _ => " Alarm sound is the phone default.".into(),
    }
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// Accept a watch, share, shorts, embed, or live URL, or a bare 11-character id.
/// Anything else is rejected. No network request is made.
pub fn parse_youtube_id(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 500 {
        return Err(ERR_YOUTUBE.into());
    }
    if is_video_id(raw) {
        return Ok(raw.to_string());
    }
    let Some(rest) = raw.strip_prefix("https://") else {
        return Err(ERR_YOUTUBE.into());
    };
    if rest.contains('@') || rest.contains('\\') || rest.contains(' ') {
        return Err(ERR_YOUTUBE.into());
    }
    let rest = rest.split('#').next().unwrap_or(rest);
    let (host_raw, after) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if host_raw.is_empty()
        || host_raw.contains(':')
        || !host_raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(ERR_YOUTUBE.into());
    }
    let host = host_raw.to_ascii_lowercase();
    let (path, query) = match after.split_once('?') {
        Some((path, query)) => (path, query),
        None => (after, ""),
    };
    let id = if host == "youtu.be" || host == "www.youtu.be" {
        let seg = path.trim_start_matches('/');
        if seg.is_empty() || seg.contains('/') {
            return Err(ERR_YOUTUBE.into());
        }
        seg.to_string()
    } else if matches!(
        host.as_str(),
        "youtube.com"
            | "www.youtube.com"
            | "m.youtube.com"
            | "music.youtube.com"
            | "youtube-nocookie.com"
            | "www.youtube-nocookie.com"
    ) {
        let path = path.trim_start_matches('/');
        path_or_query_id(path, query)?
    } else {
        return Err(ERR_YOUTUBE.into());
    };
    if !is_video_id(&id) {
        return Err(ERR_YOUTUBE.into());
    }
    Ok(id)
}

fn path_or_query_id(path: &str, query: &str) -> Result<String, String> {
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let head = parts.next().unwrap_or("");
    if matches!(head, "shorts" | "embed" | "live" | "v") {
        let id = parts.next().ok_or(ERR_YOUTUBE)?;
        if parts.next().is_some() {
            return Err(ERR_YOUTUBE.into());
        }
        return Ok(id.to_string());
    }
    if head == "watch" || head.is_empty() {
        return query_video_id(query).ok_or_else(|| ERR_YOUTUBE.into());
    }
    Err(ERR_YOUTUBE.into())
}

fn query_video_id(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut bits = pair.splitn(2, '=');
        let key = bits.next().unwrap_or("");
        let value = bits.next().unwrap_or("");
        if key == "v" && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn is_video_id(value: &str) -> bool {
    value.len() == 11
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn youtube_choice(raw: &str) -> Result<SoundChoice, String> {
    let id = parse_youtube_id(raw)?;
    Ok(SoundChoice {
        kind: YOUTUBE.into(),
        reference: id.clone(),
        label: watch_url(&id),
    })
}

pub fn audio_extension(name: &str) -> Option<&str> {
    let ext = name.rsplit('.').next()?;
    let ext = AUDIO_EXT
        .iter()
        .copied()
        .find(|item| item.eq_ignore_ascii_case(ext))?;
    Some(ext)
}

pub fn sanitize_stem(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    let stem = if audio_extension(base).is_some() {
        match base.rfind('.') {
            Some(i) => &base[..i],
            None => base,
        }
    } else {
        base
    };
    let mut out = String::new();
    let mut dash = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "audio".into() } else { out }
}

pub fn normalize_relative(reference: &str) -> Result<String, String> {
    let reference = reference.trim().replace('\\', "/");
    if reference.contains("..") || reference.starts_with('/') || reference.contains(':') {
        return Err(ERR_NOT_SAVED.into());
    }
    let mut parts = reference.split('/').filter(|part| !part.is_empty());
    let dir = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if parts.next().is_some() || dir != DIR || name.is_empty() {
        return Err(ERR_NOT_SAVED.into());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
    {
        return Err(ERR_NOT_SAVED.into());
    }
    if audio_extension(name).is_none() {
        return Err(ERR_NOT_AUDIO.into());
    }
    Ok(format!("{DIR}/{name}"))
}

pub fn resolve_file(sounds_dir: &Path, reference: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(reference)?;
    let name = relative.trim_start_matches(&format!("{DIR}/"));
    let root = sounds_dir
        .canonicalize()
        .map_err(|_| ERR_MISSING.to_string())?;
    let path = root.join(name);
    let canon = path.canonicalize().map_err(|_| ERR_MISSING.to_string())?;
    if canon.parent() != Some(root.as_path()) {
        return Err(ERR_NOT_SAVED.into());
    }
    Ok(canon)
}

pub fn file_choice(sounds_dir: &Path, reference: &str, label: &str) -> Result<SoundChoice, String> {
    let relative = normalize_relative(reference)?;
    let path = resolve_file(sounds_dir, &relative)?;
    let meta = fs::metadata(&path).map_err(|_| ERR_MISSING.to_string())?;
    if !meta.is_file() {
        return Err(ERR_MISSING.into());
    }
    if meta.len() == 0 {
        return Err(ERR_READ.into());
    }
    if meta.len() > MAX_BYTES {
        return Err(ERR_TOO_LARGE.into());
    }
    let fallback = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Audio");
    let label = clean_label(label).unwrap_or_else(|| fallback.to_string());
    Ok(SoundChoice {
        kind: FILE.into(),
        reference: relative,
        label,
    })
}

fn clean_label(label: &str) -> Option<String> {
    let label: String = label
        .chars()
        .filter(|ch| !ch.is_control())
        .take(120)
        .collect();
    let label = label.trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

pub fn match_saved_file(sounds_dir: &Path, spoken: &str) -> Result<String, String> {
    let spoken = spoken.trim();
    if spoken.is_empty() {
        return Err(ERR_NOT_SAVED.into());
    }
    if spoken.replace('\\', "/").contains("alarm-sounds/") {
        let relative = normalize_relative(spoken)?;
        let _ = resolve_file(sounds_dir, &relative)?;
        return Ok(relative);
    }
    let wanted_name = spoken
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(spoken)
        .trim()
        .to_ascii_lowercase();
    let wanted_stem = sanitize_stem(spoken);
    let entries = fs::read_dir(sounds_dir).map_err(|_| ERR_NOT_SAVED.to_string())?;
    let mut strong = Vec::new();
    let mut weak = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if audio_extension(&name).is_none() {
            continue;
        }
        let relative = format!("{DIR}/{name}");
        if name.eq_ignore_ascii_case(&wanted_name) || sanitize_stem(&name) == wanted_stem {
            strong.push(relative);
            continue;
        }
        let stem = sanitize_stem(&name);
        if let Some(rest) = stem.strip_prefix(&format!("{wanted_stem}-")) {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                weak.push(relative);
            }
        }
    }
    if strong.len() == 1 {
        return Ok(strong.remove(0));
    }
    if strong.is_empty() && weak.len() == 1 {
        return Ok(weak.remove(0));
    }
    Err(ERR_NOT_SAVED.into())
}

pub fn from_form(
    kind: &str,
    reference: &str,
    label: &str,
    sounds_dir: &Path,
) -> Result<SoundChoice, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "" | "default" | "none" | "clear" | "off" => Ok(SoundChoice::none()),
        FILE | "audio" => file_choice(sounds_dir, reference, label),
        YOUTUBE | "video" => {
            let raw = if reference.trim().is_empty() {
                label
            } else {
                reference
            };
            youtube_choice(raw)
        }
        _ => Err(ERR_YOUTUBE.into()),
    }
}

pub fn from_tool(args: &Value, sounds_dir: &Path) -> Result<Option<SoundChoice>, String> {
    let has = ["sound", "youtubeUrl", "soundUrl", "soundFile"]
        .iter()
        .any(|key| args.get(key).is_some());
    if !has {
        return Ok(None);
    }
    let sound_raw = args
        .get("sound")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let sound_key = sound_raw.to_ascii_lowercase();
    let youtube = first_text(args, &["youtubeUrl", "soundUrl"]);
    let file = first_text(args, &["soundFile"]);
    if matches!(sound_key.as_str(), "default" | "none" | "clear" | "off") {
        if youtube.is_some() || file.is_some() {
            return Err(ERR_BOTH.into());
        }
        return Ok(Some(SoundChoice::none()));
    }
    if youtube.is_some() && file.is_some() {
        return Err(ERR_BOTH.into());
    }
    if (sound_key == FILE || sound_key == "audio") && youtube.is_some() {
        return Err(ERR_BOTH.into());
    }
    if (sound_key == YOUTUBE || sound_key == "video") && file.is_some() {
        return Err(ERR_BOTH.into());
    }
    if let Some(url) = youtube {
        return Ok(Some(youtube_choice(url)?));
    }
    if let Some(file) = file {
        let relative = match_saved_file(sounds_dir, file)?;
        let label = file.rsplit(['/', '\\']).next().unwrap_or(file);
        return Ok(Some(file_choice(sounds_dir, &relative, label)?));
    }
    if sound_raw.starts_with("https://") || sound_key.contains("youtu") {
        return Ok(Some(youtube_choice(sound_raw)?));
    }
    if sound_key == YOUTUBE || sound_key == "video" {
        return Err(ERR_YOUTUBE.into());
    }
    if sound_key == FILE || sound_key == "audio" {
        return Err(ERR_NOT_SAVED.into());
    }
    if sound_raw.is_empty() {
        return Err(ERR_YOUTUBE.into());
    }
    Err(ERR_YOUTUBE.into())
}

fn first_text<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        args.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
    })
}

pub fn delete_if_unused(sounds_dir: &Path, relative: &str, still_used: &[String]) {
    if relative.is_empty() || still_used.iter().any(|used| used == relative) {
        return;
    }
    if let Ok(path) = resolve_file(sounds_dir, relative) {
        let _ = fs::remove_file(path);
    }
}

/// Drop copies that no alarm references. A file picked moments ago is kept so
/// an open form can still save it.
pub fn sweep_orphans(sounds_dir: &Path, used: &[String]) {
    let Ok(entries) = fs::read_dir(sounds_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if audio_extension(&name).is_none() {
            continue;
        }
        let relative = format!("{DIR}/{name}");
        if used.iter().any(|item| item == &relative) {
            continue;
        }
        let recent = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(2 * 60 * 60));
        if recent {
            continue;
        }
        let _ = fs::remove_file(entry.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gv-sounds-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn touch(dir: &Path, name: &str, bytes: &[u8]) -> String {
        fs::write(dir.join(name), bytes).unwrap();
        format!("{DIR}/{name}")
    }

    #[test]
    fn youtube_links_keep_only_the_video_id() {
        for (raw, id) in [
            ("dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            ("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            (
                "https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=12",
                "dQw4w9WgXcQ",
            ),
            (
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
                "dQw4w9WgXcQ",
            ),
            ("https://youtu.be/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            ("https://www.youtube.com/shorts/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            ("https://www.youtube.com/embed/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            ("https://www.youtube.com/live/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            (
                "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
                "dQw4w9WgXcQ",
            ),
        ] {
            let choice = youtube_choice(raw).unwrap();
            assert_eq!(choice.reference, id, "{raw}");
            assert_eq!(choice.kind, YOUTUBE);
            assert_eq!(choice.label, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        }
    }

    #[test]
    fn youtube_rejects_pages_that_are_not_one_video() {
        for raw in [
            "",
            "not a url",
            "http://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://user:pw@www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://evil.example/watch?v=dQw4w9WgXcQ",
            "https://youtube.com.evil.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/@channel",
            "https://youtu.be/dQw4w9WgXcQ/extra",
            "javascript:alert(1)",
        ] {
            assert_eq!(parse_youtube_id(raw).unwrap_err(), ERR_YOUTUBE, "{raw}");
        }
    }

    #[test]
    fn a_picked_file_must_stay_inside_alarm_storage() {
        let sounds = dir("file");
        let relative = touch(&sounds, "morning.mp3", b"mp3");
        let choice = file_choice(&sounds, &relative, "Morning.mp3").unwrap();
        assert_eq!(choice.kind, FILE);
        assert_eq!(choice.reference, "alarm-sounds/morning.mp3");
        assert_eq!(choice.label, "Morning.mp3");
        assert_eq!(
            file_choice(&sounds, "alarm-sounds/../morning.mp3", "x").unwrap_err(),
            ERR_NOT_SAVED
        );
        assert_eq!(
            file_choice(&sounds, "/tmp/morning.mp3", "x").unwrap_err(),
            ERR_NOT_SAVED
        );
        assert_eq!(
            file_choice(&sounds, "alarm-sounds/notes.txt", "x").unwrap_err(),
            ERR_NOT_AUDIO
        );
        assert_eq!(
            file_choice(&sounds, "alarm-sounds/gone.mp3", "x").unwrap_err(),
            ERR_MISSING
        );
        fs::write(sounds.join("empty.mp3"), b"").unwrap();
        assert_eq!(
            file_choice(&sounds, "alarm-sounds/empty.mp3", "x").unwrap_err(),
            ERR_READ
        );
        fs::write(sounds.join("big.mp3"), vec![1u8; (MAX_BYTES as usize) + 1]).unwrap();
        assert_eq!(
            file_choice(&sounds, "alarm-sounds/big.mp3", "x").unwrap_err(),
            ERR_TOO_LARGE
        );
        let _ = fs::remove_dir_all(&sounds);
    }

    #[test]
    fn talk_finds_a_file_by_the_name_the_user_says() {
        let sounds = dir("match");
        touch(&sounds, "my-song.mp3", b"song");
        touch(&sounds, "other.ogg", b"ogg");
        assert_eq!(
            match_saved_file(&sounds, "My Song.mp3").unwrap(),
            "alarm-sounds/my-song.mp3"
        );
        assert_eq!(
            match_saved_file(&sounds, "my-song").unwrap(),
            "alarm-sounds/my-song.mp3"
        );
        assert_eq!(
            match_saved_file(&sounds, "alarm-sounds/other.ogg").unwrap(),
            "alarm-sounds/other.ogg"
        );
        assert_eq!(
            match_saved_file(&sounds, "missing.mp3").unwrap_err(),
            ERR_NOT_SAVED
        );
        touch(&sounds, "my-song-2.mp3", b"two");
        assert_eq!(
            match_saved_file(&sounds, "my song").unwrap(),
            "alarm-sounds/my-song.mp3"
        );
        touch(&sounds, "wake-up.mp3", b"a");
        touch(&sounds, "wake-up.ogg", b"b");
        assert_eq!(
            match_saved_file(&sounds, "wake up").unwrap_err(),
            ERR_NOT_SAVED
        );
        assert_eq!(sanitize_stem("My Song (live).mp3"), "my-song-live");
        let _ = fs::remove_dir_all(&sounds);
    }

    #[test]
    fn tool_args_set_one_sound_or_clear_it() {
        let sounds = dir("tool");
        touch(&sounds, "wake.mp3", b"wake");
        let youtube = from_tool(
            &json!({"youtubeUrl": "https://youtu.be/dQw4w9WgXcQ"}),
            &sounds,
        )
        .unwrap()
        .unwrap();
        assert_eq!(youtube.reference, "dQw4w9WgXcQ");
        let file = from_tool(&json!({"soundFile": "wake.mp3"}), &sounds)
            .unwrap()
            .unwrap();
        assert_eq!(file.reference, "alarm-sounds/wake.mp3");
        assert!(from_tool(&json!({"hour": 7}), &sounds).unwrap().is_none());
        assert!(
            from_tool(&json!({"sound": "default"}), &sounds)
                .unwrap()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            from_tool(
                &json!({"youtubeUrl": "https://youtu.be/dQw4w9WgXcQ", "soundFile": "wake.mp3"}),
                &sounds
            )
            .unwrap_err(),
            ERR_BOTH
        );
        assert_eq!(
            from_tool(&json!({"youtubeUrl": "https://example.com"}), &sounds).unwrap_err(),
            ERR_YOUTUBE
        );
        assert_eq!(
            from_tool(&json!({"sound": "file"}), &sounds).unwrap_err(),
            ERR_NOT_SAVED
        );
        let _ = fs::remove_dir_all(&sounds);
    }

    #[test]
    fn orphans_go_away_and_a_fresh_pick_stays() {
        let sounds = dir("sweep");
        touch(&sounds, "keep.mp3", b"keep");
        touch(&sounds, "old.mp3", b"old");
        let old = sounds.join("old.mp3");
        let ago = std::time::SystemTime::now() - Duration::from_secs(3 * 60 * 60);
        let _ = filetime_set(&old, ago);
        touch(&sounds, "fresh.mp3", b"fresh");
        sweep_orphans(&sounds, &["alarm-sounds/keep.mp3".into()]);
        assert!(sounds.join("keep.mp3").is_file());
        assert!(sounds.join("fresh.mp3").is_file());
        assert!(!sounds.join("old.mp3").exists());
        delete_if_unused(&sounds, "alarm-sounds/keep.mp3", &[]);
        assert!(!sounds.join("keep.mp3").exists());
        let _ = fs::remove_dir_all(&sounds);
    }

    #[test]
    fn youtube_playback_uses_the_official_player_and_speech_still_runs() {
        let activity =
            include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/AlarmSoundActivity.kt");
        let sounds =
            include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/AlarmSounds.kt");
        let spoken =
            include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/SpokenAlerts.kt");
        let phone =
            include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/PhoneActions.kt");
        assert!(activity.contains("https://www.youtube.com/iframe_api"));
        assert!(!activity.contains("youtubei/v1"));
        assert!(!activity.contains("yt-dlp"));
        assert!(!activity.contains("youtube-dl"));
        assert!(!sounds.contains("youtubei/v1"));
        assert!(!sounds.contains("/get_video_info"));
        assert!(spoken.contains("yieldForSpeech"));
        assert!(spoken.contains("resumeAfterSpeech"));
        assert!(phone.contains("grok_alarm_sound"));
        assert!(!phone.contains("ACTION_SET_ALARM"));
        let yield_at = spoken.find("yieldForSpeech").unwrap();
        let speak_at = spoken.find("playPiper").unwrap();
        assert!(yield_at < speak_at);
    }

    fn filetime_set(path: &Path, when: std::time::SystemTime) -> std::io::Result<()> {
        let file = fs::File::options().write(true).open(path)?;
        file.set_modified(when)
    }
}
