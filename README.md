# Grok Voice

**Website:** [gabo-tech.github.io/grok-voice-android](https://gabo-tech.github.io/grok-voice-android/)

A bring-your-own-key voice assistant for Android. The phone talks to the [xAI Realtime API](https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech) directly. The wake word stays on the device. Speech, transcripts, and search are sent to xAI under your key.

The current sideload build is [Grok Voice 0.1.44](https://github.com/Gabo-Tech/grok-voice-android/releases/latest). It is an arm64 APK for Android 8 and newer, signed for sideloading.

## Install

Download `GrokVoice-0.1.44.apk` from the [latest release](https://github.com/Gabo-Tech/grok-voice-android/releases/latest), then either open it on the phone or install it from a computer:

```bash
adb install -r GrokVoice-0.1.44.apk
```

Allow installs from unknown sources if Android asks. This package is signed with a debug key so it can be installed by hand. It is not a Play Store upload.

## First launch

The first run is a five-step tour.

1. **Hello.** What the app is.
2. **Key.** Paste an xAI API key from [console.x.ai](https://console.x.ai/team/default/api-keys). The app checks the key, then stores it in an on-device vault.
3. **Microphone.** Allow the microphone and notifications.
4. **Wake word.** Record yourself saying “Hey Grok” three times, or skip and train it later.
5. **Ready.** How to talk, and a reminder to use headphones the first time.

You can replay the tour from **Debug → Replay setup tour**.

## Talking

Open **Talk** and tap the square, or say “Hey Grok” when hands-free is on. The transcript stays on screen while the session is running. Tap the square again to hang up.

**Hands-free** keeps listening after you leave the app. A notification means the microphone is still open. **Stop** on that notification ends it. Android may ask you to allow unrestricted battery use; without that, some phones kill the microphone later.

Use headphones the first few times. On a loudspeaker the microphone hears the reply and the model answers itself. Echo guard mutes the uplink while Grok is speaking. Turn it off in **Debug** if you want to interrupt.

## What you can ask

During a session the assistant can act on the phone. It calls the tool, then tells you what happened. The full voice-command guide, in the five languages the app speaks, is on the site: [English](docs/voice/en.html), [Spanish](docs/voice/es.html), [German](docs/voice/de.html), [Portuguese](docs/voice/pt.html), and [Valencian](docs/voice/ca.html).

| Ask | What happens |
| --- | --- |
| “Make a note: buy milk” | Saved on the phone. |
| “Set a timer for 10 minutes” | Saved on the Time screen. Grok Voice rings it. |
| “Set an alarm for 7:30” | Saved on the Time screen for the next 7:30. Grok Voice rings it. An alarm can keep the phone sound, or use an audio file you pick or a YouTube video. The app does not download that video. |
| “I spent 12 euros on coffee” | Logged on the Finance tab. Money stays on this phone. There is no bank login, and Talk does not invent a balance or an exchange rate. |
| “Play some music” | Plays with the default music player in Settings. Files on this phone stay the default until you pick Musify, YouTube Music, YouTube, or ask each time. Saying “on YouTube” still waits for a yes. |
| “Remind me in 5 minutes” | An in-app reminder five minutes from now, instead of a phone Clock countdown. |
| “Remind me tomorrow at 9 to call Sam” | A notification at that local time. |
| “What’s on my calendar today?” | Reads the phone calendar. Android asks for calendar access the first time. |
| “Add lunch with Ana tomorrow at 1” | Saved on this app's Calendar. If you omit the end time, it lasts one hour. |
| “Call Mom” | Looks up that People nickname. A preferred channel wins; otherwise the dialer opens after you say yes. The call is not placed for you. |
| “Text Sam that I’m late” | Opens the text app with the draft. You tap Send. |
| “Message Ana” | If Ana’s preferred channel is WhatsApp, Signal, text, or email, that app opens after you confirm. |
| “What’s Ana’s preferred channel?” | Spoken from People. WhatsApp, the dialer, and the other apps stay closed. |
| “When’s Sam’s birthday?” | Reads the date saved on that person. |
| “Remind me every year for Mom’s birthday on March 3” | A yearly notification on this phone. It stays on the person all year and shows under Time once the heads-up day arrives. |
| “Remind me every weekday at 9 to take meds” | A repeating reminder. One-off reminders still work. |
| “Snooze that 10 minutes” | Delays the reminder that just rang, or asks which one. |

**People** is under **Settings**. It is an overlay on this phone, not a second contacts app. Phone numbers and email still come from Android contacts. Nicknames, aliases, a favorite, a note, a preferred channel (call, text, WhatsApp, Signal, or email), and dated reminders (birthday, anniversary, or a custom date) stay in Grok Voice. Unlink or delete removes only that overlay. Asking for a fact answers in the conversation. A call, text, or chat still waits for a clear yes, and nothing is sent until you confirm in that app.

The bottom bar is **Talk**, **Notes**, **Calendar**, **Time**, **Finance**, and **Settings**. Back and add sit in that bar when a screen needs them. Lists stay quiet by default; tap **+** to add, or open a row to edit. Voice still works for create, edit, and delete (“change that note”, “delete the lunch event”, “move my reminder to 5”). Calendar access is requested from the Calendar tab or the first time Talk reads the phone calendar. If the phone has no writable calendar account, an event added on the Calendar screen is saved in the app instead.

0.1.44 makes expenses and incomes fully editable, including amounts. Coming expenses and Coming income are limited to the next 30 days. Credit cards are payment methods apart from accounts. A charge raises card debt and hits this month's result without moving the bank. Paying the card from a funding account is a debt payment, not an expense, and does not change net worth. Talk has voice parity for Finance: accounts, investments, cash, debts, cards, expenses, incomes, charges, and pay-card. A whole-app translation audit covers English, Spanish, German, Portuguese, and Valencian (en, es, de, pt, ca-ES-valencia), including calendar titles that were previously hardcoded. 0.1.43 makes Finance phone-first. Filters are a scrolling date-range row plus a Filters sheet, and the action bar sits above the Finance tabs so Program a bill is not covered. The same clipping pattern is tightened elsewhere. Live daily FX uses ordered multi-provider failover. api.frankfurter.app now 301s to api.frankfurter.dev, and the old redirect allowlist refused the new host, so every fiat fetch failed. Fiat now tries Frankfurter on api.frankfurter.dev, then open.er-api.com, ECB, and currency-api on jsDelivr and pages.dev. Crypto tries CoinGecko, Coinbase, Kraken, then currency-api. One book is kept per local day. Settings Refresh forces a fetch. Manual rates in Settings are the last resort. There are no invented rates, no 1:1, and no silent USD fallback. Unpaid scheduled expenses, payments, income, and investment payouts appear as all-day items on the in-app Calendar on each due date, for example rent on the 20th. Existing Time reminders and Spoken alerts are unchanged. 0.1.42 opens Finance on net worth, then this month's expenses, then income, then the month's result. A thumb-zone tab bar lists Overview, Expenses, Income, Accounts, and Charts. The rate book is only in Settings. Live Frankfurter and CoinGecko rates still load in the background as in 0.1.41. 0.1.41 uses today's live exchange rate, with manual rates in Settings. Finance fetches Frankfurter and CoinGecko when the screen opens, when the local day changes, and when a needed pair is missing. A rate fetched today wins, then a labeled stale cache, then a Settings manual rate unless that rate is marked as an override. Overview sections share one column width. 0.1.40 shows combined Finance totals in the Settings currency on Overview, charts, the PDF, and voice. A missing rate asks for a real rate instead of leading with a per-currency stack, and native amounts stay on each item. Themes add Midnight, Carbon, Ivory, Sand, Forest, Plum, and a custom palette. Home-screen widgets follow the theme and show those combined totals. 0.1.39 adds Finance schedules for one-off and recurring income and expenses, including fixed-term runs such as a 9-month 270 EUR course, open-ended rent with no lifetime total, subscriptions, planned income, and investment payouts such as Nitroeggs with a quoted value settled in another currency. Overview is five cards: net worth right now, this month's cash flow, Coming expenses, Coming income, and If these land. Display-only multi-currency conversion uses Frankfurter, CoinGecko, cached, or manual rates; native amounts stay, and a missing rate omits the combined total. Next 30 days is its own panel of planned in and out, with overdue listed separately. Missing Spanish, German, Portuguese, and Valencian UI, PDF, and voice strings are filled in, PDF accents are distinct, and phone layout fixes keep Finance panels from scrolling sideways, wrap the program sheet, and label reminders for one day and a week. 0.1.38 adds an app-wide magazine look, Graphite, Ink, Stone, and System themes, a keyboard inset under the focused input, a default currency in reorganized Settings, and Finance cash-flow, upcoming bills, and debt payoff. 0.1.37 adds separate Finance accounts, phone charts, a PDF summary, and payment reminders. 0.1.36 adds a default music player, Musify voice control, and searchable Settings. 0.1.35 adds an on-device Finance tab and Talk money tools, optional Spanish speech-to-text and voice packs, LFM2.5 350M for Spanish and English on-device Talk, local music and playlists from Talk, and a custom audio file or YouTube sound on in-app alarms. Screen copy is complete in Spanish, German, Portuguese, and Valencian, and the phone layout was reviewed for one-handed use. 0.1.34 fixes the blank screen after launch for already-onboarded users (the 0.1.33 hook crash). In 0.1.33, Settings lists all 28 Grok voices and the 10 OpenAI Realtime voices, and each one has a preview. System back walks in-app screens instead of leaving on the first press. Back and add sit in the bottom bar. A wake-word watchdog restarts listening if the microphone stalls, and Settings shows the background-listening status. Spoken mode still talks when a Grok Voice alert fires. The banner posts on a silent channel, so the normal chime no longer plays over the voice. If a sound still plays, speech waits for it. The spoken voice plays at full level on the alarm stream, and the phone's alarm volume is the only ceiling. Missed speaks are logged under `GrokVoice`. **Settings → Notification alerts** has an optional name. With a name saved, reminders are spoken naturally in the second person, for example “Hey Gabriel, remember to brush your teeth.” An empty name keeps a plain “Hey.” Normal mode is unchanged and remains the default. Reminders, alarms, countdowns, and calendar events created in Talk stay inside Grok Voice, on Time and Calendar, and fire through this app's notifications. That in-app schedule shipped in 0.1.30. Times you say are local. “Remind me tomorrow at 3” and “remind me in 5 minutes” are reminders in this app. A relative phrase such as “in 5 minutes” creates an in-app reminder instead of a phone Clock countdown. “Set an alarm for 7:30” is an alarm in this app for the next time that hour and minute occurs. “Add lunch tomorrow at 1” is an event on this app's Calendar. Talk does not write these creates to the phone Clock, and Talk-created events go to the app calendar. Reading the phone calendar is unchanged. An event added from the Calendar screen, while calendar access is granted, still goes to the phone calendar. Daily and weekly reminders, including custom weekdays, keep their next time on the Time tab. Snooze moves that notification; a repeating series still fires on its usual schedule. Person-date visibility is unchanged: a date stays on the person all year and shows under Time once the heads-up day arrives. **Settings → Notification alerts** stays Normal unless you opt in to Spoken.

**Settings → Daily voice budget** is off until you pick a number of minutes. Around that amount, the app shows a cue once per day and can say it during a call. The call is not cut off unless you also turn on a hard stop. The count is minutes of realtime sessions on this phone and resets at local midnight.

**Settings → Backup** writes an encrypted file (AES-256-GCM, passphrase via PBKDF2) of notes, People, and Finance, including person dates. Share or save it from the sheet. Restore replaces those stores after you review the file and confirm. An older backup without Finance leaves the money already on the phone. Voice keys stay in the vault unless you include them on export and again on restore. There is no cloud sync.

## How it is put together

- **UI.** React, Vite, and Tailwind. The screen is flat grayscale.
- **Audio.** A Rust thread reads the microphone. On Android that capture is an `AudioRecord` owned by the microphone foreground service, so it keeps running with the screen off or after the app is swiped away. `cpal` is the desktop capture and the speaker. Uplink is 16 kHz PCM16. Downlink is 24 kHz PCM16.
- **Session.** Each call mints a short-lived client secret with `POST https://api.x.ai/v1/realtime/client_secrets`, then connects to `wss://api.x.ai/v1/realtime?model=grok-voice-latest` with `Authorization: Bearer <ephemeral>`. The long-lived API key is not sent on the WebSocket.
- **Vault.** The key lives in a [Stronghold](https://github.com/iotaledger/stronghold.rs) snapshot. On Android the snapshot key is wrapped by a non-exportable Android Keystore key (StrongBox, then TEE).
- **Wake word.** [rustpotter](https://github.com/GiviMAD/rustpotter), trained from the three recordings on the phone. The samples are not uploaded.
- **Background.** The microphone foreground service records, holds a partial wake lock, and keeps the process up when the activity is destroyed. Leaving with the back gesture, locking the screen, or clearing recents does not stop hands-free. A wake-word watchdog restarts listening if the microphone stalls, and Settings shows the background-listening status. Listen off, Stop on the notification, or a force-stop does. Open the app once after a reboot before background listening can use the microphone. A force-stop stays off until the app is opened again.
- **Phone tools.** Notes, reminders, alarms, timers, calendar events, and Finance Talk creates are stored in the app. Schedules use `AlarmManager`, so a spoken notification can read them. Finance has no bank login. Balances are what you last said, kept per currency with no invented exchange rate. Overview shows that picture in phone-sized charts. A PDF of the same summary is written on the phone and opened in the share sheet. A payment or payday can remind you in this app two days before and again on the day. Talk does not write those dates to the phone Clock. Talk plays with the default music player in Settings. Files on this phone stay the default. Musify, when it is installed, can be chosen and then searched by voice; pause and next use its playback session when that session is visible. If Musify cannot take the request, YouTube opens, then the YouTube site in the browser. The app does not download that audio. Saying “on YouTube” still waits for a yes and opens a search, which does not start playing by itself. An explicit local request still plays songs and playlists already on the phone. An in-app alarm can ring the phone sound, an audio file you pick, or a YouTube video, and the app does not download that video. People preferences are stored in the app. Phone numbers and email are read from `ContactsContract` and are not written back. Reminders, including repeating ones and person dates, are notifications. Reading the phone calendar, and an event you add on the Calendar screen while access is granted, uses `CalendarContract`. A call opens the dialer, and a text, WhatsApp, Signal, or email opens a draft. Nothing is sent until you confirm in that app. Questions about a person are answered in the session. Backups are encrypted on the phone and do not include vault keys unless you opt in.

xAI does not offer a public third-party OAuth flow for this API, so sign-in is the API key you paste.

## On device

Settings → Voice keys → **On device** is a full Talk path that does not call xAI for speech. It is utterance mode: the phone waits until you pause, then answers. It is not the full-duplex barge-in of `grok-voice-latest`.

The stack is Moonshine Base English (int8) for speech to text, a GGUF chat model for the reply and tool call, and Piper `en_US-lessac-low` for the voice. SmolLM2 135M Instruct Q8_0 stays the default. LFM2.5 350M Q8_0 is the optional compact Spanish and English chat model. Gemma 3 1B Instruct Q4_K_M, Qwen2.5 0.5B Instruct Q8_0, and Llama 3.2 1B Instruct Q4_K_M stay optional. You can also pick a `.gguf` already on the phone, or paste an https link (optional SHA-256). The file is only loaded as the chat model. Nothing new downloads until you tap Download, including after an upgrade. German uses a separate NeMo FastConformer download and Piper `de_DE-thorsten-low`. Spanish uses the same kind of optional pack: NeMo FastConformer `stt_es_fastconformer_hybrid_large_pc` (int8 transducer, CC-BY-4.0) and Piper `es_ES-davefx-medium` (the voice dataset is CC0). sherpa-onnx does not publish a Spanish Moonshine model or an es_ES Piper low voice, so Spanish speech uses the same NeMo transducer loader as the German pack, and the voice is the medium davefx model. It uses the same VITS loader, noise settings, and espeak data as the English and German voices. Choose it under Settings → Language → Local speech. That choice is separate from the screen language. Spoken alerts use the Piper pack for the screen language when it is installed, and Android text-to-speech when it is not. Portuguese and Valencian stay on Android text-to-speech. sherpa-onnx 1.13.8 runs the speech models. llama.cpp `b12140` runs the GGUF. The reply is ordinary speech, and a JSON object in that reply can call a phone tool. A grammar is not applied, so a tokenizer piece cannot abort the process. Generation stops when that JSON object closes. Phone tools (notes, reminders, calendar, People, and the confirm-before-open gates) go through the same action path as the realtime session. Android system speech is only a fallback if Piper cannot speak.

The weights are not in the APK. Download them on the phone from Voice keys, or fetch them yourself. A phone that already has Moonshine Tiny or SmolLM2 360M Q8_0 needs these two packs again. The Piper voices are unchanged. A larger language model is loaded into memory while Talk uses it, so it needs free storage and about that much free memory on top of speech and voice.

| Pack | Download | What it is |
| --- | --- | --- |
| Speech to text | 239 MB | [sherpa-onnx-moonshine-base-en-int8.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-int8.tar.bz2) (about 275 MB unpacked), sha256 `21870cecaa2e44e4e2bf63e02d1072bed183ccd10284871353bd9d24dad14e5e` |
| Language model (default) | 138 MB | [SmolLM2-135M-Instruct-Q8_0.gguf](https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF/resolve/main/SmolLM2-135M-Instruct-Q8_0.gguf), sha256 `5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83` |
| Gemma 3 1B Instruct, Q4_K_M | 769 MB | [gemma-3-1b-it-Q4_K_M.gguf](https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/resolve/main/gemma-3-1b-it-Q4_K_M.gguf), sha256 `8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135` |
| Qwen2.5 0.5B Instruct, Q8_0 | 644 MB | [qwen2.5-0.5b-instruct-q8_0.gguf](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q8_0.gguf), sha256 `ca59ca7f13d0e15a8cfa77bd17e65d24f6844b554a7b6c12e07a5f89ff76844e` |
| Llama 3.2 1B Instruct, Q4_K_M | 770 MB | [Llama-3.2-1B-Instruct-Q4_K_M.gguf](https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf), sha256 `6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83` |
| LFM2.5 350M, Q8_0 | 362 MB | [LFM2.5-350M-Q8_0.gguf](https://huggingface.co/LiquidAI/LFM2.5-350M-GGUF/resolve/main/LFM2.5-350M-Q8_0.gguf), sha256 `be036a757295e550098b85e13f6af2735d0fa73b41e1156a40c7d8e8e32a5766` |
| Voice | 64 MB | [vits-piper-en_US-lessac-low.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-low.tar.bz2) (about 77 MB unpacked) |
| Spanish speech to text | 102 MB | [sherpa-onnx-nemo-fast-conformer-transducer-es-1424-int8.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-fast-conformer-transducer-es-1424-int8.tar.bz2) (about 130 MB unpacked), sha256 `d6d29ffd560c366da1b177b71e79d8733a9a9910dd1ace282d9bfae94e056210` |
| Spanish voice | 64 MB | [vits-piper-es_ES-davefx-medium.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-es_ES-davefx-medium.tar.bz2) (about 77 MB unpacked), sha256 `a3f6beb54a9cb893279f72978a22f807a4d9fc9c7848157b524d5cc7b7f58b22` |

Put On device first in the key list. If the packs are missing, or the model cannot load (the phone wants about 1.4 GB free; 6–8 GB devices are the target), Talk tries the next saved key.

Web search in this mode does not use xAI. It reads DuckDuckGo. Offline, the tool returns “Web search needs a network connection, and this phone is offline.” so the model can say that.

Hands-free “Hey Grok” uses a microphone foreground service and its notification. Leaving Talk, switching apps, locking the screen, or clearing the app from recents does not stop that listener and does not delete the enrollment. It stops when Listen is turned off (including Stop on the notification), when the app is force-stopped, or when Android kills the process because the phone is extremely low on memory. After a force-stop or that kind of kill, open the app and turn Listen on again.

Limits for this version: English, Spanish, and German recognition from downloaded packs, one voice per language, a small conversational model that is less capable than `grok-voice-latest`, and no true barge-in. The Android build downloads the sherpa-onnx arm64 libraries and compiles llama.cpp. It does not download the model packs.

## Build from source

You need Node, pnpm, Rust, a JDK, and the Android SDK with NDK `27.1.12297006`.

```bash
pnpm install
pnpm tauri dev
```

That runs the desktop shell, which is enough to work on the UI. Phone alarms, the calendar, the keystore, and the foreground service run on Android.

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$HOME/Android/Sdk/ndk/27.1.12297006"
export NDK_HOME="$ANDROID_NDK_HOME"

pnpm tauri android dev
pnpm tauri android build --target aarch64 --apk
```

The release APK is written to:

```text
src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

`pnpm tauri android init` is only needed when `src-tauri/gen/android` is missing. This repository already contains that project, including the voice service, calendar permissions, and launcher icon. Do not re-init over it.

Minimum SDK is 26. The release native library is arm64.

## License

[MIT](LICENSE).
