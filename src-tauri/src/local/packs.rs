//! On-demand model packs. Weights stay out of the APK and live under app storage.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use super::gguf::{self, inspect_gguf};

pub const STT_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-int8.tar.bz2";
/// SmolLM2 135M Instruct, Q8_0. Same ChatML family as the old 360M pack, and not a
/// coding model. Q8 keeps tool JSON closer to the full weights. The 360M Q8 file
/// was 386 MB, which is too much to reread on every voice turn.
pub const LLM_URL: &str = "https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF/resolve/main/SmolLM2-135M-Instruct-Q8_0.gguf";
pub const LLM_FILE: &str = "smollm2-135m-instruct-q8_0.gguf";
/// Gemma 3 1B Instruct, Q4_K_M, from the llama.cpp ggml-org conversion.
pub const GEMMA_URL: &str =
    "https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/resolve/main/gemma-3-1b-it-Q4_K_M.gguf";
pub const GEMMA_FILE: &str = "gemma-3-1b-it-q4_k_m.gguf";
/// Qwen2.5 0.5B Instruct, Q8_0, from the official Qwen GGUF repo.
pub const QWEN_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q8_0.gguf";
pub const QWEN_FILE: &str = "qwen2.5-0.5b-instruct-q8_0.gguf";
/// Llama 3.2 1B Instruct, Q4_K_M, from bartowski's GGUF repo.
pub const LLAMA_URL: &str = "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf";
pub const LLAMA_FILE: &str = "llama-3.2-1b-instruct-q4_k_m.gguf";
/// LFM2.5 350M, Q8_0, from Liquid AI's official GGUF repo. Instruction-tuned
/// for on-device chat. The file lists English and Spanish. Q8 matches the
/// other sub-1B chat packs so tool JSON stays closer to the full weights.
pub const LFM_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2.5-350M-GGUF/resolve/main/LFM2.5-350M-Q8_0.gguf";
pub const LFM_FILE: &str = "lfm2.5-350m-q8_0.gguf";
pub const STT_DIR: &str = "sherpa-onnx-moonshine-base-en-int8";
const LEGACY_STT_DIR: &str = "sherpa-onnx-moonshine-tiny-en-int8";
const REPLACE_NOTE: &str = "Replaces the previous pack. Download it again.";
const DEFAULT_LLM: &str = "llm";
pub const CUSTOM_LLM: &str = "llm-custom";
const CUSTOM_FILE: &str = "model.gguf";
const CUSTOM_META: &str = "meta.json";
const SELECTED_FILE: &str = "selected-llm.txt";
const INCOMING_FILE: &str = "incoming.gguf";
/// A phone chat GGUF. Larger files are refused before they fill storage.
pub const CUSTOM_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[cfg_attr(not(test), allow(dead_code))]
pub fn llm_model_id() -> &'static str {
    "smollm2-135m-instruct-q8_0"
}
pub const TTS_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-low.tar.bz2";
pub const STT_DE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8.tar.bz2";
pub const TTS_DE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-de_DE-thorsten-low.tar.bz2";

struct Spec {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    url: &'static str,
    download_bytes: u64,
    sha256: &'static str,
    archive: bool,
    /// English speech. German packs and extra language models are optional.
    required: bool,
    /// `speech` or `llm`.
    kind: &'static str,
    file: &'static str,
    /// `chatml`, `gemma`, `llama3`, or `lfm2` for a language model.
    family: &'static str,
}

