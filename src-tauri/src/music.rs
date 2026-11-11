//! Match local audio files and playlists for Talk.
//! Scanning and playback stay on the phone. This module only ranks what the phone found.
//! YouTube requests are refused here so they keep using `open_youtube`.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Track {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default, rename = "track")]
    pub track_no: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Playlist {
    #[allow(dead_code)]
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Auto,
    Track,
    Playlist,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Play these items in order. `line_key` is an i18n id with `{title}`, `{artist}`, or `{album}`.
    Play {
        items: Vec<Track>,
        line_key: &'static str,
        title: String,
        artist: String,
        album: String,
    },
    Miss {
        key: &'static str,
        query: String,
    },
    Youtube {
        query: String,
    },
}

pub fn kind_of(raw: &str) -> Kind {
    match fold(raw).as_str() {
        "playlist" | "playlists" => Kind::Playlist,
        "track" | "song" | "music" | "audio" => Kind::Track,
        _ => Kind::Auto,
    }
}

/// `Some` when the words ask for YouTube. The string is the search with those words removed.
pub fn youtube_request(query: &str) -> Option<String> {
    let folded = fold(query);
    if !folded.contains("youtube") && !folded.contains("you tube") {
        return None;
    }
    let stripped = folded
        .replace("on youtube", " ")
        .replace("you tube", " ")
        .replace("youtube", " ");
    Some(interpret(&stripped, Kind::Auto).query)
}

pub fn plan(library: &Library, query: &str, kind: Kind, pick: usize) -> Plan {
    if let Some(cleaned) = youtube_request(query) {
        return Plan::Youtube { query: cleaned };
    }
    let intent = interpret(query, kind);
    let tracks = pool(library);
    let lists = library.playlists.iter().filter(|list| list.tracks.iter().any(playable)).collect::<Vec<_>>();

    if intent.query.is_empty() {
        if intent.playlist {
            return random_playlist(&lists, pick);
        }
        return random_track(&tracks, pick);
    }

    if intent.playlist {
        return match best_playlist(&lists, &intent.query) {
            Some(list) if list.score >= THRESHOLD => play_playlist(list.list),
            _ => Plan::Miss { key: "music-playlist-nomatch", query: intent.query },
        };
    }

    let ranked = rank_tracks(&tracks, &intent.query, intent.prefer);
    let list_hit = if matches!(kind, Kind::Track) {
        None
    } else {
        best_playlist(&lists, &intent.query)
    };
    if let Some(list) = list_hit {
        let track_score = ranked.first().map(|hit| hit.field_score).unwrap_or(0);
        let strong_track = ranked.first().is_some_and(|hit| hit.field_score >= STRONG);
        if list.score >= THRESHOLD && list.score > track_score && !strong_track {
            return play_playlist(list.list);
        }
    }
    match ranked.first() {
        Some(hit) if hit.field_score >= THRESHOLD => play_tracks(&tracks, hit, &ranked, pick),
        _ => Plan::Miss { key: "music-nomatch", query: intent.query },
    }
}

const THRESHOLD: i32 = 360;
const STRONG: i32 = 780;
const QUEUE_CAP: usize = 40;

#[derive(Clone, Copy)]
struct Weights {
    exact: i32,
    contains: i32,
    contained: i32,
    fuzzy1: i32,
    fuzzy2: i32,
}

const TITLE_W: Weights = Weights { exact: 1000, contains: 740, contained: 660, fuzzy1: 700, fuzzy2: 540 };
const ARTIST_W: Weights = Weights { exact: 860, contains: 580, contained: 520, fuzzy1: 540, fuzzy2: 420 };
const ALBUM_W: Weights = Weights { exact: 840, contains: 560, contained: 500, fuzzy1: 520, fuzzy2: 400 };
const FILE_W: Weights = Weights { exact: 780, contains: 500, contained: 460, fuzzy1: 480, fuzzy2: 360 };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Title,
    Artist,
    Album,
    File,
}

struct Intent {
    query: String,
    playlist: bool,
    prefer: Option<Field>,
}

struct Hit {
    index: usize,
    total: i32,
    field: Field,
    field_score: i32,
}

struct ListHit<'a> {
    list: &'a Playlist,
    score: i32,
}

