//! On-demand model packs. Weights stay out of the APK and live under app storage.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

pub const STT_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-tiny-en-int8.tar.bz2";
pub const LLM_URL: &str =
    "https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct-GGUF/resolve/main/smollm2-360m-instruct-q8_0.gguf";
pub const TTS_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-low.tar.bz2";
pub const STT_DE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8.tar.bz2";
pub const TTS_DE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-de_DE-thorsten-low.tar.bz2";

struct Spec {
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    url: &'static str,
    download_bytes: u64,
    sha256: &'static str,
    archive: bool,
    /// English speech plus the language model. German packs are optional.
    required: bool,
}

const PACKS: &[Spec] = &[
    Spec {
        id: "stt",
        label: "Speech to text",
        detail: "Moonshine Tiny English, int8",
        url: STT_URL,
        download_bytes: 107_600_538,
        sha256: "d5fe6ec4334fef36255b2a4010412cad4c007e33103fec62fb5d17cad88086f2",
        archive: true,
        required: true,
    },
    Spec {
        id: "llm",
        label: "Language model",
        detail: "SmolLM2 360M Instruct, Q8_0",
        url: LLM_URL,
        download_bytes: 386_404_992,
        sha256: "48ab3034d0dd401fbc721eb1df3217902fee7dab9078992d66431f09b7750201",
        archive: false,
        required: true,
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
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?.join("local-models");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn model_paths(root: &Path) -> ModelPaths {
    model_paths_for(root, "en")
}

pub fn model_paths_for(root: &Path, lang: &str) -> ModelPaths {
    let llm = root.join("llm/smollm2-360m-instruct-q8_0.gguf");
    if lang == "de" {
        let stt = root.join("stt-de/sherpa-onnx-nemo-transducer-stt_de_fastconformer_hybrid_large_pc-int8");
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
    let stt = root.join("stt/sherpa-onnx-moonshine-tiny-en-int8");
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
    PACKS
        .iter()
        .map(|spec| PackInfo {
            id: spec.id.into(),
            label: spec.label.into(),
            detail: spec.detail.into(),
            download_bytes: spec.download_bytes,
            installed_bytes: dir_bytes(&root.join(spec.id)),
            ready: pack_ready(root, spec.id),
        })
        .collect()
}

pub fn packs_ready(root: &Path) -> bool {
    packs_ready_for(root, "en")
}

pub fn packs_ready_for(root: &Path, lang: &str) -> bool {
    if !PACKS.iter().filter(|spec| spec.required).all(|spec| pack_ready(root, spec.id)) {
        return false;
    }
    if lang == "de" {
        return pack_ready(root, "stt-de") && pack_ready(root, "tts-de");
    }
    true
}

fn pack_ready(root: &Path, id: &str) -> bool {
    let english = model_paths_for(root, "en");
    let german = model_paths_for(root, "de");
    match id {
        "stt" => file_ok(&english.preprocess)
            && file_ok(&english.encoder)
            && file_ok(&english.uncached)
            && file_ok(&english.cached)
            && file_ok(&english.stt_tokens),
        "llm" => file_ok(&english.llm),
        "tts" => file_ok(&english.tts_model) && file_ok(&english.tts_tokens) && dir_ok(&english.tts_data),
        "stt-de" => file_ok(&german.encoder) && file_ok(&german.uncached) && file_ok(&german.joiner) && file_ok(&german.stt_tokens),
        "tts-de" => file_ok(&german.tts_model) && file_ok(&german.tts_tokens) && dir_ok(&german.tts_data),
        _ => false,
    }
}

fn file_ok(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.is_file() && meta.len() > 0).unwrap_or(false)
}

fn dir_ok(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.is_dir()).unwrap_or(false)
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
        let n = response.read(&mut buf).map_err(|err| format!("download failed: {err}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        received += n as u64;
        progress(PackProgress { id: spec.id.into(), received, total });
    }
    file.flush().map_err(|err| err.to_string())?;
    drop(file);
    if hex_encode(&hasher.finalize()) != spec.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!("{} download did not match the expected checksum", spec.label));
    }
    if spec.archive {
        unpack_bz2(&partial, &dir)?;
        let _ = fs::remove_file(&partial);
    } else {
        let final_path = model_paths(root).llm;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::rename(&partial, &final_path).map_err(|err| err.to_string())?;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path != final_path && path.extension().and_then(|ext| ext.to_str()) == Some("gguf") {
                    let _ = fs::remove_file(path);
                }
            }
        }
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

const PACK_HOSTS: &[&str] = &[
    "github.com",
    "*.githubusercontent.com",
    "huggingface.co",
    "*.huggingface.co",
    "*.hf.co",
];

fn unpack_bz2(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    let file = File::open(archive).map_err(|err| err.to_string())?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries().map_err(|err| err.to_string())? {
        let mut entry = entry.map_err(|err| err.to_string())?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() || kind.is_gnu_longname() || kind.is_gnu_longlink() {
            continue;
        }
        if kind.is_symlink() || kind.is_hard_link() || kind.is_fifo() || kind.is_character_special() || kind.is_block_special() {
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
    if !rel.components().any(|component| matches!(component, Component::Normal(_)))
        || rel.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
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
    if out.symlink_metadata().is_ok_and(|meta| meta.file_type().is_symlink()) {
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
        for path in [&paths.preprocess, &paths.encoder, &paths.uncached, &paths.cached, &paths.stt_tokens, &paths.llm, &paths.tts_model, &paths.tts_tokens] {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, b"x").unwrap();
        }
        fs::create_dir_all(&paths.tts_data).unwrap();
        let status = pack_status(&root);
        assert!(status.iter().filter(|pack| pack.id != "stt-de" && pack.id != "tts-de").all(|pack| pack.ready));
        assert!(status.iter().any(|pack| pack.id == "stt-de" && !pack.ready));
        assert!(packs_ready(&root));
        assert!(!packs_ready_for(&root, "de"));
        assert_eq!(status.iter().find(|pack| pack.id == "stt-de").unwrap().download_bytes, 108_124_747);
        assert_eq!(status.iter().find(|pack| pack.id == "tts-de").unwrap().download_bytes, 67_101_576);
        assert_eq!(status[1].download_bytes, 386_404_992);
        assert!(status.iter().filter(|pack| pack.ready).all(|pack| pack.installed_bytes > 0));
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

    #[test]
    fn archive_extract_rejects_paths_outside_the_pack() {
        let root = std::env::temp_dir().join(format!("gv-tar-escape-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.txt");
        let absolute = root.join("absolute.txt");
        for (label, name, kind) in [
            ("parent", "../outside.txt", tar::EntryType::Regular),
            ("absolute", absolute.to_str().unwrap(), tar::EntryType::Regular),
            ("symlink", "inside-link", tar::EntryType::Symlink),
        ] {
            let archive = root.join(format!("{label}.tar.bz2"));
            {
                let encoder = bzip2::write::BzEncoder::new(File::create(&archive).unwrap(), bzip2::Compression::fast());
                let mut builder = tar::Builder::new(encoder);
                let data = b"nope";
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(kind);
                header.set_mode(0o644);
                header.set_size(if kind.is_symlink() { 0 } else { data.len() as u64 });
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
        assert!(PACKS.iter().all(|spec| spec.sha256.len() == 64 && spec.sha256.chars().all(|c| c.is_ascii_hexdigit())));
        assert_eq!(hex_encode(&[0x74, 0xa4]), "74a4");
    }
}
