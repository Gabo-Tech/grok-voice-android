//! On-demand model packs. Weights stay out of the APK and live under app storage.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub const STT_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-tiny-en-int8.tar.bz2";
pub const LLM_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
pub const TTS_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-low.tar.bz2";

struct Spec {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    url: &'static str,
    download_bytes: u64,
    archive: bool,
}

const PACKS: &[Spec] = &[
    Spec {
        id: "stt",
        label: "Speech to text",
        detail: "Moonshine Tiny English, int8",
        url: STT_URL,
        download_bytes: 107_600_538,
        archive: true,
    },
    Spec {
        id: "llm",
        label: "Language model",
        detail: "Qwen2.5 0.5B Instruct, Q4_K_M",
        url: LLM_URL,
        download_bytes: 491_400_032,
        archive: false,
    },
    Spec {
        id: "tts",
        label: "Voice",
        detail: "Piper en_US lessac, low",
        url: TTS_URL,
        download_bytes: 67_097_098,
        archive: true,
    },
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub download_bytes: u64,
    pub ready: bool,
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
    pub preprocess: PathBuf,
    pub encoder: PathBuf,
    pub uncached: PathBuf,
    pub cached: PathBuf,
    pub stt_tokens: PathBuf,
    pub llm: PathBuf,
    pub tts_model: PathBuf,
    pub tts_tokens: PathBuf,
    pub tts_data: PathBuf,
}

pub fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?.join("local-models");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn model_paths(root: &Path) -> ModelPaths {
    let stt = root.join("stt/sherpa-onnx-moonshine-tiny-en-int8");
    let tts = root.join("tts/vits-piper-en_US-lessac-low");
    ModelPaths {
        preprocess: stt.join("preprocess.onnx"),
        encoder: stt.join("encode.int8.onnx"),
        uncached: stt.join("uncached_decode.int8.onnx"),
        cached: stt.join("cached_decode.int8.onnx"),
        stt_tokens: stt.join("tokens.txt"),
        llm: root.join("llm/qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        tts_model: tts.join("en_US-lessac-low.onnx"),
        tts_tokens: tts.join("tokens.txt"),
        tts_data: tts.join("espeak-ng-data"),
    }
}

pub fn pack_status(root: &Path) -> Vec<PackInfo> {
    PACKS
        .iter()
        .map(|spec| PackInfo {
            id: spec.id.into(),
            label: spec.label.into(),
            detail: spec.detail.into(),
            download_bytes: spec.download_bytes,
            ready: pack_ready(root, spec.id),
        })
        .collect()
}

pub fn packs_ready(root: &Path) -> bool {
    PACKS.iter().all(|spec| pack_ready(root, spec.id))
}

fn pack_ready(root: &Path, id: &str) -> bool {
    let paths = model_paths(root);
    match id {
        "stt" => file_ok(&paths.preprocess)
            && file_ok(&paths.encoder)
            && file_ok(&paths.uncached)
            && file_ok(&paths.cached)
            && file_ok(&paths.stt_tokens),
        "llm" => file_ok(&paths.llm),
        "tts" => file_ok(&paths.tts_model) && file_ok(&paths.tts_tokens) && dir_ok(&paths.tts_data),
        _ => false,
    }
}

fn file_ok(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.is_file() && meta.len() > 0).unwrap_or(false)
}

fn dir_ok(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.is_dir()).unwrap_or(false)
}

static BUSY: AtomicBool = AtomicBool::new(false);

pub fn download_pack(root: &Path, id: &str, mut progress: impl FnMut(PackProgress)) -> Result<(), String> {
    let spec = PACKS.iter().find(|spec| spec.id == id).ok_or_else(|| format!("unknown pack {id}"))?;
    if BUSY.swap(true, Ordering::AcqRel) {
        return Err("A pack download is already running.".into());
    }
    let result = download_locked(root, spec, &mut progress);
    BUSY.store(false, Ordering::Release);
    result
}

fn download_locked(root: &Path, spec: &Spec, progress: &mut impl FnMut(PackProgress)) -> Result<(), String> {
    let dir = root.join(spec.id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let partial = dir.join("download.partial");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .connect_timeout(std::time::Duration::from_secs(20))
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
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buf).map_err(|err| format!("download failed: {err}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        received += n as u64;
        progress(PackProgress { id: spec.id.into(), received, total });
    }
    file.flush().map_err(|err| err.to_string())?;
    drop(file);
    if spec.archive {
        unpack_bz2(&partial, &dir)?;
        let _ = fs::remove_file(&partial);
    } else {
        let final_path = model_paths(root).llm;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::rename(&partial, &final_path).map_err(|err| err.to_string())?;
    }
    if !pack_ready(root, spec.id) {
        return Err(format!("{} downloaded, but the expected files were not in the archive", spec.label));
    }
    Ok(())
}

pub fn delete_pack(root: &Path, id: &str) -> Result<(), String> {
    if !PACKS.iter().any(|spec| spec.id == id) {
        return Err(format!("unknown pack {id}"));
    }
    let dir = root.join(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn unpack_bz2(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|err| err.to_string())?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries().map_err(|err| err.to_string())? {
        let mut entry = entry.map_err(|err| err.to_string())?;
        let rel = entry.path().map_err(|err| err.to_string())?.into_owned();
        if rel.components().any(|component| matches!(component, Component::ParentDir)) {
            return Err("the model archive contained an unsafe path".into());
        }
        let out = dest.join(&rel);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out).map_err(|err| err.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut file = File::create(&out).map_err(|err| err.to_string())?;
        io::copy(&mut entry, &mut file).map_err(|err| err.to_string())?;
    }
    Ok(())
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
        for path in [&paths.preprocess, &paths.encoder, &paths.uncached, &paths.cached, &paths.stt_tokens, &paths.llm, &paths.tts_model, &paths.tts_tokens] {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, b"x").unwrap();
        }
        fs::create_dir_all(&paths.tts_data).unwrap();
        let status = pack_status(&root);
        assert!(status.iter().all(|pack| pack.ready));
        assert!(packs_ready(&root));
        assert_eq!(status[1].download_bytes, 491_400_032);
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
            let encoder = bzip2::write::BzEncoder::new(File::create(&archive).unwrap(), bzip2::Compression::fast());
            let mut builder = tar::Builder::new(encoder);
            let data = b"token";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "sherpa-onnx-moonshine-tiny-en-int8/tokens.txt", &data[..])
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
}