const PACKS: &[Spec] = &[
    Spec {
        id: "stt",
        label: "Speech to text",
        detail: "Moonshine Base English, int8",
        url: STT_URL,
        download_bytes: 250_807_309,
        sha256: "21870cecaa2e44e4e2bf63e02d1072bed183ccd10284871353bd9d24dad14e5e",
        archive: true,
        required: true,
        kind: "speech",
        file: "",
        family: "",
    },
    Spec {
        id: "llm",
        label: "SmolLM2 135M Instruct",
        detail: "Q8_0. Smallest chat model. Talk and tools.",
        url: LLM_URL,
        download_bytes: 144_811_360,
        sha256: "5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83",
        archive: false,
        required: false,
        kind: "llm",
        file: LLM_FILE,
        family: "chatml",
    },
    Spec {
        id: "tts",
        label: "Voice",
        detail: "Piper en_US lessac, low",
        url: TTS_URL,
        download_bytes: 67_097_098,
        sha256: "8fb427b8637334072ee5723d72fa418c45bfdd4b7deebeacdf2938662618c1cb",
        archive: true,
        required: true,
        kind: "speech",
        file: "",
        family: "",
    },
    Spec {
        id: "stt-de",
        label: "German speech to text",
        detail: "NeMo FastConformer German, int8",
        url: STT_DE_URL,
        download_bytes: 108_124_747,
        sha256: "73b3cce9f94a19fc04bc54b55abd576b622e3925a84ddd554a95e5984bc11677",
        archive: true,
        required: false,
        kind: "speech",
        file: "",
        family: "",
    },
    Spec {
        id: "tts-de",
        label: "German voice",
        detail: "Piper de_DE thorsten, low",
        url: TTS_DE_URL,
        download_bytes: 67_101_576,
        sha256: "41fab35910fdcec4696b031951d8fd6c262e594cf77b35e1068fadbeb5a091a6",
        archive: true,
        required: false,
        kind: "speech",
        file: "",
        family: "",
    },
    Spec {
        id: "llm-gemma3-1b",
        label: "Gemma 3 1B Instruct",
        detail: "Q4_K_M. Gemma 3 chat model for talk and tools.",
        url: GEMMA_URL,
        download_bytes: 806_058_240,
        sha256: "8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135",
        archive: false,
        required: false,
        kind: "llm",
        file: GEMMA_FILE,
        family: "gemma",
    },
    Spec {
        id: "llm-qwen25-05b",
        label: "Qwen2.5 0.5B Instruct",
        detail: "Q8_0. Small Qwen chat model for talk and tools.",
        url: QWEN_URL,
        download_bytes: 675_710_816,
        sha256: "ca59ca7f13d0e15a8cfa77bd17e65d24f6844b554a7b6c12e07a5f89ff76844e",
        archive: false,
        required: false,
        kind: "llm",
        file: QWEN_FILE,
        family: "chatml",
    },
    Spec {
        id: "llm-llama32-1b",
        label: "Llama 3.2 1B Instruct",
        detail: "Q4_K_M. Llama 3.2 chat model for talk and tools.",
        url: LLAMA_URL,
        download_bytes: 807_694_464,
        sha256: "6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83",
        archive: false,
        required: false,
        kind: "llm",
        file: LLAMA_FILE,
        family: "llama3",
    },
    Spec {
        id: "llm-lfm25-350m",
        label: "LFM2.5 350M",
        detail: "Q8_0. Compact Spanish and English chat model for talk and tools.",
        url: LFM_URL,
        download_bytes: 379_217_632,
        sha256: "be036a757295e550098b85e13f6af2735d0fa73b41e1156a40c7d8e8e32a5766",
        archive: false,
        required: false,
        kind: "llm",
        file: LFM_FILE,
        family: "lfm2",
    },
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub ready: bool,
    /// Set when an older pack is on the phone and this one still needs a download.
    pub note: String,
    /// `speech`, `llm`, or `custom`.
    pub kind: String,
    /// The language model Talk will load. Speech packs are never selected.
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct ActiveModel {
    #[cfg_attr(not(test), allow(dead_code))]
    pub id: String,
    pub name: String,
    pub family: String,
    pub path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomMeta {
    name: String,
    source: String,
    url: String,
    sha256: String,
    sha256_checked: bool,
    family: String,
    architecture: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackProgress {
    pub id: String,
    pub received: u64,
    pub total: u64,
}

#[derive(Clone)]
pub struct ModelPaths {
    /// `moonshine` or `transducer`. Read by the Android speech loader.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub asr: &'static str,
    pub preprocess: PathBuf,
    pub encoder: PathBuf,
    pub uncached: PathBuf,
    pub cached: PathBuf,
    pub joiner: PathBuf,
    pub stt_tokens: PathBuf,
    pub llm: PathBuf,
    pub tts_model: PathBuf,
    pub tts_tokens: PathBuf,
    pub tts_data: PathBuf,
}

pub fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join("local-models");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn model_paths(root: &Path) -> ModelPaths {
    model_paths_for(root, "en")
}

pub fn model_paths_for(root: &Path, lang: &str) -> ModelPaths {
    let llm = active_model(root).path;
    if lang == "de" {
        let stt = root
            .join("stt-de/sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8");
        let tts = root.join("tts-de/vits-piper-de_DE-thorsten-low");
        return ModelPaths {
            asr: "transducer",
            preprocess: stt.join("unused"),
            encoder: stt.join("encoder.int8.onnx"),
            uncached: stt.join("decoder.int8.onnx"),
            cached: stt.join("unused"),
            joiner: stt.join("joiner.int8.onnx"),
            stt_tokens: stt.join("tokens.txt"),
            llm,
            tts_model: tts.join("de_DE-thorsten-low.onnx"),
            tts_tokens: tts.join("tokens.txt"),
            tts_data: tts.join("espeak-ng-data"),
        };
    }
    let stt = root.join("stt").join(STT_DIR);
    let tts = root.join("tts/vits-piper-en_US-lessac-low");
    ModelPaths {
        asr: "moonshine",
        preprocess: stt.join("preprocess.onnx"),
        encoder: stt.join("encode.int8.onnx"),
        uncached: stt.join("uncached_decode.int8.onnx"),
        cached: stt.join("cached_decode.int8.onnx"),
        joiner: stt.join("unused"),
        stt_tokens: stt.join("tokens.txt"),
        llm,
        tts_model: tts.join("en_US-lessac-low.onnx"),
        tts_tokens: tts.join("tokens.txt"),
        tts_data: tts.join("espeak-ng-data"),
    }
}

pub fn pack_status(root: &Path) -> Vec<PackInfo> {
    let selected = selected_id(root);
    let mut packs: Vec<PackInfo> = PACKS
        .iter()
        .map(|spec| PackInfo {
            id: spec.id.into(),
            label: spec.label.into(),
            detail: spec.detail.into(),
            download_bytes: spec.download_bytes,
            installed_bytes: dir_bytes(&root.join(spec.id)),
            ready: pack_ready(root, spec.id),
            note: pack_note(root, spec.id),
            kind: spec.kind.into(),
            selected: spec.kind == "llm" && spec.id == selected,
        })
        .collect();
    packs.push(custom_info(root, &selected));
    packs
}

pub fn packs_ready(root: &Path) -> bool {
    packs_ready_for(root, "en")
}

pub fn packs_ready_for(root: &Path, lang: &str) -> bool {
    if !PACKS
        .iter()
        .filter(|spec| spec.kind == "speech" && spec.required)
        .all(|spec| pack_ready(root, spec.id))
    {
        return false;
    }
    if lang == "de" && !(pack_ready(root, "stt-de") && pack_ready(root, "tts-de")) {
        return false;
    }
    pack_ready(root, &selected_id(root))
}

pub fn active_model(root: &Path) -> ActiveModel {
    let id = selected_id(root);
    if id == CUSTOM_LLM {
        let meta = read_meta(root);
        return ActiveModel {
            id,
            name: meta
                .as_ref()
                .map(|meta| meta.name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Custom model".into()),
            family: meta
                .map(|meta| meta.family)
                .filter(|family| matches!(family.as_str(), "chatml" | "gemma" | "llama3"))
                .unwrap_or_else(|| "chatml".into()),
            path: custom_file(root),
        };
    }
    let spec =
        spec_by_id(&id).unwrap_or_else(|| spec_by_id(DEFAULT_LLM).expect("default language model"));
    ActiveModel {
        id: spec.id.into(),
        name: spec.label.into(),
        family: spec.family.into(),
        path: root.join(spec.id).join(spec.file),
    }
}

pub fn select_model(root: &Path, id: &str) -> Result<(), String> {
    if !known_llm(id) {
        return Err(format!("unknown language model {id}"));
    }
    if !pack_ready(root, id) {
        return Err("Download this model before Talk can use it.".into());
    }
    fs::write(root.join(SELECTED_FILE), id).map_err(|err| err.to_string())?;
    Ok(())
}

fn pack_ready(root: &Path, id: &str) -> bool {
    if let Some(spec) = spec_by_id(id) {
        if spec.kind == "llm" {
            return file_ok(&root.join(spec.id).join(spec.file));
        }
    }
    let english = speech_paths(root, "en");
    let german = speech_paths(root, "de");
    match id {
        "stt" => {
            file_ok(&english.preprocess)
                && file_ok(&english.encoder)
                && file_ok(&english.uncached)
                && file_ok(&english.cached)
                && file_ok(&english.stt_tokens)
        }
        CUSTOM_LLM => custom_ready(root),
        "tts" => {
            file_ok(&english.tts_model) && file_ok(&english.tts_tokens) && dir_ok(&english.tts_data)
        }
        "stt-de" => {
            file_ok(&german.encoder)
                && file_ok(&german.uncached)
                && file_ok(&german.joiner)
                && file_ok(&german.stt_tokens)
        }
        "tts-de" => {
            file_ok(&german.tts_model) && file_ok(&german.tts_tokens) && dir_ok(&german.tts_data)
        }
        _ => false,
    }
}

/// Speech paths must not follow the selected language model. `pack_ready` calls
/// this while deciding whether that model exists.
fn speech_paths(root: &Path, lang: &str) -> ModelPaths {
    let mut paths = model_paths_for(root, lang);
    paths.llm = root.join(DEFAULT_LLM).join(LLM_FILE);
    paths
}

fn pack_note(root: &Path, id: &str) -> String {
    if pack_ready(root, id) || !legacy_pack(root, id) {
        return String::new();
    }
    REPLACE_NOTE.into()
}

fn legacy_pack(root: &Path, id: &str) -> bool {
    match id {
        "stt" => root.join("stt").join(LEGACY_STT_DIR).exists(),
        "llm" => {
            let current = root.join(DEFAULT_LLM).join(LLM_FILE);
            let Ok(entries) = fs::read_dir(root.join("llm")) else {
                return false;
            };
            entries.flatten().any(|entry| {
                let path = entry.path();
                path != current && path.extension().and_then(|ext| ext.to_str()) == Some("gguf")
            })
        }
        _ => false,
    }
}

fn file_ok(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

fn dir_ok(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
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
        } else if meta.is_dir() {
            total += dir_bytes(&entry.path());
        }
    }
    total
}

static BUSY: AtomicBool = AtomicBool::new(false);

pub fn download_pack(
    root: &Path,
    id: &str,
    mut progress: impl FnMut(PackProgress),
) -> Result<(), String> {
    let spec = PACKS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("unknown pack {id}"))?;
    if BUSY.swap(true, Ordering::AcqRel) {
        return Err("A pack download is already running.".into());
    }
    let result = download_locked(root, spec, &mut progress);
    BUSY.store(false, Ordering::Release);
    result
}

fn download_locked(
    root: &Path,
    spec: &Spec,
    progress: &mut impl FnMut(PackProgress),
) -> Result<(), String> {
    if !crate::http::https_url_allowed(spec.url, PACK_HOSTS) {
        return Err("download address was not https".into());
    }
    let dir = root.join(spec.id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let partial = dir.join("download.partial");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(20))
        .redirect(crate::http::pack_redirect())
        .build()
        .map_err(|err| err.to_string())?;
    let mut response = client
        .get(spec.url)
        .header("user-agent", "GrokVoice/0.1.14")
        .send()
        .map_err(|err| format!("download failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("download failed ({})", response.status()));
    }
    let total = response.content_length().unwrap_or(spec.download_bytes);
    let mut file = File::create(&partial).map_err(|err| err.to_string())?;
    let mut received = 0u64;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|err| format!("download failed: {err}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        received += n as u64;
        progress(PackProgress {
            id: spec.id.into(),
            received,
            total,
        });
    }
    file.flush().map_err(|err| err.to_string())?;
    drop(file);
    if hex_encode(&hasher.finalize()) != spec.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "{} download did not match the expected checksum",
            spec.label
        ));
    }
    if spec.archive {
        clear_extracted(&dir, &partial)?;
        unpack_bz2(&partial, &dir)?;
        let _ = fs::remove_file(&partial);
    } else {
        let final_path = dir.join(spec.file);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::rename(&partial, &final_path).map_err(|err| err.to_string())?;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path != final_path
                    && path.extension().and_then(|ext| ext.to_str()) == Some("gguf")
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
    if !pack_ready(root, spec.id) {
        return Err(format!(
            "{} downloaded, but the expected files were not in the archive",
            spec.label
        ));
    }
    Ok(())
}

pub fn delete_pack(root: &Path, id: &str) -> Result<(), String> {
    if id != CUSTOM_LLM && spec_by_id(id).is_none() {
        return Err(format!("unknown pack {id}"));
    }
    let dir = root.join(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    }
    if selected_id(root) == id {
        let _ = fs::write(root.join(SELECTED_FILE), DEFAULT_LLM);
    }
    Ok(())
}

pub fn import_custom_file(root: &Path, src: &Path, name: &str, family: &str) -> Result<(), String> {
    if BUSY.swap(true, Ordering::AcqRel) {
        return Err("A pack download is already running.".into());
    }
    let result = install_custom(root, src, name, family, "file", "", "", false);
    BUSY.store(false, Ordering::Release);
    if result.is_err() && src.file_name().and_then(|name| name.to_str()) == Some(INCOMING_FILE) {
        let _ = fs::remove_file(src);
    }
    result
}

pub fn download_custom_model(
    root: &Path,
    url: &str,
    sha256: &str,
    name: &str,
    family: &str,
    mut progress: impl FnMut(PackProgress),
) -> Result<(), String> {
    let sha = normalize_sha(sha256)?;
    crate::http::custom_download_url(url)?;
    if BUSY.swap(true, Ordering::AcqRel) {
        return Err("A pack download is already running.".into());
    }
    let result = (|| {
        let dir = root.join(CUSTOM_LLM);
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let partial = dir.join("download.partial");
        download_public(&partial, url, sha.as_deref(), &mut progress)?;
        let checked = sha.is_some();
        let sha_hex = sha.unwrap_or_default();
        let installed = install_custom(root, &partial, name, family, "url", url, &sha_hex, checked);
        if installed.is_err() {
            let _ = fs::remove_file(&partial);
        }
        installed
    })();
    BUSY.store(false, Ordering::Release);
    result
}

fn spec_by_id(id: &str) -> Option<&'static Spec> {
    PACKS.iter().find(|spec| spec.id == id)
}

