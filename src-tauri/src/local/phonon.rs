//! Phonon checkpoints from Hugging Face. Weights stay out of the APK.
//! Gated models are downloaded only with the token the user saved. A refusal
//! stays a refusal: nothing is fetched from another host.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::packs::{self, PackProgress};

pub const GATED_REFUSAL: &str = "Hugging Face refused this model. Accept the license on its page and check the token. Nothing was downloaded another way.";
const TOKEN_NEEDED: &str = "This model needs a Hugging Face token. Accept its license on the model page, then save the token in Settings. English Phonon 31m does not need a token.";
const CHECKSUM: &str = "The download did not match the expected file.";
const VOICES_REPO: &str = "gradium/phonon-voices";
const VOICES_REV: &str = "0ee17205f254b5abc43b266c8a0f215565c352f7";

pub struct AssetFile {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub license: &'static str,
    pub gated: bool,
    pub multilingual: bool,
    pub langs: &'static [&'static str],
    pub files: &'static [AssetFile],
}

const PUBLIC_TOKENIZER: &str = "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936";
const PUBLIC_CONFIG: &str = "b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85";
const PUBLIC_VOICE: &str = "dc537478eaadc1a5257fcda2e3501ab81e14c111a5d95a5838f350db2afd0f1a";
const PUBLIC_Q8: &str = "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae";

const EN_31: &[AssetFile] = &[
    AssetFile {
        name: "config.json",
        bytes: 4_634,
        sha256: PUBLIC_CONFIG,
    },
    AssetFile {
        name: "tokenizer.json",
        bytes: 277_967,
        sha256: PUBLIC_TOKENIZER,
    },
    AssetFile {
        name: "default-voice.safetensors",
        bytes: 256_136,
        sha256: PUBLIC_VOICE,
    },
    AssetFile {
        name: "model.q8.gguf",
        bytes: 64_512_352,
        sha256: PUBLIC_Q8,
    },
];

const fn file(name: &'static str, bytes: u64) -> AssetFile {
    AssetFile {
        name,
        bytes,
        sha256: "",
    }
}

const EN_40: &[AssetFile] = &[
    file("config.json", 4_634),
    AssetFile {
        name: "tokenizer.json",
        bytes: 277_967,
        sha256: PUBLIC_TOKENIZER,
    },
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];
const ES_40: &[AssetFile] = &[
    file("config.json", 4_634),
    file("tokenizer.json", 278_837),
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];
const DE_40: &[AssetFile] = &[
    file("config.json", 4_634),
    file("tokenizer.json", 278_111),
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];
const PT_40: &[AssetFile] = &[
    file("config.json", 4_634),
    file("tokenizer.json", 279_034),
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];
const FR_40: &[AssetFile] = &[
    file("config.json", 4_634),
    file("tokenizer.json", 277_749),
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];
const MULTI_40: &[AssetFile] = &[
    file("config.json", 4_674),
    file("tokenizer.json", 277_749),
    file("default-voice.safetensors", 256_136),
    file("model.q8.gguf", 74_553_568),
];

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "phonon-31m-en",
        repo: "gradium/phonon-31m-en",
        revision: "8f7a37c94a4c58b04c8287300c53f11d2e208eae",
        label: "Phonon 31m English",
        detail: "Apache 2.0. Public model. No Hugging Face token.",
        license: "Apache-2.0",
        gated: false,
        multilingual: false,
        langs: &["en"],
        files: EN_31,
    },
    ModelSpec {
        id: "phonon-40m-en",
        repo: "gradium/phonon-40m-en",
        revision: "bc7e8ae7e543a7a3e4604753e42fbda76ddf0781",
        label: "Phonon 40m English",
        detail: "CC BY-NC 4.0. Non-commercial. A Hugging Face token is required.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: false,
        langs: &["en"],
        files: EN_40,
    },
    ModelSpec {
        id: "phonon-40m-es",
        repo: "gradium/phonon-40m-es",
        revision: "e5e052b26e31abd5918ade71d3135e69d51a66f8",
        label: "Phonon 40m Spanish",
        detail: "CC BY-NC 4.0. Non-commercial. A Hugging Face token is required.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: false,
        langs: &["es"],
        files: ES_40,
    },
    ModelSpec {
        id: "phonon-40m-de",
        repo: "gradium/phonon-40m-de",
        revision: "79cfc7b47fcfc0b666657b981eb366415023a862",
        label: "Phonon 40m German",
        detail: "CC BY-NC 4.0. Non-commercial. A Hugging Face token is required.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: false,
        langs: &["de"],
        files: DE_40,
    },
    ModelSpec {
        id: "phonon-40m-pt",
        repo: "gradium/phonon-40m-pt",
        revision: "4cbfd742a1555d60eb5399c6d296e9f2e07389e9",
        label: "Phonon 40m Portuguese",
        detail: "CC BY-NC 4.0. Non-commercial. A Hugging Face token is required.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: false,
        langs: &["pt"],
        files: PT_40,
    },
    ModelSpec {
        id: "phonon-40m-fr",
        repo: "gradium/phonon-40m-fr",
        revision: "d91086970206b88efd2c88ca971d0628b5968f0f",
        label: "Phonon 40m French",
        detail: "CC BY-NC 4.0. Non-commercial. A Hugging Face token is required. This app has no French screen.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: false,
        langs: &["fr"],
        files: FR_40,
    },
    ModelSpec {
        id: "phonon-40m-multilingual",
        repo: "gradium/phonon-40m-multilingual",
        revision: "1537ce46c64f42903e0d608b132ccb073dac46d2",
        label: "Phonon 40m multilingual",
        detail: "CC BY-NC 4.0. Non-commercial. One model for English, Spanish, German, Portuguese, and French. A Hugging Face token is required.",
        license: "CC-BY-NC-4.0",
        gated: true,
        multilingual: true,
        langs: &["en", "es", "de", "pt", "fr"],
        files: MULTI_40,
    },
];

