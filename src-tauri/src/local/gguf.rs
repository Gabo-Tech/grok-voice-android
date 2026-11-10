//! Enough of a GGUF header read to decide whether llama.cpp `b12140` can load the file.
//! The bytes are never executed. Talk only passes the path to the existing loader.

use std::{fs::File, io::Read, path::Path};

const HEADER_BUDGET: u64 = 64 * 1024 * 1024;
const MAX_STRING: u64 = 8 * 1024 * 1024;
const MAX_KV: u64 = 100_000;
const MAX_TENSORS: u64 = 1_000_000;
const MAX_ARRAY: u64 = 4_000_000;

const NOT_GGUF: &str = "That file is not a GGUF model.";
const BAD_VERSION: &str = "This GGUF version is not supported by the on-device loader.";
const NO_ARCH: &str = "This GGUF has no model architecture, so Talk cannot load it.";
const BAD_ARCH: &str = "This GGUF architecture is not supported by the on-device loader.";
const NO_TENSORS: &str = "This GGUF has no tensors, so Talk cannot load it.";
const HEADER_TOO_BIG: &str = "This GGUF header is too large to check.";
const MISSING: &str = "That file is missing.";

/// Architectures llama.cpp b12140 knows how to load as a generative model.
/// Embeddings, speech synthesizers, OCR, and encoder-only weights are left out.
const ARCHITECTURES: &[&str] = &[
    "afmoe",
    "apertus",
    "arcee",
    "arctic",
    "arwkv7",
    "baichuan",
    "bailingmoe",
    "bailingmoe2",
    "bailingmoe3",
    "beam",
    "bitnet",
    "bloom",
    "chameleon",
    "chatglm",
    "clef",
    "clover_moe",
    "codeshell",
    "cogvlm",
    "cohere2",
    "cohere2moe",
    "command-r",
    "dbrx",
    "deci",
    "deepseek",
    "deepseek2",
    "deepseek2-ocr",
    "deepseek32",
    "deepseek4",
    "deepseek41",
    "dflash",
    "dots1",
    "dots3note",
    "dream",
    "eagle3",
    "ernie4_5",
    "ernie4_5-moe",
    "exaone",
    "exaone-moe",
    "exaone4",
    "falcon",
    "falcon-h1",
    "flint",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gemma4",
    "gemma4-assistant",
    "glm-dsa",
    "glm4",
    "glm4moe",
    "glm5-next",
    "gpt-oss",
    "gpt2",
    "gptj",
    "gptneox",
    "granite",
    "granite_swa",
    "granitehybrid",
    "granitemoe",
    "graniteswitch",
    "grok",
    "grovemoe",
    "hrm_text",
    "hunyuan-dense",
    "hunyuan-moe",
    "hunyuan_vl",
    "hy_v3",
    "hy_v4",
    "internlm2",
    "jais",
    "jais2",
    "jamba",
    "k2-horizon",
    "kimi-k3",
    "kimi-linear",
    "kolibri1",
    "laguna",
    "lfm2",
    "lfm2moe",
    "lfm3",
    "limite",
    "llada",
    "llada-moe",
    "llama",
    "llama4",
    "maincoder",
    "maion-coder",
    "mamba",
    "mamba2",
    "maple",
    "mellum",
    "metis16",
    "mimo2",
    "minicpm",
    "minicpm3",
    "minimax-01",
    "minimax-m2",
    "minimax-m3",
    "mistral-large4",
    "mistral3",
    "mistral4",
    "mpt",
    "muse-glimmer",
    "muse-spark",
    "nanbeige",
    "nemotron",
    "nemotron_h",
    "nemotron_h_moe",
    "olmo",
    "olmo2",
    "olmoe",
    "openelm",
    "orion",
    "phi2",
    "phi3",
    "phimoe",
    "plamo",
    "plamo2",
    "plamo3",
    "plm",
    "qwen",
    "qwen2",
    "qwen2moe",
    "qwen2vl",
    "qwen3",
    "qwen35",
    "qwen35moe",
    "qwen3moe",
    "qwen3next",
    "qwen3vl",
    "qwen3vlmoe",
    "qwen4exp",
    "refact",
    "rnd1",
    "rwkv6",
    "rwkv6qwen2",
    "rwkv7",
    "seed_oss",
    "sensenova_u1",
    "smallthinker",
    "smollm3",
    "spark2_5",
    "stablelm",
    "starcoder",
    "starcoder2",
    "step35",
    "t5",
    "talkie",
    "vllm-decision2",
    "xverse",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GgufInfo {
    pub version: u32,
    pub tensors: u64,
    pub architecture: String,
    pub name: String,
    pub chat_template: String,
    pub family: String,
}

struct Budget<R> {
    inner: R,
    left: u64,
    saw_magic: bool,
}

impl<R: Read> Budget<R> {
    fn take(&mut self, n: u64) -> Result<(), String> {
        if n > self.left {
            return Err(HEADER_TOO_BIG.into());
        }
        self.left -= n;
        Ok(())
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), String> {
        self.take(buf.len() as u64)?;
        self.inner.read_exact(buf).map_err(|_| self.short())?;
        Ok(())
    }

    fn short(&self) -> String {
        if self.saw_magic {
            "This GGUF header is incomplete.".into()
        } else {
            NOT_GGUF.into()
        }
    }

    fn u32(&mut self) -> Result<u32, String> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u64()?;
        if n > MAX_STRING {
            return Err(HEADER_TOO_BIG.into());
        }
        let mut buf = vec![0u8; n as usize];
        self.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| NOT_GGUF.into())
    }

    fn skip(&mut self, n: u64) -> Result<(), String> {
        self.take(n)?;
        let mut left = n;
        let mut buf = [0u8; 64 * 1024];
        while left > 0 {
            let chunk = left.min(buf.len() as u64) as usize;
            self.inner
                .read_exact(&mut buf[..chunk])
                .map_err(|_| self.short())?;
            left -= chunk as u64;
        }
        Ok(())
    }
}

