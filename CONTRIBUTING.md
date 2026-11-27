# Contributing

Grok Voice is a Tauri and React app for Android. The site at [gabo-tech.github.io/grok-voice-android](https://gabo-tech.github.io/grok-voice-android/) has the [user guide](docs/guide/index.html) and the [developer notes](docs/develop.html). Those notes are the map. This file is the path for a change.

## Before you edit a screen

Read [DESIGN-LANGUAGE.md](DESIGN-LANGUAGE.md) (v2). Themes swap with `data-appearance` and may set only the semantic tokens that file lists. No raw hex in a component. Text is at least 13px and follows the Android font scale. Touch targets are at least 48px. Radius uses the v2 scale (4, 8, 12, 20). Cards, hero figures, and charts are allowed. Primary actions are solid fills. Secondary actions are underlined text. Pull request #132’s Talk bubbles, landscape rail, Activity, Sankey, and Plan stay; v2 restyles them later and does not replace them.

Copy for the interface exists in English, Spanish, German, Portuguese, and Valencian (`en`, `es`, `de`, `pt`, `ca-ES-valencia`). Apostrophes in Android `strings.xml` are written as `\'`.

## Voice and the screen share one record

Every entity the person can see must be created, read, updated, and deleted from the screen and by Talk. Talk tools are `actions::tool_definitions` in `src-tauri/src/actions.rs`, plus `finance::tool_definitions`. `remember_fact` and `forget_fact` are on-device only.

Deletes wait for a clear yes when the tool takes `confirmed`. Call it first with `confirmed` false, say what will go, then call again after a yes. Talk does not create an account, card, or other record the person did not ask for. A new field or a new kind of record ships with all four operations on both paths, in all five locales.

The spoken examples are [docs/voice](docs/voice/en.html). The tool catalog is [docs/guide/commands.html](docs/guide/commands.html). If you add a tool, update both from the definition, not from a guessed phrase.

## Checks

Translations and setup copy:

```bash
node --experimental-strip-types --test src/lib/setupGuide.test.ts src/lib/i18nCoverage.test.ts src/lib/backupRoundTrip.test.ts
```

The other `src/lib/*.test.ts` files cover finance math, colors, appearance, budgets, and time tabs. Rust tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

The pull-request job **Build and sign** (`.github/workflows/release-apk.yml`) runs the two Node files above, then builds and signs the arm64 APK. It does not upload a release. It does not run `cargo test`.

## Build

Desktop UI:

```bash
pnpm install
pnpm tauri dev
```

Android, arm64-v8a, NDK `27.1.12297006`:

```bash
pnpm tauri android build --target aarch64 --apk
```

`minSdk` is 26. The release build type in `src-tauri/gen/android/app/build.gradle.kts` uses the debug signing config, labeled there as the local sideload key. Do not run `pnpm tauri android init` over `src-tauri/gen/android`.

## Version and release

`versionCode` is `major * 1000000 + minor * 1000 + patch`. 0.1.65 is 1065.

A version bump touches `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, the `GrokVoice/` user-agent in `src-tauri/src/local/phonon.rs`, and the user-facing copy in `README.md` and `docs/index.html`. Tag `vX.Y.Z` on main. The tag workflow uploads `GrokVoice-X.Y.Z.apk`. See [RELEASING.md](RELEASING.md).

Do not commit keystores, API keys, or Hugging Face tokens.
