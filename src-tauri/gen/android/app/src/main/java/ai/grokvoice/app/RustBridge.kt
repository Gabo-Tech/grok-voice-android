package ai.grokvoice.app

import android.util.Log

/** JNI entry points implemented in `src-tauri/src/lib.rs`. */
object RustBridge {
  @JvmStatic
  external fun stopListening()

  @JvmStatic
  external fun engineAlive(): Boolean

  @JvmStatic
  external fun ensureUi()

  @JvmStatic
  external fun feedMic(samples: ShortArray, count: Int)

  @JvmStatic
  external fun pollPresentTalk(): Boolean

  @JvmStatic
  external fun pollReopenMic(): Boolean

  @JvmStatic
  external fun note(msg: String): Boolean

  @JvmStatic
  external fun serviceFailed()

  fun stopListeningSafe() {
    try {
      stopListening()
    } catch (e: UnsatisfiedLinkError) {
      Log.w("GrokVoice", "Rust core not loaded; nothing to stop", e)
    }
  }

  fun engineAliveSafe(): Boolean {
    return try {
      System.loadLibrary("grok_voice_lib")
      engineAlive()
    } catch (e: UnsatisfiedLinkError) {
      Log.w("GrokVoice", "Rust core not loaded", e)
      false
    }
  }

  fun ensureUiSafe() {
    try {
      System.loadLibrary("grok_voice_lib")
      ensureUi()
    } catch (e: UnsatisfiedLinkError) {
      Log.w("GrokVoice", "Rust core not loaded; window stays with the activity", e)
    }
  }

  fun noteSafe(msg: String): Boolean {
    return try {
      System.loadLibrary("grok_voice_lib")
      note(msg)
    } catch (e: UnsatisfiedLinkError) {
      false
    }
  }

  fun serviceFailedSafe() {
    try {
      System.loadLibrary("grok_voice_lib")
      serviceFailed()
    } catch (e: UnsatisfiedLinkError) {
      Log.w("GrokVoice", "Rust core not loaded; listener will retry after the window opens", e)
    }
  }
}