const GENERIC: &[&str] = &[
    "play", "put", "on", "some", "song", "songs", "track", "tracks", "music", "audio", "my", "me", "please", "local",
    "file", "files", "by", "something", "anything", "random", "tunes", "tune", "a", "an", "the", "and", "just", "any",
    "playlist", "playlists", "album", "artist", "now", "again",
];

const LEADING: &[&str] = &[
    "play", "put", "on", "some", "the", "a", "an", "my", "please", "just", "any", "me", "local", "by",
];

fn interpret(raw: &str, kind: Kind) -> Intent {
    let tokens = fold(raw).split_whitespace().map(str::to_string).collect::<Vec<_>>();
    let said_playlist = tokens.iter().any(|token| token == "playlist" || token == "playlists");
    let mut playlist = matches!(kind, Kind::Playlist) || said_playlist;
    let mut prefer = None;
    if tokens.is_empty() || tokens.iter().all(|token| GENERIC.contains(&token.as_str())) {
        return Intent { query: String::new(), playlist, prefer };
    }
    let mut rest = tokens;
    while rest.first().is_some_and(|token| {
        LEADING.contains(&token.as_str())
            || matches!(token.as_str(), "playlist" | "playlists" | "album" | "artist" | "song" | "track" | "songs" | "tracks")
    }) {
        let word = rest.remove(0);
        match word.as_str() {
            "album" => prefer = Some(Field::Album),
            "artist" => prefer = Some(Field::Artist),
            "song" | "track" | "songs" | "tracks" => prefer = Some(Field::Title),
            "playlist" | "playlists" => playlist = true,
            _ => {}
        }
    }
    while rest.last().is_some_and(|token| {
        token == "playlist" || token == "playlists" || LEADING.contains(&token.as_str())
    }) {
        let word = rest.pop().unwrap_or_default();
        if word == "playlist" || word == "playlists" {
            playlist = true;
        }
    }
    if matches!(kind, Kind::Track) && !said_playlist {
        playlist = false;
    }
    Intent { query: rest.join(" "), playlist, prefer }
}

fn pool(library: &Library) -> Vec<Track> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let listed = library.tracks.iter().chain(library.playlists.iter().flat_map(|list| list.tracks.iter()));
    for track in listed {
        if !playable(track) || !seen.insert(track.uri.clone()) {
            continue;
        }
        out.push(track.clone());
    }
    out
}

fn playable(track: &Track) -> bool {
    let uri = track.uri.trim();
    uri.starts_with("content://") && !uri.starts_with("content://http")
}

fn random_track(tracks: &[Track], pick: usize) -> Plan {
    if tracks.is_empty() {
        return Plan::Miss { key: "music-empty", query: String::new() };
    }
    let track = tracks[pick % tracks.len()].clone();
    let artist = display_meta(&track.artist);
    let title = display_title(&track);
    let line_key = if artist.is_empty() { "music-playing" } else { "music-playing-by" };
    Plan::Play { items: vec![track], line_key, title, artist, album: String::new() }
}

fn random_playlist(lists: &[&Playlist], pick: usize) -> Plan {
    if lists.is_empty() {
        return Plan::Miss { key: "music-no-playlist", query: String::new() };
    }
    play_playlist(lists[pick % lists.len()])
}

fn play_playlist(list: &Playlist) -> Plan {
    let mut items: Vec<Track> = list.tracks.iter().filter(|track| playable(track)).cloned().collect();
    items.truncate(QUEUE_CAP);
    if items.is_empty() {
        return Plan::Miss { key: "music-no-playlist", query: list.name.clone() };
    }
    let title = list.name.trim();
    let title = if title.is_empty() { "playlist" } else { title };
    Plan::Play {
        items,
        line_key: "music-playing-playlist",
        title: title.to_string(),
        artist: String::new(),
        album: String::new(),
    }
}