pub fn inspect_gguf(path: &Path) -> Result<GgufInfo, String> {
    if !path.is_file() {
        return Err(MISSING.into());
    }
    let file = File::open(path).map_err(|_| MISSING.to_string())?;
    let mut reader = Budget {
        inner: file,
        left: HEADER_BUDGET,
        saw_magic: false,
    };
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != b"GGUF" {
        return Err(NOT_GGUF.into());
    }
    reader.saw_magic = true;
    let version = reader.u32()?;
    if version == 0 || version == 1 || version > 3 || version & 0x0000_ffff == 0 {
        return Err(BAD_VERSION.into());
    }
    let tensors = reader.u64()?;
    if tensors == 0 {
        return Err(NO_TENSORS.into());
    }
    if tensors > MAX_TENSORS {
        return Err(BAD_ARCH.into());
    }
    let n_kv = reader.u64()?;
    if n_kv > MAX_KV {
        return Err(HEADER_TOO_BIG.into());
    }
    let mut architecture = String::new();
    let mut name = String::new();
    let mut chat_template = String::new();
    for _ in 0..n_kv {
        let key = reader.string()?;
        if key.is_empty() || key.len() > 256 {
            return Err(NOT_GGUF.into());
        }
        let mut value_type = reader.u32()?;
        let mut count = 1u64;
        let mut is_array = false;
        if value_type == 9 {
            is_array = true;
            value_type = reader.u32()?;
            count = reader.u64()?;
            if value_type == 9 || count > MAX_ARRAY {
                return Err(HEADER_TOO_BIG.into());
            }
        }
        let capture = !is_array
            && value_type == 8
            && matches!(
                key.as_str(),
                "general.architecture" | "general.name" | "tokenizer.chat_template"
            );
        if capture {
            let value = reader.string()?;
            match key.as_str() {
                "general.architecture" if architecture.is_empty() => architecture = value,
                "general.name" if name.is_empty() => name = value,
                "tokenizer.chat_template" if chat_template.is_empty() => chat_template = value,
                _ => {}
            }
        } else {
            skip_values(&mut reader, value_type, count)?;
        }
        if !architecture.is_empty() && !chat_template.is_empty() && !name.is_empty() {
            break;
        }
    }
    let architecture = architecture.trim().to_string();
    if architecture.is_empty() {
        return Err(NO_ARCH.into());
    }
    if ARCHITECTURES.binary_search(&architecture.as_str()).is_err() {
        return Err(BAD_ARCH.into());
    }
    let family = prompt_family(&architecture, &chat_template).to_string();
    Ok(GgufInfo {
        version,
        tensors,
        architecture,
        name: name.trim().to_string(),
        chat_template,
        family,
    })
}

