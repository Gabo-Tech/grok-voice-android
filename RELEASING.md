# Releasing

Signed release APKs are built in GitHub Actions, so a release does not depend on one computer. The signing key is the existing sideload key, stored in this repository's Actions secrets. The certificate stays the same, so an update installs over an existing install.

## Process

1. Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `package.json`, and update the user-facing copy that names the version. `versionName` is that version. `versionCode` is `major * 1000000 + minor * 1000 + patch`.
2. Merge the bump to `main`.
3. Tag that commit `vX.Y.Z` and push the tag.
4. The Release APK workflow builds the arm64 release with `pnpm tauri android build --target aarch64 --apk`. That build runs `build.rs`, which produces `libptts_ffi`. The workflow signs the APK with `apksigner` (v2) and checks the certificate, version, minSdk 26, the arm64-v8a ABI, and `libptts_ffi.so`. It uploads `GrokVoice-X.Y.Z.apk` to the GitHub release for that tag, and creates the release when it is missing.

A tag push does not replace an asset already on the release. Run the workflow manually with the same tag to replace `GrokVoice-X.Y.Z.apk`. Manual dry run builds, signs, and verifies, and skips the upload.

The release certificate SHA-256 is `fd400ffe61c9b18654bc40f567275e5a6e13bab8d311355f389f6e46458aafe9`. The workflow rejects an APK signed by any other certificate.

## Signing key

The key is in these repository secrets:

- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

GitHub cannot show secret values again. Keep an offline backup of the keystore. If these secrets are lost, they cannot be read back, and a newly generated key would not install over existing installs.