fn known_llm(id: &str) -> bool {
    id == CUSTOM_LLM || spec_by_id(id).is_some_and(|spec| spec.kind == "llm")
}

fn selected_id(root: &Path) -> String {
    let Ok(raw) = fs::read_to_string(root.join(SELECTED_FILE)) else {
        return DEFAULT_LLM.into();
    };
    let id = raw.trim();
    if known_llm(id) {
        id.to_string()
    } else {
        DEFAULT_LLM.into()
    }
}

fn custom_file(root: &Path) -> PathBuf {
    root.join(CUSTOM_LLM).join(CUSTOM_FILE)
}

fn custom_ready(root: &Path) -> bool {
    looks_like_gguf(&custom_file(root)) && read_meta(root).is_some()
}

fn looks_like_gguf(path: &Path) -> bool {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && &magic == b"GGUF"
}

fn read_meta(root: &Path) -> Option<CustomMeta> {
    let bytes = fs::read(root.join(CUSTOM_LLM).join(CUSTOM_META)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn custom_info(root: &Path, selected: &str) -> PackInfo {
    let path = custom_file(root);
    let ready = custom_ready(root);
    let meta = read_meta(root);
    let installed = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    let family = meta
        .as_ref()
        .map(|meta| meta.family.as_str())
        .unwrap_or("chatml");
    PackInfo {
        id: CUSTOM_LLM.into(),
        label: if ready {
            meta.as_ref()
                .map(|meta| meta.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Custom model".into())
        } else {
            "Custom model".into()
        },
        detail: if ready {
            family_detail(family).into()
        } else {
            "A GGUF you already have, or a download link. Talk only loads it as the local chat model.".into()
        },
        download_bytes: if ready { installed } else { 0 },
        installed_bytes: if ready { installed } else { 0 },
        ready,
        note: if !ready {
            String::new()
        } else if meta.as_ref().is_some_and(|meta| meta.sha256_checked) {
            "Checksum checked".into()
        } else {
            "Checksum was not checked".into()
        },
        kind: "custom".into(),
        selected: selected == CUSTOM_LLM,
    }
}

fn family_detail(family: &str) -> &'static str {
    match family {
        "gemma" => "Chat format: Gemma",
        "llama3" => "Chat format: Llama 3",
        _ => "Chat format: ChatML",
    }
}

fn clean_name(name: &str, fallback: &str) -> String {
    let cleaned = clean_label(name);
    if !cleaned.is_empty() {
        return cleaned;
    }
    let fallback = clean_label(fallback);
    if fallback.is_empty() {
        "Custom model".into()
    } else {
        fallback
    }
}

fn clean_label(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect::<String>()
        .trim()
        .to_string()
}

fn install_custom(
    root: &Path,
    src: &Path,
    name: &str,
    family: &str,
    source: &str,
    url: &str,
    sha256: &str,
    sha256_checked: bool,
) -> Result<(), String> {
    let info = inspect_gguf(src)?;
    let family = gguf::normalize_family(family, &info.family)?;
    let fallback = if info.name.is_empty() {
        "Custom model".into()
    } else {
        info.name.clone()
    };
    let name = clean_name(name, &fallback);
    let dir = root.join(CUSTOM_LLM);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let dest = dir.join(CUSTOM_FILE);
    if src != dest {
        if src.starts_with(&dir) {
            fs::rename(src, &dest).map_err(|err| err.to_string())?;
        } else {
            fs::copy(src, &dest).map_err(|err| err.to_string())?;
        }
    }
    let meta = CustomMeta {
        name,
        source: source.into(),
        url: url.into(),
        sha256: sha256.into(),
        sha256_checked,
        family,
        architecture: info.architecture,
    };
    fs::write(
        dir.join(CUSTOM_META),
        serde_json::to_vec_pretty(&meta).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::write(root.join(SELECTED_FILE), CUSTOM_LLM).map_err(|err| err.to_string())?;
    Ok(())
}

fn normalize_sha(sha: &str) -> Result<Option<String>, String> {
    let sha = sha.trim().to_ascii_lowercase();
    if sha.is_empty() {
        return Ok(None);
    }
    if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("The checksum must be a SHA-256 hex string.".into());
    }
    Ok(Some(sha))
}

fn ensure_public_host(url: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs;
    let parsed =
        reqwest::Url::parse(url).map_err(|_| "The download address must be https.".to_string())?;
    let host = parsed.host_str().unwrap_or("");
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    let mut any = false;
    for addr in (host, 443u16)
        .to_socket_addrs()
        .map_err(|_| "Could not look up that download address.".to_string())?
    {
        any = true;
        if !crate::http::ip_is_public(addr.ip()) {
            return Err("That download address is not allowed.".into());
        }
    }
    if !any {
        return Err("Could not look up that download address.".into());
    }
    Ok(())
}

fn download_public(
    dest: &Path,
    url: &str,
    sha: Option<&str>,
    progress: &mut impl FnMut(PackProgress),
) -> Result<(), String> {
    ensure_public_host(url)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| err.to_string())?;
    let mut current = url.to_string();
    let mut response = None;
    for _ in 0..5 {
        crate::http::custom_download_url(&current)?;
        ensure_public_host(&current)?;
        let resp = client
            .get(&current)
            .header("user-agent", "GrokVoice/0.1.14")
            .send()
            .map_err(|err| format!("download failed: {err}"))?;
        let status = resp.status();
        if status.is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| "download failed: redirect had no location".to_string())?;
            let base = reqwest::Url::parse(&current).map_err(|err| err.to_string())?;
            current = base.join(loc).map_err(|err| err.to_string())?.to_string();
            continue;
        }
        if !status.is_success() {
            return Err(format!("download failed ({status})"));
        }
        response = Some(resp);
        break;
    }
    let mut response = response.ok_or_else(|| "download failed: too many redirects".to_string())?;
    if response
        .content_length()
        .is_some_and(|len| len > CUSTOM_MAX_BYTES)
    {
        return Err(
            "That download is larger than 2 GB, which is too big for a phone chat model.".into(),
        );
    }
    let total = response.content_length().unwrap_or(0);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = File::create(dest).map_err(|err| err.to_string())?;
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
        received += n as u64;
        if received > CUSTOM_MAX_BYTES {
            drop(file);
            let _ = fs::remove_file(dest);
            return Err(
                "That download is larger than 2 GB, which is too big for a phone chat model."
                    .into(),
            );
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        progress(PackProgress {
            id: CUSTOM_LLM.into(),
            received,
            total: if total == 0 { received } else { total },
        });
    }
    file.flush().map_err(|err| err.to_string())?;
    drop(file);
    if received == 0 {
        let _ = fs::remove_file(dest);
        return Err("download failed: empty file".into());
    }
    if let Some(expected) = sha {
        if hex_encode(&hasher.finalize()) != expected {
            let _ = fs::remove_file(dest);
            return Err("The download did not match the checksum.".into());
        }
    }
    Ok(())
}