fn play_tracks(tracks: &[Track], best: &Hit, ranked: &[Hit], pick: usize) -> Plan {
    let winner = &tracks[best.index];
    if best.field == Field::Artist && best.field_score >= STRONG {
        let mut group: Vec<Track> = tracks
            .iter()
            .filter(|track| same_meta(&track.artist, &winner.artist))
            .cloned()
            .collect();
        if group.len() >= 2 {
            shuffle(&mut group, pick);
            group.truncate(QUEUE_CAP);
            return Plan::Play {
                items: group,
                line_key: "music-playing-artist",
                title: String::new(),
                artist: display_meta(&winner.artist),
                album: String::new(),
            };
        }
    }
    if best.field == Field::Album && best.field_score >= STRONG {
        let mut group: Vec<Track> = tracks
            .iter()
            .filter(|track| same_meta(&track.album, &winner.album))
            .cloned()
            .collect();
        if group.len() >= 2 {
            group.sort_by(|a, b| a.track_no.cmp(&b.track_no).then_with(|| a.title.cmp(&b.title)));
            group.truncate(QUEUE_CAP);
            return Plan::Play {
                items: group,
                line_key: "music-playing-album",
                title: String::new(),
                artist: String::new(),
                album: display_meta(&winner.album),
            };
        }
    }
    let title = display_title(winner);
    let artist = display_meta(&winner.artist);
    let line_key = if artist.is_empty() || !matches!(best.field, Field::Title | Field::File) {
        "music-playing"
    } else {
        "music-playing-by"
    };
    let _ = ranked;
    Plan::Play { items: vec![winner.clone()], line_key, title, artist, album: String::new() }
}

fn rank_tracks(tracks: &[Track], query: &str, prefer: Option<Field>) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        if let Some(hit) = score_track(query, track, prefer, index) {
            hits.push(hit);
        }
    }
    hits.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.index.cmp(&b.index)));
    hits
}

fn score_track(query: &str, track: &Track, prefer: Option<Field>, index: usize) -> Option<Hit> {
    let q = fold(query);
    if q.is_empty() {
        return None;
    }
    let q_tokens: Vec<String> = q.split_whitespace().map(str::to_string).collect();
    let fields = [
        (Field::Title, fold(&track.title), TITLE_W),
        (Field::Artist, fold_meta(&track.artist).unwrap_or_default(), ARTIST_W),
        (Field::Album, fold_meta(&track.album).unwrap_or_default(), ALBUM_W),
        (Field::File, file_stem(&track.file), FILE_W),
    ];
    let mut best_field = Field::Title;
    let mut best_field_score = 0;
    let mut parts = [0i32; 4];
    for (i, (field, text, weights)) in fields.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let mut score = field_score(&q, &q_tokens, text, *weights);
        if prefer == Some(*field) {
            score += 90;
        }
        parts[i] = token_support(&q_tokens, text, 80);
        if score > best_field_score {
            best_field_score = score;
            best_field = *field;
        }
    }
    if best_field_score < THRESHOLD {
        return None;
    }
    let extras: i32 = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| fields[*i].0 != best_field)
        .map(|(_, score)| *score)
        .sum();
    Some(Hit { index, total: best_field_score + extras.min(160), field: best_field, field_score: best_field_score })
}

fn best_playlist<'a>(lists: &[&'a Playlist], query: &str) -> Option<ListHit<'a>> {
    let q = fold(query);
    let tokens: Vec<String> = q.split_whitespace().map(str::to_string).collect();
    lists
        .iter()
        .filter_map(|list| {
            let score = field_score(&q, &tokens, &fold(&list.name), TITLE_W);
            (score >= THRESHOLD).then_some(ListHit { list: *list, score })
        })
        .max_by_key(|hit| hit.score)
}

fn field_score(query: &str, q_tokens: &[String], text: &str, w: Weights) -> i32 {
    if text.is_empty() || query.is_empty() {
        return 0;
    }
    let mut best = 0;
    if text == query {
        best = best.max(w.exact);
    }
    if query.chars().count() >= 3 && text.contains(query) && text != query {
        let diff = text.chars().count().saturating_sub(query.chars().count()).min(20) as i32;
        best = best.max(w.contains + (20 - diff));
    }
    if text.chars().count() >= 4 && query.contains(text) && text != query {
        best = best.max(w.contained);
    }
    let min_len = query.chars().count().min(text.chars().count());
    match edit_distance(query, text) {
        1 if min_len >= 5 => best = best.max(w.fuzzy1),
        2 if min_len >= 8 => best = best.max(w.fuzzy2),
        _ => {}
    }
    let field_tokens: Vec<&str> = text.split_whitespace().collect();
    if q_tokens.len() == 1 {
        let token = &q_tokens[0];
        if token.len() >= 5
            && !field_tokens.contains(&token.as_str())
            && field_tokens.iter().any(|field| {
                field.chars().count().abs_diff(token.chars().count()) <= 2
                    && edit_distance(token, field) == 1
                    && field.chars().count() >= 5
            })
        {
            best = best.max(w.fuzzy1);
        }
    }
    if q_tokens.len() >= 2 {
        let hits = q_tokens
            .iter()
            .filter(|token| token.len() >= 2 && field_tokens.contains(&token.as_str()))
            .count();
        if hits == q_tokens.len() {
            best = best.max(w.contains - 40);
        }
    }
    best
}