impl ModelSpec {
    pub fn speaks(&self, lang: &str) -> bool {
        self.langs.contains(&lang)
    }

    pub fn download_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

pub fn model_by_id(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|model| model.id == id)
}

pub fn file_url(model: &ModelSpec, file: &AssetFile) -> String {
    format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        model.repo, model.revision, file.name
    )
}

pub fn gate_error(model: &ModelSpec, token: &str) -> Option<&'static str> {
    if model.gated && token.trim().is_empty() {
        Some(TOKEN_NEEDED)
    } else {
        None
    }
}

/// Public models never send the token, so a bad token cannot block English 31m.
pub fn auth_header(model: &ModelSpec, token: &str) -> Option<String> {
    if !model.gated {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(format!("Bearer {token}"))
    }
}

pub fn refused_status(status: u16) -> Option<&'static str> {
    if status == 401 || status == 403 {
        Some(GATED_REFUSAL)
    } else {
        None
    }
}

pub fn valid_voice_id(id: &str) -> bool {
    let len = id.len();
    (1..=64).contains(&len)
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

pub fn model_dir(root: &Path, id: &str) -> PathBuf {
    root.join("phonon").join(id)
}

pub fn voice_path(root: &Path, id: &str) -> PathBuf {
    root.join("phonon-voices").join(format!("{id}.safetensors"))
}

pub fn installed_ids(root: &Path) -> Vec<String> {
    MODELS
        .iter()
        .filter(|model| model_ready(root, model))
        .map(|model| model.id.to_string())
        .collect()
}

pub fn installed_voice_ids(root: &Path) -> Vec<String> {
    let dir = root.join("phonon-voices");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(id) = name.strip_suffix(".safetensors") else {
            continue;
        };
        if valid_voice_id(id) && file_len(&entry.path()) > 1_000 {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    ids
}

pub fn installed_bytes(root: &Path, id: &str) -> u64 {
    dir_bytes(&model_dir(root, id))
}

pub fn model_ready(root: &Path, model: &ModelSpec) -> bool {
    model
        .files
        .iter()
        .all(|file| asset_ready(&model_dir(root, model.id).join(file.name), file))
}

fn asset_ready(path: &Path, file: &AssetFile) -> bool {
    if file_len(path) != file.bytes {
        return false;
    }
    if file.name.ends_with(".gguf") {
        return starts_with(path, b"GGUF");
    }
    if file.name.ends_with(".json") {
        return starts_with(path, b"{");
    }
    if file.name.ends_with(".safetensors") {
        return safetensors_header(path);
    }
    true
}

pub fn download_model(
    root: &Path,
    id: &str,
    token: &str,
    mut progress: impl FnMut(PackProgress),
) -> Result<(), String> {
    let model = model_by_id(id).ok_or_else(|| format!("unknown Phonon model {id}"))?;
    if let Some(reason) = gate_error(model, token) {
        return Err(reason.into());
    }
    packs::begin_download()?;
    let result = download_model_locked(root, model, token, &mut progress);
    packs::end_download();
    result
}

fn download_model_locked(
    root: &Path,
    model: &ModelSpec,
    token: &str,
    progress: &mut impl FnMut(PackProgress),
) -> Result<(), String> {
    let dir = model_dir(root, model.id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let total = model.download_bytes();
    let mut received_before = 0u64;
    for file in model.files {
        let dest = dir.join(file.name);
        if asset_ready(&dest, file) {
            received_before += file.bytes;
            progress(PackProgress {
                id: model.id.into(),
                received: received_before,
                total,
            });
            continue;
        }
        download_file(
            &dest,
            &file_url(model, file),
            file,
            model,
            token,
            received_before,
            total,
            progress,
        )?;
        received_before += file.bytes;
    }
    if !model_ready(root, model) {
        return Err(format!(
            "{} downloaded, but a file was missing.",
            model.label
        ));
    }
    Ok(())
}

pub fn delete_model(root: &Path, id: &str) -> Result<(), String> {
    if model_by_id(id).is_none() {
        return Err(format!("unknown Phonon model {id}"));
    }
    let dir = model_dir(root, id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn download_voice(
    root: &Path,
    id: &str,
    mut progress: impl FnMut(PackProgress),
) -> Result<(), String> {
    if !valid_voice_id(id) {
        return Err("That voice is not in the Phonon catalog.".into());
    }
    let url = format!(
        "https://huggingface.co/{VOICES_REPO}/resolve/{VOICES_REV}/voices/{id}.safetensors"
    );
    if !crate::http::https_url_allowed(&url, &["huggingface.co", "*.huggingface.co", "*.hf.co"]) {
        return Err("download address was not https".into());
    }
    packs::begin_download()?;
    let dest = voice_path(root, id);
    let result = (|| {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let spec = AssetFile {
            name: "voice.safetensors",
            bytes: 0,
            sha256: "",
        };
        download_file(
            &dest,
            &url,
            &spec,
            model_by_id("phonon-31m-en").expect("public model"),
            "",
            0,
            0,
            &mut progress,
        )?;
        if !safetensors_header(&dest) || file_len(&dest) < 1_000 {
            let _ = fs::remove_file(&dest);
            return Err("That voice file could not be read.".into());
        }
        Ok(())
    })();
    packs::end_download();
    result
}

pub fn delete_voice(root: &Path, id: &str) -> Result<(), String> {
    if !valid_voice_id(id) {
        return Err("That voice is not in the Phonon catalog.".into());
    }
    let path = voice_path(root, id);
    if path.exists() {
        fs::remove_file(&path).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
pub fn write_placeholder(root: &Path, id: &str) -> Result<(), String> {
    let model = model_by_id(id).ok_or_else(|| format!("unknown Phonon model {id}"))?;
    let dir = model_dir(root, id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    for file in model.files {
        let path = dir.join(file.name);
        let mut out = File::create(&path).map_err(|err| err.to_string())?;
        out.set_len(file.bytes).map_err(|err| err.to_string())?;
        if file.name.ends_with(".gguf") {
            out.write_all(b"GGUF").map_err(|err| err.to_string())?;
        } else if file.name.ends_with(".json") {
            out.write_all(b"{}\n").map_err(|err| err.to_string())?;
        } else if file.name.ends_with(".safetensors") {
            out.write_all(&128u64.to_le_bytes())
                .map_err(|err| err.to_string())?;
            out.write_all(br#"{"speaker_wavs":{}}"#)
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn download_file(
    dest: &Path,
    url: &str,
    file: &AssetFile,
    model: &ModelSpec,
    token: &str,
    received_before: u64,
    total: u64,
    progress: &mut impl FnMut(PackProgress),
) -> Result<(), String> {
    if !crate::http::https_url_allowed(url, &["huggingface.co", "*.huggingface.co", "*.hf.co"]) {
        return Err("download address was not https".into());
    }
    let partial = dest.with_extension(format!(
        "{}.partial",
        dest.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("bin")
    ));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(20))
        .redirect(crate::http::pack_redirect())
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.get(url).header("user-agent", "GrokVoice/0.1.54");
    if let Some(header) = auth_header(model, token) {
        request = request.header("authorization", header);
    }
    let mut response = request
        .send()
        .map_err(|err| format!("download failed: {err}"))?;
    let status = response.status();
    if let Some(reason) = refused_status(status.as_u16()) {
        return Err(reason.into());
    }
    if !status.is_success() {
        return Err(format!("download failed ({status})"));
    }
    if let Some(parent) = partial.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut out = File::create(&partial).map_err(|err| err.to_string())?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|err| format!("download failed: {err}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        received += n as u64;
        if total > 0 {
            progress(PackProgress {
                id: model.id.into(),
                received: received_before + received,
                total,
            });
        }
    }
    out.flush().map_err(|err| err.to_string())?;
    drop(out);
    if file.bytes > 0 && received != file.bytes {
        let _ = fs::remove_file(&partial);
        return Err(CHECKSUM.into());
    }
    if !file.sha256.is_empty() && hex_encode(&hasher.finalize()) != file.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(CHECKSUM.into());
    }
    fs::rename(&partial, dest).map_err(|err| err.to_string())?;
    if file.bytes > 0 && !asset_ready(dest, file) {
        let _ = fs::remove_file(dest);
        return Err(CHECKSUM.into());
    }
    Ok(())
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn starts_with(path: &Path, magic: &[u8]) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; magic.len()];
    file.read_exact(&mut buf).is_ok() && buf == magic
}

fn safetensors_header(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut len = [0u8; 8];
    if file.read_exact(&mut len).is_err() {
        return false;
    }
    let header = u64::from_le_bytes(len);
    if !(8..=1_000_000).contains(&header) {
        return false;
    }
    let mut brace = [0u8; 1];
    file.read_exact(&mut brace).is_ok() && brace[0] == b'{'
}

fn dir_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() {
            total += meta.len();
        }
    }
    total
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

/// Mono f32 at the model's rate. Host builds report that synthesis runs on the phone.
pub fn synthesize(
    root: &Path,
    model_id: &str,
    lang: &str,
    voice_id: &str,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model = model_by_id(model_id).ok_or_else(|| format!("unknown Phonon model {model_id}"))?;
    if !model.speaks(super::speech::speech_lang(lang)) {
        return Err("That Phonon model does not speak this language.".into());
    }
    if !model_ready(root, model) {
        return Err("Phonon is not downloaded for this language yet.".into());
    }
    let voice = if valid_voice_id(voice_id) {
        voice_id
    } else {
        ""
    };
    let voice_file = if voice.is_empty() {
        None
    } else {
        let path = voice_path(root, voice);
        path.is_file().then_some(path)
    };
    #[cfg(target_os = "android")]
    {
        android::synthesize(
            &model_dir(root, model.id),
            lang,
            voice,
            voice_file.as_deref(),
            text,
        )
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (voice, voice_file, text);
        Err("Voice preview runs in the Android app.".into())
    }
}

/// Android links the official `ptts-ffi` CPU library. It is built on its own so
/// its `half` crate does not have to match the one candle uses here.
#[cfg(target_os = "android")]
mod android {
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::path::Path;
    use std::sync::Mutex;

    const CPU: u32 = 0;
    const RATE: u32 = 24_000;

    #[repr(C)]
    struct PttsResult {
        frames: u32,
        samples: usize,
        ttfa_ms: f64,
        total_ms: f64,
    }

    unsafe extern "C" {
        fn ptts_new(dir: *const c_char, unit: u32, lang: *const c_char) -> *mut ();
        fn ptts_speak(
            handle: *mut (),
            text: *const c_char,
            cb: extern "C" fn(*const f32, usize, *mut c_void) -> bool,
            user: *mut c_void,
            out: *mut PttsResult,
        ) -> bool;
        fn ptts_set_voice(handle: *mut (), name: *const c_char) -> bool;
        fn ptts_free(handle: *mut ());
        fn ptts_last_error(handle: *const ()) -> *const c_char;
    }

    struct Held {
        key: String,
        handle: *mut (),
    }

    unsafe impl Send for Held {}

    static HELD: Mutex<Option<Held>> = Mutex::new(None);

    extern "C" fn collect(pcm: *const f32, n: usize, user: *mut c_void) -> bool {
        if pcm.is_null() || n == 0 {
            return true;
        }
        let out = unsafe { &mut *(user as *mut Vec<f32>) };
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(pcm, n) });
        true
    }

    fn last_error(handle: *const ()) -> String {
        let ptr = unsafe { ptts_last_error(handle) };
        if ptr.is_null() {
            return "could not load Phonon".into();
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .trim()
            .to_string()
    }

    pub fn synthesize(
        dir: &Path,
        lang: &str,
        voice: &str,
        voice_file: Option<&Path>,
        text: &str,
    ) -> Result<(Vec<f32>, u32), String> {
        let text = text.trim().chars().take(500).collect::<String>();
        if text.is_empty() {
            return Err("nothing to say".into());
        }
        if let Some(path) = voice_file {
            if !voice.is_empty() {
                let voices = dir.join("voices");
                std::fs::create_dir_all(&voices).map_err(|err| err.to_string())?;
                let dest = voices.join(format!("{voice}.safetensors"));
                if !dest.is_file() {
                    std::fs::copy(path, &dest).map_err(|err| err.to_string())?;
                }
            }
        }
        let key = format!("{}\n{lang}\n{voice}", dir.display());
        let mut guard = HELD.lock().unwrap_or_else(|err| err.into_inner());
        if guard.as_ref().map(|held| held.key.as_str()) != Some(key.as_str()) {
            if let Some(old) = guard.take() {
                unsafe { ptts_free(old.handle) };
            }
            if std::env::var_os("RAYON_NUM_THREADS").is_none() {
                // The Android guide asks for two threads before the first load.
                // Setting the variable is unsafe because another thread could read the environment.
                unsafe { std::env::set_var("RAYON_NUM_THREADS", "2") };
            }
            let dir_c = CString::new(dir.to_string_lossy().as_bytes())
                .map_err(|_| "model path contained a null".to_string())?;
            let lang_c = CString::new(lang).map_err(|_| "language contained a null".to_string())?;
            let handle = unsafe { ptts_new(dir_c.as_ptr(), CPU, lang_c.as_ptr()) };
            if handle.is_null() {
                return Err(last_error(std::ptr::null()));
            }
            if !voice.is_empty() {
                let name = CString::new(voice).map_err(|_| "voice contained a null".to_string())?;
                if !unsafe { ptts_set_voice(handle, name.as_ptr()) } {
                    let err = last_error(handle);
                    unsafe { ptts_free(handle) };
                    return Err(err);
                }
            }
            *guard = Some(Held { key, handle });
        }
        let handle = guard
            .as_ref()
            .map(|held| held.handle)
            .ok_or_else(|| "could not load Phonon".to_string())?;
        let spoken = CString::new(text).map_err(|_| "text contained a null".to_string())?;
        let mut pcm = Vec::new();
        let mut result = PttsResult {
            frames: 0,
            samples: 0,
            ttfa_ms: 0.0,
            total_ms: 0.0,
        };
        let ok = unsafe {
            ptts_speak(
                handle,
                spoken.as_ptr(),
                collect,
                &mut pcm as *mut Vec<f32> as *mut c_void,
                &mut result,
            )
        };
        if !ok || pcm.is_empty() {
            return Err(last_error(handle));
        }
        let _ = result;
        Ok((pcm, RATE))
    }
}
