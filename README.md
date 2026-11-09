# Grok Voice

**Website:** [gabo-tech.github.io/grok-voice-android](https://gabo-tech.github.io/grok-voice-android/)

A bring-your-own-key voice assistant for Android. The phone talks to the [xAI Realtime API](https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech) directly. The wake word stays on the device. Speech, transcripts, and search are sent to xAI under your key.

The current sideload build is [Grok Voice 0.1.29](https://github.com/Gabo-Tech/grok-voice-android/releases/latest). It is an arm64 APK for Android 8 and newer, signed for sideloading.

## Install

Download `GrokVoice-0.1.29.apk` from the [latest release](https://github.com/Gabo-Tech/grok-voice-android/releases/latest), then either open it on the phone or install it from a computer:

```bash
adb install -r GrokVoice-0.1.29.apk
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

During a session the assistant can act on the phone. It calls the tool, then tells you what happened.

| Ask | What happens |
| --- | --- |
| “Make a note: buy milk” | Saved on the phone. |
| “Set a timer for 10 minutes” | Saved on the Time screen. Grok Voice rings it. |
| “Set an alarm for 7:30” | Saved on the Time screen for the next 7:30. Grok Voice rings it. |
| “Remind me in 5 minutes” | A reminder in this app, firing five minutes from now. |
| “Remind me tomorrow at 9 to call Sam” | A notification at that local time. |
| “What’s on my calendar today?” | Reads the phone calendar. Android asks for calendar access the first time. |
| “Add lunch with Ana tomorrow at 1” | Writes an event. If you omit the end time, it lasts one hour. |
| “Call Mom” | Looks up that People nickname. A preferred channel wins; otherwise the dialer opens after you say yes. The call is not placed for you. |
| “Text Sam that I’m late” | Opens the text app with the draft. You tap Send. |
| “Message Ana” | If Ana’s preferred channel is WhatsApp, Signal, text, or email, that app opens after you confirm. |
| “What’s Ana’s preferred channel?” | Spoken from People. WhatsApp, the dialer, and the other apps stay closed. |
| “When’s Sam’s birthday?” | Reads the date saved on that person. |
| “Remind me every year for Mom’s birthday on March 3” | A yearly notification on this phone. It stays on the person all year and shows under Time once the heads-up day arrives. |
| “Remind me every weekday at 9 to take meds” | A repeating reminder. One-off reminders still work. |
| “Snooze that 10 minutes” | Delays the reminder that just rang, or asks which one. |

**People** is under **Settings**. It is an overlay on this phone, not a second contacts app. Phone numbers and email still come from Android contacts. Nicknames, aliases, a favorite, a note, a preferred channel (call, text, WhatsApp, Signal, or email), and dated reminders (birthday, anniversary, or a custom date) stay in Grok Voice. Unlink or delete removes only that overlay. Asking for a fact answers in the conversation. A call, text, or chat still waits for a clear yes, and nothing is sent until you confirm in that app.

The bottom bar is **Talk**, **Notes**, **Calendar**, **Remind**, and **More**. Lists stay quiet by default; tap **+** to add, or open a row to edit. Voice still works for create, edit, and delete (“change that note”, “delete the lunch event”, “move my reminder to 5”). Calendar access is requested from the Calendar tab or the first voice calendar ask. If the phone has no writable calendar account, events are saved in the app instead.

Times you say are local. “Remind me tomorrow at 3” and “remind me in 5 minutes” are reminders in this app. “Set an alarm for 7:30” is an alarm in this app for the next time that hour and minute occurs. “Add lunch tomorrow at 1” is an event in this app's Calendar. Daily and weekly reminders, including custom weekdays, keep their next time on the Time tab. Snooze moves that notification; a repeating series still fires on its usual schedule.

**Settings → Daily voice budget** is off until you pick a number of minutes. Around that amount, the app shows a cue once per day and can say it during a call. The call is not cut off unless you also turn on a hard stop. The count is minutes of realtime sessions on this phone and resets at local midnight.

**Settings → Backup** writes an encrypted file (AES-256-GCM, passphrase via PBKDF2) of notes and People, including person dates. Share or save it from the sheet. Restore replaces those two stores after you review the file and confirm. Voice keys stay in the vault unless you include them on export and again on restore. There is no cloud sync.

## How it is put together

- **UI.** React, Vite, and Tailwind. The screen is flat grayscale.
- **Audio.** A Rust thread reads the microphone. On Android that capture is an `AudioRecord` owned by the microphone foreground service, so it keeps running with the screen off or after the app is swiped away. `cpal` is the desktop capture and the speaker. Uplink is 16 kHz PCM16. Downlink is 24 kHz PCM16.
- **Session.** Each call mints a short-lived client secret with `POST https://api.x.ai/v1/realtime/client_secrets`, then connects to `wss://api.x.ai/v1/realtime?model=grok-voice-latest` with `Authorization: Bearer <ephemeral>`. The long-lived API key is not sent on the WebSocket.
- **Vault.** The key lives in a [Stronghold](https://github.com/iotaledger/stronghold.rs) snapshot. On Android the snapshot key is wrapped by a non-exportable Android Keystore key (StrongBox, then TEE).
- **Wake word.** [rustpotter](https://github.com/GiviMAD/rustpotter), trained from the three recordings on the phone. The samples are not uploaded.
- **Background.** The microphone foreground service records, holds a partial wake lock, and keeps the process up when the activity is destroyed. Leaving with the back gesture, locking the screen, or clearing recents does not stop hands-free. Listen off, Stop on the notification, or a force-stop does.
- **Phone tools.** Notes, reminders, alarms, timers, and calendar events Talk creates are stored in the app and scheduled with `AlarmManager`, so a spoken notification can read them. People preferences are stored in the app. Phone numbers and email are read from `ContactsContract` and are not written back. Reminders, including repeating ones and person dates, are notifications. Reading the phone calendar, and an event you add on the Calendar screen while access is granted, uses `CalendarContract`. A call opens the dialer, and a text, WhatsApp, Signal, or email opens a draft. Nothing is sent until you confirm in that app. Questions about a person are answered in the session. Backups are encrypted on the phone and do not include vault keys unless you opt in.

xAI does not offer a public third-party OAuth flow for this API, so sign-in is the API key you paste.

## On device

Settings → Voice keys → **On device** is a full Talk path that does not call xAI for speech. It is utterance mode: the phone waits until you pause, then answers. It is not the full-duplex barge-in of `grok-voice-latest`.

The stack is Moonshine Base English (int8) for speech to text, a GGUF chat model for the reply and tool call, and Piper `en_US-lessac-low` for the voice. SmolLM2 135M Instruct Q8_0 stays the default. Gemma 3 1B Instruct Q4_K_M, Qwen2.5 0.5B Instruct Q8_0, and Llama 3.2 1B Instruct Q4_K_M are optional. You can also pick a `.gguf` already on the phone, or paste an https link (optional SHA-256). The file is only loaded as the chat model. Nothing new downloads until you tap Download, including after an upgrade. German uses a separate NeMo FastConformer download and Piper `de_DE-thorsten-low`. sherpa-onnx 1.13.8 runs the speech models. llama.cpp `b12140` runs the GGUF. The reply is ordinary speech, and a JSON object in that reply can call a phone tool. A grammar is not applied, so a tokenizer piece cannot abort the process. Generation stops when that JSON object closes. Phone tools (notes, reminders, calendar, People, and the confirm-before-open gates) go through the same action path as the realtime session. Android system speech is only a fallback if Piper cannot speak.

The weights are not in the APK. Download them on the phone from Voice keys, or fetch them yourself. A phone that already has Moonshine Tiny or SmolLM2 360M Q8_0 needs these two packs again. The Piper voices are unchanged. A larger language model is loaded into memory while Talk uses it, so it needs free storage and about that much free memory on top of speech and voice.

| Pack | Download | What it is |
| --- | --- | --- |
| Speech to text | 239 MB | [sherpa-onnx-moonshine-base-en-int8.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-int8.tar.bz2) (about 275 MB unpacked), sha256 `21870cecaa2e44e4e2bf63e02d1072bed183ccd10284871353bd9d24dad14e5e` |
| Language model (default) | 138 MB | [SmolLM2-135M-Instruct-Q8_0.gguf](https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF/resolve/main/SmolLM2-135M-Instruct-Q8_0.gguf), sha256 `5a1395716f7913741cc51d98581b9b1228d80987a9f7d3664106742eb06bba83` |
| Gemma 3 1B Instruct, Q4_K_M | 769 MB | [gemma-3-1b-it-Q4_K_M.gguf](https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/resolve/main/gemma-3-1b-it-Q4_K_M.gguf), sha256 `8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135` |
| Qwen2.5 0.5B Instruct, Q8_0 | 644 MB | [qwen2.5-0.5b-instruct-q8_0.gguf](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q8_0.gguf), sha256 `ca59ca7f13d0e15a8cfa77bd17e65d24f6844b554a7b6c12e07a5f89ff76844e` |
| Llama 3.2 1B Instruct, Q4_K_M | 770 MB | [Llama-3.2-1B-Instruct-Q4_K_M.gguf](https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf), sha256 `6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83` |
| Voice | 64 MB | [vits-piper-en_US-lessac-low.tar.bz2](https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-low.tar.bz2) (about 77 MB unpacked) |

Put On device first in the key list. If the packs are missing, or the model cannot load (the phone wants about 1.4 GB free; 6–8 GB devices are the target), Talk tries the next saved key.

Web search in this mode does not use xAI. It reads DuckDuckGo. Offline, the tool returns “Web search needs a network connection, and this phone is offline.” so the model can say that.

Hands-free “Hey Grok” uses a microphone foreground service and its notification. Leaving Talk, switching apps, locking the screen, or clearing the app from recents does not stop that listener and does not delete the enrollment. It stops when Listen is turned off (including Stop on the notification), when the app is force-stopped, or when Android kills the process because the phone is extremely low on memory. After a force-stop or that kind of kill, open the app and turn Listen on again.

Limits for this version: English and German recognition from downloaded packs, one voice per language, a small conversational model that is less capable than `grok-voice-latest`, and no true barge-in. The Android build downloads the sherpa-onnx arm64 libraries and compiles llama.cpp. It does not download the model packs.

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