const PACK_HOSTS: &[&str] = &[
    "github.com",
    "*.githubusercontent.com",
    "huggingface.co",
    "*.huggingface.co",
    "*.hf.co",
];

fn clear_extracted(dir: &Path, keep: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let meta = entry.metadata().map_err(|err| err.to_string())?;
        if meta.is_dir() {
            fs::remove_dir_all(&path).map_err(|err| err.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn unpack_bz2(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    let file = File::open(archive).map_err(|err| err.to_string())?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries().map_err(|err| err.to_string())? {
        let mut entry = entry.map_err(|err| err.to_string())?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
            || kind.is_gnu_longname()
            || kind.is_gnu_longlink()
        {
            continue;
        }
        if kind.is_symlink()
            || kind.is_hard_link()
            || kind.is_fifo()
            || kind.is_character_special()
            || kind.is_block_special()
        {
            return Err("the model archive contained an unsafe path".into());
        }
        let rel = entry.path().map_err(|err| err.to_string())?.into_owned();
        let out = contained_path(dest, &rel)?;
        if kind.is_dir() {
            fs::create_dir_all(&out).map_err(|err| err.to_string())?;
            continue;
        }
        if !kind.is_file() {
            return Err("the model archive contained an unsafe path".into());
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut file = File::create(&out).map_err(|err| err.to_string())?;
        io::copy(&mut entry, &mut file).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn contained_path(dest: &Path, rel: &Path) -> Result<PathBuf, String> {
    let err = "the model archive contained an unsafe path";
    if !rel
        .components()
        .any(|component| matches!(component, Component::Normal(_)))
        || rel
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(err.into());
    }
    let out = dest.join(rel);
    let dest_canon = dest.canonicalize().map_err(|err| err.to_string())?;
    let Some(parent) = out.parent() else {
        return Err(err.into());
    };
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let parent_canon = parent.canonicalize().map_err(|err| err.to_string())?;
    if !parent_canon.starts_with(&dest_canon) {
        return Err(err.into());
    }
    if out
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(err.into());
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_tracks_each_pack() {
        let root = std::env::temp_dir().join(format!("gv-packs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(!packs_ready(&root));
        let paths = model_paths(&root);
        for path in [
            &paths.preprocess,
            &paths.encoder,
            &paths.uncached,
            &paths.cached,
            &paths.stt_tokens,
            &paths.llm,
            &paths.tts_model,
            &paths.tts_tokens,
        ] {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, b"x").unwrap();
        }
        fs::create_dir_all(&paths.tts_data).unwrap();
        let status = pack_status(&root);
        assert!(
            status
                .iter()
                .filter(|pack| pack.kind == "speech" && pack.id != "stt-de" && pack.id != "tts-de")
                .all(|pack| pack.ready)
        );
        assert!(
            status
                .iter()
                .any(|pack| pack.id == "llm" && pack.ready && pack.selected)
        );
        assert!(
            status
                .iter()
                .any(|pack| pack.id == "llm-gemma3-1b" && !pack.ready && !pack.selected)
        );
        assert!(
            status
                .iter()
                .any(|pack| pack.id == "llm-custom" && !pack.ready)
        );
        assert!(status.iter().any(|pack| pack.id == "stt-de" && !pack.ready));
        assert!(packs_ready(&root));
        assert!(!packs_ready_for(&root, "de"));
        assert_eq!(
            status
                .iter()
                .find(|pack| pack.id == "stt-de")
                .unwrap()
                .download_bytes,
            108_124_747
        );
        assert_eq!(
            status
                .iter()
                .find(|pack| pack.id == "tts-de")
                .unwrap()
                .download_bytes,
            67_101_576
        );
        assert_eq!(status[0].download_bytes, 250_807_309);
        assert_eq!(status[1].download_bytes, 144_811_360);
        assert!(status[1].label.contains("135M"));
        assert!(status[1].detail.contains("Q8_0"));
        assert!(status[0].detail.contains("Base"));
        assert!(status.iter().all(|pack| pack.note.is_empty()));
        assert!(
            status
                .iter()
                .filter(|pack| pack.ready)
                .all(|pack| pack.installed_bytes > 0)
        );
        delete_pack(&root, "llm").unwrap();
        assert!(!packs_ready(&root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_extract_writes_nested_files() {
        let root = std::env::temp_dir().join(format!("gv-tar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("pack.tar.bz2");
        {
            let encoder = bzip2::write::BzEncoder::new(
                File::create(&archive).unwrap(),
                bzip2::Compression::fast(),
            );
            let mut builder = tar::Builder::new(encoder);
            let data = b"token";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "sherpa-onnx-moonshine-tiny-en-int8/tokens.txt",
                    &data[..],
                )
                .unwrap();
            builder.finish().unwrap();
        }
        unpack_bz2(&archive, &root.join("stt")).unwrap();
        assert_eq!(
            fs::read(root.join("stt/sherpa-onnx-moonshine-tiny-en-int8/tokens.txt")).unwrap(),
            b"token"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_extract_rejects_paths_outside_the_pack() {
        let root = std::env::temp_dir().join(format!("gv-tar-escape-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.txt");
        let absolute = root.join("absolute.txt");
        for (label, name, kind) in [
            ("parent", "../outside.txt", tar::EntryType::Regular),
            (
                "absolute",
                absolute.to_str().unwrap(),
                tar::EntryType::Regular,
            ),
            ("symlink", "inside-link", tar::EntryType::Symlink),
        ] {
            let archive = root.join(format!("{label}.tar.bz2"));
            {
                let encoder = bzip2::write::BzEncoder::new(
                    File::create(&archive).unwrap(),
                    bzip2::Compression::fast(),
                );
                let mut builder = tar::Builder::new(encoder);
                let data = b"nope";
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(kind);
                header.set_mode(0o644);
                header.set_size(if kind.is_symlink() {
                    0
                } else {
                    data.len() as u64
                });
                if kind.is_symlink() {
                    header.set_link_name_literal(b"../outside").unwrap();
                }
                if name.starts_with('/') {
                    header.set_path_absolute(name).unwrap();
                } else {
                    let bytes = name.as_bytes();
                    header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
                }
                header.set_cksum();
                let body: &[u8] = if kind.is_symlink() { b"" } else { &data[..] };
                builder.append(&header, body).unwrap();
                builder.finish().unwrap();
            }
            assert!(unpack_bz2(&archive, &root.join("stt")).is_err(), "{label}");
        }
        assert!(!outside.exists());
        assert!(!absolute.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pinned_checksums_are_sha256() {
        assert!(
            PACKS.iter().all(|spec| spec.sha256.len() == 64
                && spec.sha256.chars().all(|c| c.is_ascii_hexdigit()))
        );
        assert_eq!(hex_encode(&[0x74, 0xa4]), "74a4");
        let llm = PACKS.iter().find(|spec| spec.id == "llm").unwrap();
        let stt = PACKS.iter().find(|spec| spec.id == "stt").unwrap();
        let tts = PACKS.iter().find(|spec| spec.id == "tts").unwrap();
        let stt_de = PACKS.iter().find(|spec| spec.id == "stt-de").unwrap();
        assert_eq!(
            llm.sha256,
            "5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83"
        );
        assert_eq!(
            stt.sha256,
            "21870cecaa2e44e4e2bf63e02d1072bed183ccd10284871353bd9d24dad14e5e"
        );
        assert_eq!(
            tts.sha256,
            "8fb427b8637334072ee5723d72fa418c45bfdd4b7deebeacdf2938662618c1cb"
        );
        assert_eq!(
            stt_de.sha256,
            "73b3cce9f94a19fc04bc54b55abd576b622e3925a84ddd554a95e5984bc11677"
        );
        assert!(crate::http::https_url_allowed(llm.url, PACK_HOSTS));
        assert!(crate::http::https_url_allowed(stt.url, PACK_HOSTS));
        assert_eq!(llm_model_id(), "smollm2-135m-instruct-q8_0");
        assert!(model_paths(Path::new("/models")).llm.ends_with(LLM_FILE));
        for id in [
            "llm-gemma3-1b",
            "llm-qwen25-05b",
            "llm-llama32-1b",
            "llm-lfm25-350m",
        ] {
            let spec = PACKS.iter().find(|spec| spec.id == id).unwrap();
            assert_eq!(spec.kind, "llm");
            assert!(!spec.required);
            assert!(spec.sha256.len() == 64);
            assert!(crate::http::https_url_allowed(spec.url, PACK_HOSTS));
            assert!(spec.url.ends_with(".gguf"));
        }
        let gemma = PACKS
            .iter()
            .find(|spec| spec.id == "llm-gemma3-1b")
            .unwrap();
        let qwen = PACKS
            .iter()
            .find(|spec| spec.id == "llm-qwen25-05b")
            .unwrap();
        let llama = PACKS
            .iter()
            .find(|spec| spec.id == "llm-llama32-1b")
            .unwrap();
        assert_eq!(gemma.download_bytes, 806_058_240);
        assert_eq!(
            gemma.sha256,
            "8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135"
        );
        assert_eq!(gemma.family, "gemma");
        assert!(gemma.url.contains("ggml-org/gemma-3-1b-it-GGUF"));
        assert_eq!(qwen.download_bytes, 675_710_816);
        assert_eq!(
            qwen.sha256,
            "ca59ca7f13d0e15a8cfa77bd17e65d24f6844b554a7b6c12e07a5f89ff76844e"
        );
        assert_eq!(qwen.family, "chatml");
        assert_eq!(llama.download_bytes, 807_694_464);
        assert_eq!(
            llama.sha256,
            "6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83"
        );
        assert_eq!(llama.family, "llama3");
        let lfm = PACKS
            .iter()
            .find(|spec| spec.id == "llm-lfm25-350m")
            .unwrap();
        assert_eq!(lfm.download_bytes, 379_217_632);
        assert_eq!(
            lfm.sha256,
            "be036a757295e550098b85e13f6af2735d0fa73b41e1156a40c7d8e8e32a5766"
        );
        assert_eq!(lfm.family, "lfm2");
        assert_eq!(lfm.file, LFM_FILE);
        assert!(lfm.url.contains("LiquidAI/LFM2.5-350M-GGUF"));
        assert!(lfm.url.ends_with("LFM2.5-350M-Q8_0.gguf"));
        assert!(!lfm.required);
        assert_eq!(active_model(Path::new("/models")).id, "llm");
    }

    #[test]
    fn an_older_pack_asks_for_the_new_download() {
        let root = std::env::temp_dir().join(format!("gv-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let old_llm = root.join("llm/smollm2-360m-instruct-q8_0.gguf");
        let old_stt = root.join("stt").join(LEGACY_STT_DIR).join("tokens.txt");
        fs::create_dir_all(old_llm.parent().unwrap()).unwrap();
        fs::create_dir_all(old_stt.parent().unwrap()).unwrap();
        fs::write(&old_llm, b"old-model").unwrap();
        fs::write(&old_stt, b"old-tokens").unwrap();
        let status = pack_status(&root);
        let llm = status.iter().find(|pack| pack.id == "llm").unwrap();
        let stt = status.iter().find(|pack| pack.id == "stt").unwrap();
        assert!(!llm.ready);
        assert!(!stt.ready);
        assert!(llm.note.contains("previous"));
        assert!(stt.note.contains("previous"));
        assert!(!packs_ready(&root));
        let fresh = root.join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join("download.partial"), b"keep").unwrap();
        fs::create_dir_all(fresh.join(LEGACY_STT_DIR)).unwrap();
        fs::write(fresh.join("stale.txt"), b"stale").unwrap();
        clear_extracted(&fresh, &fresh.join("download.partial")).unwrap();
        assert!(fresh.join("download.partial").exists());
        assert!(!fresh.join(LEGACY_STT_DIR).exists());
        assert!(!fresh.join("stale.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn write_gguf(path: &Path, version: u32, tensors: u64, rows: &[(&str, &str)]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(path).unwrap();
        file.write_all(b"GGUF").unwrap();
        file.write_all(&version.to_le_bytes()).unwrap();
        file.write_all(&tensors.to_le_bytes()).unwrap();
        file.write_all(&(rows.len() as u64).to_le_bytes()).unwrap();
        for (key, value) in rows {
            write_gguf_string(&mut file, key);
            file.write_all(&8u32.to_le_bytes()).unwrap();
            write_gguf_string(&mut file, value);
        }
    }

    fn write_gguf_string(file: &mut File, text: &str) {
        file.write_all(&(text.len() as u64).to_le_bytes()).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn selecting_a_model_points_talk_at_that_file() {
        let root = std::env::temp_dir().join(format!("gv-select-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(selected_id(&root), "llm");
        assert!(
            select_model(&root, "llm-gemma3-1b")
                .unwrap_err()
                .contains("Download this model")
        );
        assert!(
            select_model(&root, "missing")
                .unwrap_err()
                .contains("unknown")
        );
        let gemma = root.join("llm-gemma3-1b").join(GEMMA_FILE);
        fs::create_dir_all(gemma.parent().unwrap()).unwrap();
        fs::write(&gemma, b"weights").unwrap();
        select_model(&root, "llm-gemma3-1b").unwrap();
        let active = active_model(&root);
        assert_eq!(active.id, "llm-gemma3-1b");
        assert_eq!(active.family, "gemma");
        assert!(active.path.ends_with(GEMMA_FILE));
        assert!(model_paths(&root).llm.ends_with(GEMMA_FILE));
        let status = pack_status(&root);
        assert!(
            status
                .iter()
                .any(|pack| pack.id == "llm-gemma3-1b" && pack.selected && pack.ready)
        );
        assert!(status.iter().any(|pack| pack.id == "llm" && !pack.selected));
        delete_pack(&root, "llm-gemma3-1b").unwrap();
        assert_eq!(selected_id(&root), "llm");
        assert!(!gemma.exists());
        let lfm = root.join("llm-lfm25-350m").join(LFM_FILE);
        fs::create_dir_all(lfm.parent().unwrap()).unwrap();
        fs::write(&lfm, b"weights").unwrap();
        select_model(&root, "llm-lfm25-350m").unwrap();
        let active = active_model(&root);
        assert_eq!(active.id, "llm-lfm25-350m");
        assert_eq!(active.family, "lfm2");
        assert_eq!(active.name, "LFM2.5 350M");
        assert!(active.path.ends_with(LFM_FILE));
        assert!(model_paths(&root).llm.ends_with(LFM_FILE));
        assert!(
            pack_status(&root)
                .iter()
                .any(|pack| pack.id == "llm-lfm25-350m"
                    && pack.selected
                    && pack.ready
                    && pack.kind == "llm")
        );
        assert!(
            pack_status(&root)
                .iter()
                .any(|pack| pack.id == "llm" && !pack.selected)
        );
        delete_pack(&root, "llm-lfm25-350m").unwrap();
        assert_eq!(selected_id(&root), "llm");
        assert_eq!(active_model(&root).family, "chatml");
        assert!(!lfm.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn custom_gguf_rejects_a_missing_or_plain_file_and_accepts_a_header() {
        let root = std::env::temp_dir().join(format!("gv-byom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let missing = root.join("nope.gguf");
        assert!(
            import_custom_file(&root, &missing, "Nope", "auto")
                .unwrap_err()
                .contains("missing")
        );
        let plain = root.join("notes.txt");
        fs::write(&plain, b"hello").unwrap();
        assert!(
            import_custom_file(&root, &plain, "Nope", "auto")
                .unwrap_err()
                .contains("not a GGUF")
        );
        assert!(!pack_ready(&root, CUSTOM_LLM));
        let old = root.join("old.gguf");
        write_gguf(&old, 1, 1, &[("general.architecture", "qwen2")]);
        assert!(
            import_custom_file(&root, &old, "Old", "auto")
                .unwrap_err()
                .contains("version")
        );
        let empty = root.join("empty.gguf");
        write_gguf(&empty, 3, 0, &[("general.architecture", "qwen2")]);
        assert!(
            import_custom_file(&root, &empty, "Empty", "auto")
                .unwrap_err()
                .contains("no tensors")
        );
        let unknown = root.join("unknown.gguf");
        write_gguf(&unknown, 3, 2, &[("general.architecture", "not-a-model")]);
        assert!(
            import_custom_file(&root, &unknown, "Odd", "auto")
                .unwrap_err()
                .contains("not supported")
        );
        let good = root.join("mine.gguf");
        write_gguf(
            &good,
            3,
            4,
            &[
                ("general.architecture", "qwen2"),
                ("general.name", "Pocket Qwen"),
                ("tokenizer.chat_template", "<|im_start|>system\n"),
            ],
        );
        import_custom_file(&root, &good, "  My phone model  ", "auto").unwrap();
        assert!(
            good.exists(),
            "the original file stays where the user had it"
        );
        let active = active_model(&root);
        assert_eq!(active.id, CUSTOM_LLM);
        assert_eq!(active.name, "My phone model");
        assert_eq!(active.family, "chatml");
        assert!(active.path.ends_with("llm-custom/model.gguf"));
        let info = inspect_gguf(&active.path).unwrap();
        assert_eq!(info.version, 3);
        assert_eq!(info.architecture, "qwen2");
        assert_eq!(info.tensors, 4);
        let custom = pack_status(&root)
            .into_iter()
            .find(|pack| pack.id == CUSTOM_LLM)
            .unwrap();
        assert!(custom.ready);
        assert!(custom.selected);
        assert_eq!(custom.label, "My phone model");
        assert_eq!(custom.detail, "Chat format: ChatML");
        assert!(custom.note.contains("not checked"));
        assert!(custom.installed_bytes > 0);
        let paths = model_paths(&root);
        for path in [
            &paths.preprocess,
            &paths.encoder,
            &paths.uncached,
            &paths.cached,
            &paths.stt_tokens,
            &paths.tts_model,
            &paths.tts_tokens,
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"x").unwrap();
        }
        fs::create_dir_all(&paths.tts_data).unwrap();
        assert!(packs_ready(&root));
        assert!(!root.join("llm").join(LLM_FILE).exists());
        delete_pack(&root, CUSTOM_LLM).unwrap();
        assert_eq!(selected_id(&root), "llm");
        assert!(!custom_ready(&root));
        let gemma_header = root.join("gemma.gguf");
        write_gguf(
            &gemma_header,
            3,
            8,
            &[
                ("general.architecture", "gemma3"),
                ("tokenizer.chat_template", "<start_of_turn>user"),
            ],
        );
        import_custom_file(&root, &gemma_header, "", "auto").unwrap();
        assert_eq!(active_model(&root).family, "gemma");
        assert_eq!(
            pack_status(&root)
                .into_iter()
                .find(|pack| pack.id == CUSTOM_LLM)
                .unwrap()
                .detail,
            "Chat format: Gemma"
        );
        import_custom_file(&root, &gemma_header, "Forced", "llama3").unwrap();
        assert_eq!(active_model(&root).family, "llama3");
        assert_eq!(active_model(&root).name, "Forced");
        assert!(normalize_sha("").unwrap().is_none());
        assert!(normalize_sha("ZZ").is_err());
        assert_eq!(
            normalize_sha("AB").unwrap_err(),
            "The checksum must be a SHA-256 hex string."
        );
        let _ = fs::remove_dir_all(&root);
    }
}