fn skip_values<R: Read>(reader: &mut Budget<R>, value_type: u32, count: u64) -> Result<(), String> {
    if value_type == 8 {
        for _ in 0..count {
            let n = reader.u64()?;
            if n > MAX_STRING {
                return Err(HEADER_TOO_BIG.into());
            }
            reader.skip(n)?;
        }
        return Ok(());
    }
    let size = scalar_size(value_type)?;
    let total = size
        .checked_mul(count)
        .ok_or_else(|| HEADER_TOO_BIG.to_string())?;
    reader.skip(total)
}

fn scalar_size(value_type: u32) -> Result<u64, String> {
    Ok(match value_type {
        0 | 1 | 7 => 1,
        2 | 3 => 2,
        4 | 5 | 6 => 4,
        10 | 11 | 12 => 8,
        _ => return Err(NOT_GGUF.into()),
    })
}

pub fn prompt_family(architecture: &str, template: &str) -> &'static str {
    if template.contains("<|im_start|>") {
        return "chatml";
    }
    if template.contains("<start_of_turn>") {
        return "gemma";
    }
    if template.contains("<|start_header_id|>") {
        return "llama3";
    }
    if architecture.starts_with("gemma") {
        return "gemma";
    }
    if architecture == "llama" || architecture == "llama4" {
        return "llama3";
    }
    "chatml"
}

pub fn normalize_family(requested: &str, detected: &str) -> Result<String, String> {
    match requested.trim() {
        "" | "auto" => Ok(if detected.is_empty() {
            "chatml".into()
        } else {
            detected.to_string()
        }),
        "chatml" | "gemma" | "llama3" => Ok(requested.trim().to_string()),
        _ => Err("Choose ChatML, Gemma, or Llama 3 for the chat format.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_architectures_are_sorted_and_include_the_phone_models() {
        let mut sorted = ARCHITECTURES.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, ARCHITECTURES);
        assert!(ARCHITECTURES.binary_search(&"gemma3").is_ok());
        assert!(ARCHITECTURES.binary_search(&"qwen2").is_ok());
        assert!(ARCHITECTURES.binary_search(&"llama").is_ok());
        assert!(ARCHITECTURES.binary_search(&"lfm2").is_ok());
        assert!(ARCHITECTURES.binary_search(&"smollm3").is_ok());
        assert!(ARCHITECTURES.binary_search(&"bert").is_err());
        assert!(ARCHITECTURES.binary_search(&"qwen3tts").is_err());
    }

    #[test]
    fn chat_family_follows_the_template_then_the_architecture() {
        assert_eq!(prompt_family("qwen2", "<|im_start|>system"), "chatml");
        assert_eq!(prompt_family("gemma3", "<start_of_turn>user"), "gemma");
        assert_eq!(
            prompt_family("llama", "<|start_header_id|>system"),
            "llama3"
        );
        assert_eq!(prompt_family("gemma3", ""), "gemma");
        assert_eq!(prompt_family("llama", ""), "llama3");
        assert_eq!(prompt_family("qwen2", ""), "chatml");
        assert_eq!(normalize_family("auto", "gemma").unwrap(), "gemma");
        assert_eq!(normalize_family("llama3", "chatml").unwrap(), "llama3");
        assert!(normalize_family("ollama", "chatml").is_err());
    }
}
