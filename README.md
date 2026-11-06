# Grok Voice

**Website:** [gabo-tech.github.io/grok-voice-android](https://gabo-tech.github.io/grok-voice-android/)

A bring-your-own-key voice assistant for Android. The phone talks to the [xAI Realtime API](https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech) directly. The wake word stays on the device. Speech, transcripts, and search are sent to xAI under your key.

The current sideload build is [Grok Voice 0.1.15](https://github.com/Gabo-Tech/grok-voice-android/releases/latest). It is an arm64 APK for Android 8 and newer, signed for sideloading.

## Install

Download `GrokVoice-0.1.15.apk` from the [latest release](https://github.com/Gabo-Tech/grok-voice-android/releases/latest), then either open it on the phone or install it from a computer:

```bash
adb install -r GrokVoice-0.1.15.apk
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
| “Set a timer for 10 minutes” | Starts in the Clock app while Grok Voice is on screen. From the background, the app rings it itself. |
| “Set an alarm for 7:30” | Same as a timer: Clock app in the foreground, an in-app alarm from the background. |
| “Remind me tomorrow at 9 to call Sam” | A notification at that local time. |
| “What’s on my calendar today?” | Reads the phone calendar. Android asks for calendar access the first time. |
| “Add lunch with Ana tomorrow at 1” | Writes an event. If you omit the end time, it lasts one hour. |
| “Call Mom” | Looks up that People nickname. A preferred channel wins; otherwise the dialer opens after you say yes. The call is not placed for you. |
| “Text Sam that I’m late” | Opens the text app with the draft. You tap Send. |
| “Message Ana” | If Ana’s preferred channel is WhatsApp, Signal, text, or email, that app opens after you confirm. |
| “What’s Ana’s preferred channel?” | Spoken from People. WhatsApp, the dialer, and the other apps stay closed. |
| “When’s Sam’s birthday?” | Reads the date saved on that person. |
| “Remind me every year for Mom’s birthday on March 3” | A yearly notification on this phone, also listed under Time. |
| “Remind me every weekday at 9 to take meds” | A repeating reminder. One-off reminders still work. |
| “Snooze that 10 minutes” | Delays the reminder that just rang, or asks which one. |

**People** is under **Settings**. It is an overlay on this phone, not a second contacts app. Phone numbers and email still come from Android contacts. Nicknames, aliases, a favorite, a note, a preferred channel (call, text, WhatsApp, Signal, or email), and dated reminders (birthday, anniversary, or a custom date) stay in Grok Voice. Unlink or delete removes only that overlay. Asking for a fact answers in the conversation. A call, text, or chat still waits for a clear yes, and nothing is sent until you confirm in that app.

The bottom bar is **Talk**, **Notes**, **Calendar**, **Remind**, and **More**. Lists stay quiet by default; tap **+** to add, or open a row to edit. Voice still works for create, edit, and delete (“change that note”, “delete the lunch event”, “move my reminder to 5”). Calendar access is requested from the Calendar tab or the first voice calendar ask. If the phone has no writable calendar account, events are saved in the app instead.

Times you say are local. “Remind me tomorrow at 3” is a one-off reminder. “Set an alarm for 7:30” is a clock alarm for the next time that hour and minute occurs. Daily and weekly reminders, including custom weekdays, keep their next time on the Time tab. Snooze moves that notification; a repeating series still fires on its usual schedule.

**Settings → Daily voice budget** is off until you pick a number of minutes. Around that amount, the app shows a cue once per day and can say it during a call. The call is not cut off unless you also turn on a hard stop. The count is minutes of realtime sessions on this phone and resets at local midnight.

**Settings → Backup** writes an encrypted file (AES-256-GCM, passphrase via PBKDF2) of notes and People, including person dates. Share or save it from the sheet. Restore replaces those two stores after you review the file and confirm. Voice keys stay in the vault unless you include them on export and again on restore. There is no cloud sync.

## How it is put together

- **UI.** React, Vite, and Tailwind. The screen is flat grayscale.
- **Audio.** A Rust thread uses `cpal` (AAudio on Android). Uplink is 16 kHz PCM16. Downlink is 24 kHz PCM16.
- **Session.** Each call mints a short-lived client secret with `POST https://api.x.ai/v1/realtime/client_secrets`, then connects to `wss://api.x.ai/v1/realtime?model=grok-voice-latest` with `Authorization: Bearer <ephemeral>`. The long-lived API key is not sent on the WebSocket.
- **Vault.** The key lives in a [Stronghold](https://github.com/iotaledger/stronghold.rs) snapshot. On Android the snapshot key is wrapped by a non-exportable Android Keystore key (StrongBox, then TEE).
- **Wake word.** [rustpotter](https://github.com/GiviMAD/rustpotter), trained from the three recordings on the phone. The samples are not uploaded.
- **Background.** A microphone foreground service and a partial wake lock keep capture alive with the screen off. Leaving with the back gesture keeps the process up while hands-free is armed.
- **Phone tools.** Notes, reminders, and People preferences are stored in the app. Phone numbers and email are read from `ContactsContract` and are not written back. Alarms and timers prefer the system Clock app when the screen is open, and `AlarmManager` otherwise. Reminders, including repeating ones and person dates, are notifications. Calendar reads and writes use `CalendarContract`. A call opens the dialer, and a text, WhatsApp, Signal, or email opens a draft. Nothing is sent until you confirm in that app. Questions about a person are answered in the session. Backups are encrypted on the phone and do not include vault keys unless you opt in.

xAI does not offer a public third-party OAuth flow for this API, so sign-in is the API key you paste.

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
