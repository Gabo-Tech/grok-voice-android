package ai.grokvoice.app

import android.content.Context
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Small on-disk ring of listener lifecycle lines. The in-memory Rust log dies with the
 * process, so a later diagnostics dump can still show why listening stopped.
 */
object ListenTrace {
  private const val PREFS = "listen_trace"
  private const val KEY_HISTORY = "history"
  private const val KEY_PENDING = "pending"
  private const val KEY_STOP = "stop"
  private const val MAX = 40

  fun event(context: Context, msg: String) {
    val line = "${stamp()} $msg"
    synchronized(this) {
      val prefs = prefs(context)
      val history = push(prefs.getString(KEY_HISTORY, "").orEmpty(), line)
      val noted = RustBridge.noteSafe(line)
      val pending = if (noted) {
        prefs.getString(KEY_PENDING, "").orEmpty()
      } else {
        push(prefs.getString(KEY_PENDING, "").orEmpty(), line)
      }
      prefs.edit().putString(KEY_HISTORY, history).putString(KEY_PENDING, pending).commit()
    }
  }

  fun stop(context: Context, reason: String) {
    synchronized(this) {
      prefs(context).edit().putString(KEY_STOP, reason).commit()
    }
    event(context, "listening stopped: $reason")
  }

  fun clearStop(context: Context) {
    synchronized(this) {
      prefs(context).edit().remove(KEY_STOP).commit()
    }
  }

  fun lastStop(context: Context): String {
    synchronized(this) {
      return prefs(context).getString(KEY_STOP, "").orEmpty()
    }
  }

  fun history(context: Context): String {
    synchronized(this) {
      return prefs(context).getString(KEY_HISTORY, "").orEmpty()
    }
  }

  fun drain(context: Context): String {
    synchronized(this) {
      val prefs = prefs(context)
      val pending = prefs.getString(KEY_PENDING, "").orEmpty()
      if (pending.isNotEmpty()) {
        prefs.edit().putString(KEY_PENDING, "").commit()
      }
      return pending
    }
  }

  private fun prefs(context: Context) =
    context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  private fun push(existing: String, line: String): String {
    val lines = existing.lineSequence().filter { it.isNotBlank() }.toMutableList()
    lines.add(line)
    while (lines.size > MAX) lines.removeAt(0)
    return lines.joinToString("\n")
  }

  private fun stamp(): String = SimpleDateFormat("HH:mm:ss", Locale.US).format(Date())
}
