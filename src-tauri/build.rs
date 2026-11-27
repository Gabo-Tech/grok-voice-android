fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target == "aarch64-linux-android" {
        build_android_local().unwrap_or_else(|err| panic!("on-device native build failed: {err}"));
    }
    tauri_build::build();
}

fn build_android_local() -> Result<(), String> {
    println!("cargo:rerun-if-changed=native/local_engine.cpp");
    println!("cargo:rerun-if-changed=native/local_engine.h");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    println!("cargo:rerun-if-env-changed=NDK_HOME");

    let manifest =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let cache = cache_dir(&manifest);
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;

    let ndk = find_ndk()?;
    let sherpa = prepare_sherpa(&cache)?;
    let llama = prepare_llama(&cache, &ndk)?;
    let jni = manifest.join("gen/android/app/src/main/jniLibs/arm64-v8a");
    std::fs::create_dir_all(&jni).map_err(|e| e.to_string())?;
    copy_file(
        &sherpa.join("libsherpa-onnx-c-api.so"),
        &jni.join("libsherpa-onnx-c-api.so"),
    )?;
    copy_file(
        &sherpa.join("libonnxruntime.so"),
        &jni.join("libonnxruntime.so"),
    )?;
    let phonon = prepare_phonon(&cache, &ndk)?;
    copy_file(&phonon, &jni.join("libptts_ffi.so"))?;

    let cxx = find_cxx_shared(&ndk)?;
    copy_file(&cxx, &jni.join("libc++_shared.so"))?;

    let clang = ndk_tool(&ndk, "aarch64-linux-android26-clang")?;
    let ar = ndk_tool(&ndk, "llvm-ar")?;
    let mut build = cc::Build::new();
    build
        .file(manifest.join("native/local_engine.cpp"))
        .include(manifest.join("native"))
        .include(sherpa.join("include"))
        .include(llama.join("include"))
        .include(llama.join("ggml/include"))
        .compiler(&clang)
        .archiver(&ar)
        .target("aarch64-linux-android")
        .cpp(true)
        .flag("-fexceptions")
        .flag("-frtti")
        .flag("-fPIC")
        .warnings(false)
        .compile("gv_local_engine");

    println!("cargo:rustc-link-search=native={}", jni.display());
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    let mut libs = Vec::new();
    collect_static_libs(&llama.join("build"), &mut libs)?;
    if libs.is_empty() {
        return Err("llama.cpp produced no static libraries".into());
    }
    for lib in libs {
        println!("cargo:rustc-link-arg={}", lib.display());
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");
    println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
    println!("cargo:rustc-link-lib=dylib=c++_shared");
    println!("cargo:rustc-link-arg=-Wl,--as-needed");
    println!("cargo:rustc-link-lib=dylib=sherpa-onnx-c-api");
    println!("cargo:rustc-link-lib=dylib=ptts_ffi");
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
    println!("cargo:rustc-link-lib=dylib=log");
    println!("cargo:rustc-link-lib=dylib=android");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
    Ok(())
}

fn cache_dir(manifest: &std::path::Path) -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir).join("native-cache");
        }
    }
    manifest.join("target").join("native-cache")
}

fn find_ndk() -> Result<std::path::PathBuf, String> {
    let mut candidates = Vec::new();
    for key in ["ANDROID_NDK_HOME", "NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Ok(value) = std::env::var(key) {
            candidates.push(std::path::PathBuf::from(value));
        }
    }
    let sdk = std::env::var("ANDROID_HOME")
        .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{home}/Android/Sdk")
        });
    candidates.push(std::path::PathBuf::from(&sdk).join("ndk/27.1.12297006"));
    if let Ok(entries) = std::fs::read_dir(std::path::Path::new(&sdk).join("ndk")) {
        for entry in entries.flatten() {
            candidates.push(entry.path());
        }
    }
    for candidate in candidates {
        if candidate
            .join("build/cmake/android.toolchain.cmake")
            .is_file()
        {
            return Ok(candidate);
        }
    }
    Err("Android NDK not found. Set ANDROID_NDK_HOME to NDK 27.1.".into())
}

fn ndk_host() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-arm64"
        } else {
            "darwin-x86_64"
        }
    } else {
        "linux-x86_64"
    }
}

fn ndk_tool(ndk: &std::path::Path, name: &str) -> Result<std::path::PathBuf, String> {
    let path = ndk
        .join("toolchains/llvm/prebuilt")
        .join(ndk_host())
        .join("bin")
        .join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("NDK tool missing: {}", path.display()))
    }
}