fn token_support(q_tokens: &[String], text: &str, weight: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let field_tokens: Vec<&str> = text.split_whitespace().collect();
    let mut score = 0;
    for token in q_tokens {
        if token.len() < 2 {
            continue;
        }
        if field_tokens.contains(&token.as_str()) {
            score += weight;
        } else if token.len() >= 5
            && field_tokens.iter().any(|field| {
                field.chars().count().abs_diff(token.chars().count()) <= 2 && edit_distance(token, field) == 1
            })
        {
            score += weight / 2;
        }
    }
    score.min(weight * 2)
}

fn display_title(track: &Track) -> String {
    let title = track.title.trim();
    if !title.is_empty() && fold_meta(title).is_some() {
        return title.to_string();
    }
    let stem = file_stem(&track.file);
    if !stem.is_empty() {
        return stem;
    }
    "music".to_string()
}

fn display_meta(raw: &str) -> String {
    if fold_meta(raw).is_none() { String::new() } else { raw.trim().to_string() }
}

fn same_meta(a: &str, b: &str) -> bool {
    match (fold_meta(a), fold_meta(b)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn fold_meta(raw: &str) -> Option<String> {
    let folded = fold(raw);
    if folded.is_empty() || matches!(folded.as_str(), "unknown" | "unknown artist" | "unknown album" | "artist" | "album" | "null") {
        None
    } else {
        Some(strip_article(&folded))
    }
}

fn file_stem(file: &str) -> String {
    let folded = fold(file);
    let mut tokens: Vec<&str> = folded.split_whitespace().collect();
    if tokens.last().is_some_and(|token| {
        matches!(*token, "mp3" | "flac" | "wav" | "m4a" | "aac" | "ogg" | "opus" | "wma" | "aiff" | "alac")
    }) {
        tokens.pop();
    }
    strip_article(&tokens.join(" "))
}

fn strip_article(folded: &str) -> String {
    for article in ["the ", "a ", "an ", "el ", "la ", "los ", "las "] {
        if let Some(rest) = folded.strip_prefix(article) {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    folded.to_string()
}

fn fold(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        let lower: String = ch.to_lowercase().collect();
        for ch in lower.chars() {
            let mapped = fold_char(ch);
            if mapped == '\0' {
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
                continue;
            }
            out.push(mapped);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fold_char(ch: char) -> char {
    match ch {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'ß' => 's',
        other if other.is_ascii_alphanumeric() => other,
        _ => '\0',
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len() > 64 || b.len() > 64 {
        return usize::MAX;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut next = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        next[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            next[j + 1] = (prev[j + 1] + 1).min(next[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut next);
    }
    prev[b.len()]
}

fn shuffle(items: &mut [Track], pick: usize) {
    if items.len() < 2 {
        return;
    }
    let mut state = (pick as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for i in (1..items.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state as usize) % (i + 1);
        items.swap(i, j);
    }
}

pub const PLAYER_LOCAL: &str = "local";
pub const PLAYER_MUSIFY: &str = "musify";
pub const PLAYER_YOUTUBE: &str = "youtube";
pub const PLAYER_YOUTUBE_MUSIC: &str = "youtube_music";
pub const PLAYER_SYSTEM: &str = "system";

/// Stored id. Unknown or empty values stay on files on this phone.
pub fn stored_player(raw: &str) -> &'static str {
    match fold(raw).as_str() {
        "musify" => PLAYER_MUSIFY,
        "youtube music" | "yt music" | "you tube music" => PLAYER_YOUTUBE_MUSIC,
        "youtube" | "yt" | "you tube" => PLAYER_YOUTUBE,
        "system" | "ask" | "default" | "phone default" => PLAYER_SYSTEM,
        "local" | "files" | "phone" | "local files" | "on this phone" | "this phone" => PLAYER_LOCAL,
        _ => PLAYER_LOCAL,
    }
}

/// A voice change. Unknown names are an error so Talk does not silently switch player.
pub fn parse_player(raw: &str) -> Result<&'static str, String> {
    let folded = fold(raw);
    match folded.as_str() {
        "musify" => Ok(PLAYER_MUSIFY),
        "youtube music" | "yt music" | "you tube music" => Ok(PLAYER_YOUTUBE_MUSIC),
        "youtube" | "yt" | "you tube" => Ok(PLAYER_YOUTUBE),
        "system" | "ask" | "default" | "phone default" => Ok(PLAYER_SYSTEM),
        "local" | "files" | "phone" | "local files" | "on this phone" | "this phone" | "files on this phone" => Ok(PLAYER_LOCAL),
        _ => Err(
            "Music player can be local files, Musify, YouTube, YouTube Music, or the phone default.".into(),
        ),
    }
}

pub fn player_name(id: &str) -> &'static str {
    match stored_player(id) {
        PLAYER_MUSIFY => "Musify",
        PLAYER_YOUTUBE => "YouTube",
        PLAYER_YOUTUBE_MUSIC => "YouTube Music",
        PLAYER_SYSTEM => "the phone default",
        _ => "files on this phone",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayRoute {
    /// The words named YouTube the site. Talk must confirm with `open_youtube`.
    ConfirmYoutube { query: String },
    Local { query: String },
    External { player: &'static str, query: String },
}

/// Where "play …" goes. A named YouTube site request still waits for yes.
/// "YouTube Music" and "Musify" are players, so they do not take that detour.
pub fn route_play(default_player: &str, query: &str) -> PlayRoute {
    if said_youtube_music(query) {
        return PlayRoute::External { player: PLAYER_YOUTUBE_MUSIC, query: search_text(query) };
    }
    if said_youtube_site(query) {
        let cleaned = youtube_video_url(query)
            .or_else(|| youtube_request(query))
            .unwrap_or_else(|| search_text(query));
        return PlayRoute::ConfirmYoutube { query: cleaned };
    }
    if asks_for_local(query) {
        return PlayRoute::Local { query: search_text(query) };
    }
    if said_musify(query) {
        let text = youtube_video_url(query).unwrap_or_else(|| search_text(query));
        return PlayRoute::External { player: PLAYER_MUSIFY, query: text };
    }
    if let Some(url) = youtube_video_url(query) {
        let player = stored_player(default_player);
        return if player == PLAYER_LOCAL {
            PlayRoute::ConfirmYoutube { query: url }
        } else {
            PlayRoute::External { player, query: url }
        };
    }
    let player = stored_player(default_player);
    if player == PLAYER_LOCAL {
        PlayRoute::Local { query: search_text(query) }
    } else {
        PlayRoute::External { player, query: search_text(query) }
    }
}

/// Song words, or a normalized YouTube watch URL when the request contains a video id.
pub fn search_text(query: &str) -> String {
    if let Some(url) = youtube_video_url(query) {
        return url;
    }
    interpret(&strip_player_words(query), Kind::Auto).query
}

/// `https://www.youtube.com/watch?v=` plus an 11-character id, when one is present.
pub fn youtube_video_url(raw: &str) -> Option<String> {
    let id = youtube_video_id(raw)?;
    Some(format!("https://www.youtube.com/watch?v={id}"))
}

fn youtube_video_id(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    for marker in ["youtu.be/", "watch?v=", "watch?v%3d", "/shorts/", "/embed/", "/live/"] {
        if let Some(pos) = lower.find(marker) {
            let start = pos + marker.len();
            if let Some(id) = take_video_id(&raw[start..]) {
                return Some(id);
            }
        }
    }
    None
}

fn take_video_id(rest: &str) -> Option<String> {
    let id: String = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-').collect();
    if id.len() == 11 { Some(id) } else { None }
}

fn without_urls(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|token| {
            let lower = token.to_ascii_lowercase();
            !lower.starts_with("http://") && !lower.starts_with("https://")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn said_youtube_music(query: &str) -> bool {
    let folded = fold(&without_urls(query));
    folded.contains("youtube music") || folded.contains("you tube music") || folded.contains("yt music")
}

fn said_youtube_site(query: &str) -> bool {
    if said_youtube_music(query) {
        return false;
    }
    let folded = fold(&without_urls(query));
    folded.split_whitespace().any(|token| token == "youtube" || token == "yt") || folded.contains("you tube")
}

fn said_musify(query: &str) -> bool {
    fold(&without_urls(query)).split_whitespace().any(|token| token == "musify")
}

fn asks_for_local(query: &str) -> bool {
    let folded = fold(&without_urls(query));
    if folded.split_whitespace().any(|token| matches!(token, "local" | "offline" | "downloaded")) {
        return true;
    }
    folded.contains("this phone") || folded.contains("my phone") || folded.contains("my files") || folded.contains("on the phone")
}

fn strip_player_words(query: &str) -> String {
    let folded = fold(&without_urls(query));
    let tokens: Vec<&str> = folded.split_whitespace().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] == "on" && player_word_at(&tokens, index + 1) {
            index += 1;
            continue;
        }
        if tokens[index] == "you" && tokens.get(index + 1) == Some(&"tube") {
            index += 2;
            if tokens.get(index) == Some(&"music") {
                index += 1;
            }
            continue;
        }
        if tokens[index] == "youtube" && tokens.get(index + 1) == Some(&"music") {
            index += 2;
            continue;
        }
        if matches!(tokens[index], "musify" | "youtube" | "yt") {
            index += 1;
            continue;
        }
        out.push(tokens[index]);
        index += 1;
    }
    out.join(" ")
}

fn player_word_at(tokens: &[&str], index: usize) -> bool {
    matches!(tokens.get(index), Some(&"musify" | &"youtube" | &"yt"))
        || (tokens.get(index) == Some(&"you") && tokens.get(index + 1) == Some(&"tube"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, title: &str, artist: &str, album: &str, file: &str, track_no: u32) -> Track {
        Track {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            file: file.into(),
            uri: format!("content://media/external/audio/media/{id}"),
            track_no,
        }
    }

    fn sample() -> Library {
        let come = track("1", "Come Together", "The Beatles", "Abbey Road", "01 Come Together.m4a", 1);
        let something = track("2", "Something", "The Beatles", "Abbey Road", "02 Something.m4a", 2);
        let yesterday = track("3", "Yesterday", "The Beatles", "Help!", "Yesterday.mp3", 1);
        let bohemian = track("4", "Bohemian Rhapsody", "Queen", "A Night at the Opera", "Bohemian_Rhapsody.mp3", 11);
        let cafe = track("5", "Café del Mar", "Energy 52", "Café del Mar", "cafe.mp3", 1);
        let noise = track("6", "Notification Test", "", "", "noise.mp3", 0);
        let road_a = track("7", "Night Drive", "Local", "Nights", "night.mp3", 1);
        let road_b = track("8", "Dawn", "Local", "Nights", "dawn.flac", 2);
        Library {
            tracks: vec![come.clone(), something, yesterday, bohemian, cafe, noise],
            playlists: vec![Playlist {
                id: "p1".into(),
                name: "Road Trip".into(),
                tracks: vec![road_a, road_b],
            }],
        }
    }

    fn titles(plan: &Plan) -> Vec<String> {
        match plan {
            Plan::Play { items, .. } => items.iter().map(|track| track.title.clone()).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn random_music_uses_a_local_track_and_empty_library_says_so() {
        let library = sample();
        let chosen = plan(&library, "", Kind::Auto, 0);
        match chosen {
            Plan::Play { items, line_key, .. } => {
                assert_eq!(items.len(), 1);
                assert!(items[0].uri.starts_with("content://media/"));
                assert!(line_key.starts_with("music-playing"));
            }
            other => panic!("expected a track, got {other:?}"),
        }
        let words = plan(&library, "play some music", Kind::Auto, 1);
        assert!(matches!(words, Plan::Play { .. }));
        assert!(matches!(plan(&Library::default(), "music", Kind::Auto, 0), Plan::Miss { key: "music-empty", .. }));
    }

    #[test]
    fn named_song_artist_album_and_typo_pick_the_closest_local_file() {
        let library = sample();
        let song = plan(&library, "Bohemian Rhapsody", Kind::Auto, 0);
        assert_eq!(titles(&song), vec!["Bohemian Rhapsody".to_string()]);
        let typo = plan(&library, "bohemian rapsody", Kind::Auto, 0);
        assert_eq!(titles(&typo), vec!["Bohemian Rhapsody".to_string()]);
        let one_word = plan(&library, "rapsody", Kind::Auto, 0);
        assert_eq!(titles(&one_word), vec!["Bohemian Rhapsody".to_string()]);
        let artist = plan(&library, "play some Beatles", Kind::Auto, 0);
        match artist {
            Plan::Play { line_key, items, artist, .. } => {
                assert_eq!(line_key, "music-playing-artist");
                assert_eq!(artist, "The Beatles");
                assert!(items.iter().all(|track| track.artist == "The Beatles"));
                assert!(items.len() >= 3);
                assert!(!items.iter().any(|track| track.artist == "Queen"));
            }
            other => panic!("expected the beatles, got {other:?}"),
        }
        let album = plan(&library, "Abbey Road", Kind::Auto, 0);
        match album {
            Plan::Play { line_key, items, album, .. } => {
                assert_eq!(line_key, "music-playing-album");
                assert_eq!(album, "Abbey Road");
                assert_eq!(items[0].title, "Come Together");
                assert_eq!(items[1].title, "Something");
            }
            other => panic!("expected the album, got {other:?}"),
        }
        let yesterday = plan(&library, "Yesterday", Kind::Auto, 0);
        assert_eq!(titles(&yesterday), vec!["Yesterday".to_string()]);
        let accent = plan(&library, "cafe del mar", Kind::Auto, 0);
        assert_eq!(titles(&accent), vec!["Café del Mar".to_string()]);
    }

    #[test]
    fn filename_matches_and_a_miss_does_not_play_something_else() {
        let mut library = sample();
        library.tracks.push(track("9", "", "", "", "Hidden-Bside.mp3", 0));
        match plan(&library, "hidden bside", Kind::Auto, 0) {
            Plan::Play { title, items, .. } => {
                assert_eq!(title, "hidden bside");
                assert_eq!(items[0].file, "Hidden-Bside.mp3");
            }
            other => panic!("expected the file name, got {other:?}"),
        }
        assert!(matches!(
            plan(&library, "not a real song zzz", Kind::Auto, 0),
            Plan::Miss { key: "music-nomatch", .. }
        ));
        assert!(matches!(
            plan(&sample(), "Queen", Kind::Auto, 0),
            Plan::Play { artist, line_key, .. } if artist == "Queen" || line_key == "music-playing-by"
        ));
    }

    #[test]
    fn playlists_are_used_only_when_they_fit() {
        let library = sample();
        match plan(&library, "play a playlist", Kind::Auto, 0) {
            Plan::Play { line_key, title, items, .. } => {
                assert_eq!(line_key, "music-playing-playlist");
                assert_eq!(title, "Road Trip");
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected a playlist, got {other:?}"),
        }
        match plan(&library, "Road Trip", Kind::Auto, 0) {
            Plan::Play { line_key, title, .. } => {
                assert_eq!(line_key, "music-playing-playlist");
                assert_eq!(title, "Road Trip");
            }
            other => panic!("expected the named playlist, got {other:?}"),
        }
        let no_lists = Library { tracks: library.tracks.clone(), playlists: Vec::new() };
        assert!(matches!(
            plan(&no_lists, "playlist", Kind::Playlist, 0),
            Plan::Miss { key: "music-no-playlist", .. }
        ));
        assert!(matches!(
            plan(&library, "missing mix", Kind::Playlist, 0),
            Plan::Miss { key: "music-playlist-nomatch", .. }
        ));
        let artist = plan(&library, "Beatles", Kind::Auto, 0);
        match artist {
            Plan::Play { line_key, .. } => assert_eq!(line_key, "music-playing-artist"),
            other => panic!("a strong artist match stays on files, got {other:?}"),
        }
    }

    #[test]
    fn youtube_is_not_a_local_play() {
        assert_eq!(youtube_request("play Bohemian Rhapsody on YouTube").as_deref(), Some("bohemian rhapsody"));
        assert_eq!(youtube_request("look for a video on youtube").as_deref(), Some("look for a video"));
        assert!(youtube_request("play Bohemian Rhapsody").is_none());
        assert!(matches!(
            plan(&sample(), "play Yesterday on YouTube", Kind::Auto, 0),
            Plan::Youtube { query } if query == "yesterday"
        ));
    }

    #[test]
    fn play_routes_follow_the_default_player_and_keep_youtube_confirmation() {
        assert!(matches!(route_play("local", "play some music"), PlayRoute::Local { query } if query.is_empty()));
        assert!(matches!(
            route_play("musify", "play some music"),
            PlayRoute::External { player: "musify", query } if query.is_empty()
        ));
        assert!(matches!(
            route_play("musify", "play Bohemian Rhapsody"),
            PlayRoute::External { player: "musify", query } if query == "bohemian rhapsody"
        ));
        assert!(matches!(
            route_play("local", "play Bohemian Rhapsody on musify"),
            PlayRoute::External { player: "musify", query } if query == "bohemian rhapsody"
        ));
        assert!(matches!(
            route_play("musify", "play local files"),
            PlayRoute::Local { .. }
        ));
        assert!(matches!(
            route_play("musify", "play Bohemian Rhapsody on YouTube"),
            PlayRoute::ConfirmYoutube { query } if query == "bohemian rhapsody"
        ));
        assert!(matches!(
            route_play("youtube", "play jazz"),
            PlayRoute::External { player: "youtube", query } if query == "jazz"
        ));
        assert!(matches!(
            route_play("local", "play See You Again on YouTube Music"),
            PlayRoute::External { player: "youtube_music", query } if query == "see you again"
        ));
        let url = "https://youtu.be/dQw4w9WgXcQ";
        assert_eq!(
            youtube_video_url(url).as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert!(matches!(
            route_play("musify", url),
            PlayRoute::External { player: "musify", query } if query == "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(matches!(route_play("local", url), PlayRoute::ConfirmYoutube { .. }));
        assert_eq!(stored_player("YouTube Music"), "youtube_music");
        assert_eq!(stored_player("nope"), "local");
        assert!(parse_player("sideways").is_err());
    }

    #[test]
    fn spoken_lines_exist_in_every_locale_and_the_player_stays_off_the_alarm_stream() {
        for key in [
            "music-playing",
            "music-playing-by",
            "music-playing-album",
            "music-playing-artist",
            "music-playing-playlist",
            "music-empty",
            "music-nomatch",
            "music-no-playlist",
            "music-playlist-nomatch",
            "music-permission",
            "music-permission-settings",
            "music-phone-only",
            "music-failed",
            "music-youtube",
            "music-musify-playing",
            "music-musify-playing-by",
            "music-musify-started",
            "music-musify-resume",
            "music-ytmusic",
            "music-ytmusic-resume",
            "music-youtube-app",
            "music-youtube-video",
            "music-youtube-home",
            "music-youtube-browser",
            "music-browser-video",
            "music-browser-home",
            "music-system",
            "music-system-resume",
            "music-system-ask",
            "music-none",
            "music-external-phone",
            "music-fallback-musify",
            "music-fallback-youtube_music",
            "music-fallback-youtube",
            "music-fallback-system",
            "media-play",
            "media-pause",
            "media-stop",
            "media-next",
            "media-previous",
            "media-shuffle",
            "media-unshuffle",
            "media-shuffle-none",
            "media-sent",
            "media-sent-listener",
            "media-bad",
        ] {
            let english = crate::i18n::line("en", key);
            assert!(english.contains(' ') || english.contains('{'), "{key}");
            for locale in ["es", "de", "pt", "ca-ES-valencia"] {
                let translated = crate::i18n::line(locale, key);
                assert_ne!(translated, english, "{locale} {key}");
                assert_ne!(translated, key, "{locale} {key}");
            }
        }
        let music = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/LocalMusic.kt");
        let spoken = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/SpokenAlerts.kt");
        assert!(music.contains("USAGE_MEDIA"));
        assert!(!music.contains("USAGE_ALARM"));
        assert!(spoken.contains("USAGE_ALARM"));
        assert!(!spoken.contains("LocalMusic"));
        for xml in [
            include_str!("../gen/android/app/src/main/res/values/strings.xml"),
            include_str!("../gen/android/app/src/main/res/values-es/strings.xml"),
            include_str!("../gen/android/app/src/main/res/values-de/strings.xml"),
            include_str!("../gen/android/app/src/main/res/values-pt/strings.xml"),
            include_str!("../gen/android/app/src/main/res/values-b+ca+ES+valencia/strings.xml"),
        ] {
            assert!(xml.contains("music_channel_name"));
            assert!(xml.contains("music_pause"));
            assert!(xml.contains("music_stop"));
            assert!(xml.contains("music_pick_player"));
        }
    }
}
