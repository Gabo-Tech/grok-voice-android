package ai.grokvoice.app

import android.util.Log

/** JNI entry points implemented in `src-tauri/src/lib.rs`. */
object RustBridge {
  @JvmStatic
  external fun stopListening()

  @JvmStatic
  external fun engineAlive(): Boolean

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
}