fn find_cxx_shared(ndk: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let candidates = [
        ndk.join("toolchains/llvm/prebuilt")
            .join(ndk_host())
            .join("sysroot/usr/lib/aarch64-linux-android/libc++_shared.so"),
        ndk.join("sources/cxx-stl/llvm-libc++/libs/arm64-v8a/libc++_shared.so"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "libc++_shared.so was not in the NDK".into())
}

const SHERPA_TAG: &str = "v1.13.8";
const LLAMA_TAG: &str = "b12774";
const SHERPA_ARCHIVE_SHA256: &str = "2ff63469a71cb6009aa2e3ed5f4a670f8abdcbe4bb9ffd23776afc792a6b4f44";
const SHERPA_HEADER_SHA256: &str = "2a1b95084be8fd1deb3228fcad2fd3f7f0258b64582f7402281ec174c7b7f4ce";
const LLAMA_ARCHIVE_SHA256: &str = "615a08fa4154fbc8febdb470b33d8aa48111bc8651aba95f457e7e6cc32f7aaf";
/// Commit that v0.7.1 of xn-ptts points at. The tag itself is not trusted.
const PHONON_COMMIT: &str = "5f9522b3e245846d312caaeb460d3dcae5d1a991";

/// Official Phonon CPU library, built from a pinned commit of xn-ptts in its
/// own Cargo project. It is not a dependency of this crate: its `half` version
/// would clash with the one candle needs.
fn prepare_phonon(
    cache: &std::path::Path,
    ndk: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let repo = cache.join("xn-ptts");
    let marker = repo.join("target/aarch64-linux-android/release/libptts_ffi.so");
    if git_head(&repo).as_deref() != Some(PHONON_COMMIT) {
        if repo.exists() {
            std::fs::remove_dir_all(&repo).map_err(|err| err.to_string())?;
        }
        std::fs::create_dir_all(&repo).map_err(|err| err.to_string())?;
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init"])
            .status()
            .map_err(|err| format!("git init phonon: {err}"))?;
        if !init.success() {
            return Err("could not prepare the Phonon checkout".into());
        }
        let remote = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["remote", "add", "origin", "https://github.com/gradium-ai/xn-ptts.git"])
            .status()
            .map_err(|err| format!("git remote phonon: {err}"))?;
        if !remote.success() {
            return Err("could not set the Phonon remote".into());
        }
        let fetched = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["fetch", "--depth", "1", "origin", PHONON_COMMIT])
            .status()
            .map_err(|err| format!("git fetch phonon: {err}"))?;
        if !fetched.success() {
            return Err("could not fetch the pinned Phonon commit".into());
        }
        let checkout = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["checkout", "--detach", "FETCH_HEAD"])
            .status()
            .map_err(|err| format!("git checkout phonon: {err}"))?;
        if !checkout.success() || git_head(&repo).as_deref() != Some(PHONON_COMMIT) {
            return Err(format!("Phonon checkout is not {PHONON_COMMIT}"));
        }
    }
    if !marker.is_file() {
        let clang = ndk_tool(ndk, "aarch64-linux-android26-clang")?;
        let ar = ndk_tool(ndk, "llvm-ar")?;
        let status = std::process::Command::new("cargo")
            .current_dir(&repo)
            .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", &clang)
            .env("CC_aarch64_linux_android", &clang)
            .env("AR_aarch64_linux_android", &ar)
            .env(
                "CFLAGS_aarch64_linux_android",
                "--target=aarch64-linux-android26",
            )
            .args([
                "build",
                "-p",
                "ptts-ffi",
                "--release",
                "--target",
                "aarch64-linux-android",
            ])
            .status()
            .map_err(|err| format!("cargo build phonon: {err}"))?;
        if !status.success() {
            return Err("could not build the Phonon runtime for Android".into());
        }
    }
    if !marker.is_file() {
        return Err("Phonon build did not produce libptts_ffi.so".into());
    }
    println!("cargo:rerun-if-changed={}", marker.display());
    Ok(marker)
}

