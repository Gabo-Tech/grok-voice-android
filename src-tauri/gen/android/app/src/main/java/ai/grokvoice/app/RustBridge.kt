package ai.grokvoice.app

import android.util.Log

/** JNI entry points implemented in `src-tauri/src/lib.rs`. */
object RustBridge {
  @JvmStatic
  external fun stopListening()

  fun stopListeningSafe() {
    try {
      stopListening()
    } catch (e: UnsatisfiedLinkError) {
      Log.w("GrokVoice", "Rust core not loaded; nothing to stop", e)
    }
  }
}