fn prepare_sherpa(cache: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dest = cache.join(format!("sherpa-onnx-{SHERPA_TAG}"));
    let marker = dest.join("libsherpa-onnx-c-api.so");
    let header = dest.join("include/c-api.h");
    let archive = cache.join(format!("sherpa-onnx-{SHERPA_TAG}-android.tar.bz2"));
    let archive_url = format!(
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/{SHERPA_TAG}/sherpa-onnx-{SHERPA_TAG}-android.tar.bz2"
    );
    ensure_pinned(&archive_url, &archive, SHERPA_ARCHIVE_SHA256)?;
    if marker.is_file() && header.is_file() {
        let header_url = format!(
            "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/{SHERPA_TAG}/sherpa-onnx/c-api/c-api.h"
        );
        ensure_pinned(&header_url, &header, SHERPA_HEADER_SHA256)?;
        return Ok(dest);
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .args([
            "-xjf",
            archive.to_str().unwrap_or(""),
            "-C",
            dest.to_str().unwrap_or(""),
            "./jniLibs/arm64-v8a",
        ])
        .status()
        .map_err(|e| format!("tar sherpa: {e}"))?;
    if !status.success() {
        return Err("could not unpack sherpa-onnx android libraries".into());
    }
    let extracted = dest.join("jniLibs/arm64-v8a");
    copy_file(&extracted.join("libsherpa-onnx-c-api.so"), &marker)?;
    copy_file(
        &extracted.join("libonnxruntime.so"),
        &dest.join("libonnxruntime.so"),
    )?;
    std::fs::create_dir_all(dest.join("include")).map_err(|e| e.to_string())?;
    let header_url = format!(
        "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/{SHERPA_TAG}/sherpa-onnx/c-api/c-api.h"
    );
    ensure_pinned(&header_url, &header, SHERPA_HEADER_SHA256)?;
    Ok(dest)
}

fn prepare_llama(
    cache: &std::path::Path,
    ndk: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let dest = cache.join(format!("llama.cpp-{LLAMA_TAG}"));
    let stamp = dest.join("build/built.stamp");
    let archive = cache.join(format!("llama.cpp-{LLAMA_TAG}.tar.gz"));
    let url = format!("https://github.com/ggml-org/llama.cpp/archive/refs/tags/{LLAMA_TAG}.tar.gz");
    ensure_pinned(&url, &archive, LLAMA_ARCHIVE_SHA256)?;
    if stamp.is_file() && dest.join("CMakeLists.txt").is_file() {
        return Ok(dest);
    }
    if !dest.join("CMakeLists.txt").is_file() {
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let status = std::process::Command::new("tar")
            .args([
                "-xzf",
                archive.to_str().unwrap_or(""),
                "-C",
                dest.to_str().unwrap_or(""),
                "--strip-components=1",
            ])
            .status()
            .map_err(|e| format!("tar llama: {e}"))?;
        if !status.success() {
            return Err("could not unpack llama.cpp".into());
        }
    }
    let toolchain = ndk.join("build/cmake/android.toolchain.cmake");
    let build_dir = dest.join("build");
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "4".into());
    let configure = std::process::Command::new("cmake")
        .arg("-S")
        .arg(&dest)
        .arg("-B")
        .arg(&build_dir)
        .arg("-G")
        .arg("Ninja")
        .arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()))
        .arg("-DANDROID_ABI=arm64-v8a")
        .arg("-DANDROID_PLATFORM=android-26")
        .arg("-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON")
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DCMAKE_POSITION_INDEPENDENT_CODE=ON")
        .arg("-DBUILD_SHARED_LIBS=OFF")
        .arg("-DGGML_NATIVE=OFF")
        .arg("-DGGML_OPENMP=OFF")
        .arg("-DGGML_LLAMAFILE=OFF")
        .arg("-DLLAMA_CURL=OFF")
        .arg("-DLLAMA_BUILD_TESTS=OFF")
        .arg("-DLLAMA_BUILD_EXAMPLES=OFF")
        .arg("-DLLAMA_BUILD_SERVER=OFF")
        .arg("-DLLAMA_BUILD_TOOLS=OFF")
        .arg("-DLLAMA_BUILD_APP=OFF")
        .arg("-DLLAMA_BUILD_COMMON=OFF")
        .status()
        .map_err(|e| format!("cmake llama: {e}"))?;
    if !configure.success() {
        return Err("cmake failed while configuring llama.cpp for arm64".into());
    }
    let built = std::process::Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("--target")
        .arg("llama")
        .arg("--parallel")
        .arg(&jobs)
        .status()
        .map_err(|e| format!("cmake build llama: {e}"))?;
    if !built.success() {
        return Err("llama.cpp failed to build for arm64".into());
    }
    std::fs::write(&stamp, LLAMA_TAG).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn collect_static_libs(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_static_libs(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("a") {
            out.push(path);
        }
    }
    Ok(())
}

fn git_head(repo: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|err| format!("sha256sum {}: {err}", path.display()))?;
    if !output.status.success() {
        return Err(format!("sha256sum failed for {}", path.display()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|hash| hash.len() == 64)
        .ok_or_else(|| format!("sha256sum returned nothing for {}", path.display()))
}

fn ensure_pinned(url: &str, dest: &std::path::Path, expected: &str) -> Result<(), String> {
    if dest.is_file() && sha256_file(dest)? == expected {
        return Ok(());
    }
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|err| err.to_string())?;
    }
    download(url, dest)?;
    let got = sha256_file(dest)?;
    if got != expected {
        let _ = std::fs::remove_file(dest);
        return Err(format!("{url} sha256 was {got}, expected {expected}"));
    }
    Ok(())
}

fn download(url: &str, dest: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("partial");
    let status = std::process::Command::new("curl")
        .args(["-fL", "--retry", "3", "--retry-delay", "2", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .map_err(|e| format!("curl {url}: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("download failed: {url}"));
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn copy_file(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if to.symlink_metadata().is_ok() {
        std::fs::remove_file(to).map_err(|e| e.to_string())?;
    }
    let bytes = std::fs::read(from).map_err(|e| format!("read {}: {e}", from.display()))?;
    if bytes.len() < 100_000
        && from.file_name().and_then(|name| name.to_str()) == Some("libc++_shared.so")
    {
        return Err(format!("{} is only {} bytes", from.display(), bytes.len()));
    }
    std::fs::write(to, &bytes).map_err(|e| format!("write {}: {e}", to.display()))?;
    Ok(())
}
