package ai.grokvoice.app

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.util.Base64
import org.json.JSONObject
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class KeyDataArgs {
  lateinit var data: String
}

@InvokeArg
class ServiceArgs {
  var title: String? = null
  var text: String? = null
}

@InvokeArg
class AlarmArgs {
  var hour: Int = 0
  var minute: Int = 0
  var label: String = ""
  var id: Int = 0
  var atMs: Long = 0
}

@InvokeArg
class TimerArgs {
  var seconds: Int = 60
  var label: String = ""
  var id: Int = 0
  var atMs: Long = 0
}

@InvokeArg
class ScheduleArgs {
  var id: Int = 0
  var kind: String = "reminder"
  var title: String = ""
  var note: String = ""
  var atMs: Long = 0
}

@InvokeArg
class CancelArgs {
  var id: Int = 0
}

@InvokeArg
class RangeArgs {
  var startMs: Long = 0
  var endMs: Long = 0
}

@InvokeArg
class EventArgs {
  var title: String = ""
  var startMs: Long = 0
  var endMs: Long = 0
  var location: String = ""
}

@InvokeArg
class EventUpdateArgs {
  var id: String = ""
  var title: String = ""
  var startMs: Long = 0
  var endMs: Long = 0
  var location: String = ""
}

@InvokeArg
class EventIdArgs {
  var id: String = ""
}

/** Native side of the Rust `native::NativeBridge` (registered from `native::init`). */
@TauriPlugin(
  permissions = [
    Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone"),
    Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications"),
    Permission(
      strings = [Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR],
      alias = "calendar"
    )
  ]
)
class VoicePlugin(private val activity: Activity) : Plugin(activity) {

  private fun b64(bytes: ByteArray): String = Base64.encodeToString(bytes, Base64.NO_WRAP)

  @Command
  fun wrapKey(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(KeyDataArgs::class.java)
      val raw = Base64.decode(args.data, Base64.NO_WRAP)
      val wrapped = KeystoreVault.wrap(raw)
      raw.fill(0)
      val ret = JSObject()
      ret.put("data", b64(wrapped))
      ret.put("securityLevel", KeystoreVault.securityLevel())
      invoke.resolve(ret)
    } catch (e: Exception) {
      invoke.reject(e.message ?: "Keystore wrap failed", e)
    }
  }

  @Command
  fun unwrapKey(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(KeyDataArgs::class.java)
      val raw = KeystoreVault.unwrap(Base64.decode(args.data, Base64.NO_WRAP))
      val ret = JSObject()
      ret.put("data", b64(raw))
      raw.fill(0)
      invoke.resolve(ret)
    } catch (e: Exception) {
      invoke.reject(e.message ?: "Keystore unwrap failed", e)
    }
  }

  @Command
  fun keySecurityLevel(invoke: Invoke) {
    val ret = JSObject()
    ret.put("securityLevel", KeystoreVault.securityLevel())
    invoke.resolve(ret)
  }

  @Command
  fun startListeningService(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ServiceArgs::class.java)
      val intent = Intent(activity, VoiceListenerService::class.java)
        .setAction(VoiceListenerService.ACTION_START)
        .putExtra(VoiceListenerService.EXTRA_TITLE, args.title)
        .putExtra(VoiceListenerService.EXTRA_TEXT, args.text)
      ContextCompat.startForegroundService(activity, intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: "could not start foreground service", e)
    }
  }

  @Command
  fun stopListeningService(invoke: Invoke) {
    activity.stopService(Intent(activity, VoiceListenerService::class.java))
    invoke.resolve()
  }

  @Command
  fun setAlarm(invoke: Invoke) {
    val a = invoke.parseArgs(AlarmArgs::class.java)
    resolveJson(invoke, PhoneActions.setAlarm(activity, a.hour, a.minute, a.label, a.id, a.atMs))
  }

  @Command
  fun setTimer(invoke: Invoke) {
    val a = invoke.parseArgs(TimerArgs::class.java)
    resolveJson(invoke, PhoneActions.setTimer(activity, a.seconds, a.label, a.id, a.atMs))
  }

  @Command
  fun scheduleItem(invoke: Invoke) {
    val a = invoke.parseArgs(ScheduleArgs::class.java)
    resolveJson(invoke, PhoneActions.schedule(activity.applicationContext, a.id, a.kind, a.title, a.note, a.atMs))
  }

  @Command
  fun cancelItem(invoke: Invoke) {
    val a = invoke.parseArgs(CancelArgs::class.java)
    PhoneActions.cancel(activity.applicationContext, a.id)
    resolveJson(invoke, """{"ok":true}""")
  }

  @Command
  fun listCalendarEvents(invoke: Invoke) {
    try {
      val a = invoke.parseArgs(RangeArgs::class.java)
      resolveJson(invoke, PhoneActions.listEvents(activity, a.startMs, a.endMs))
    } catch (e: SecurityException) {
      resolveJson(invoke, """{"ok":false,"error":"Calendar permission is not granted."}""")
    } catch (e: Exception) {
      resolveJson(invoke, JSONObject().put("ok", false).put("error", e.message ?: "calendar read failed").toString())
    }
  }

  @Command
  fun createCalendarEvent(invoke: Invoke) {
    try {
      val a = invoke.parseArgs(EventArgs::class.java)
      resolveJson(invoke, PhoneActions.createEvent(activity, a.title, a.startMs, a.endMs, a.location))
    } catch (e: SecurityException) {
      resolveJson(invoke, """{"ok":false,"error":"Calendar permission is not granted."}""")
    } catch (e: Exception) {
      resolveJson(invoke, JSONObject().put("ok", false).put("error", e.message ?: "calendar write failed").toString())
    }
  }

  @Command
  fun updateCalendarEvent(invoke: Invoke) {
    try {
      val a = invoke.parseArgs(EventUpdateArgs::class.java)
      resolveJson(invoke, PhoneActions.updateEvent(activity, a.id, a.title, a.startMs, a.endMs, a.location))
    } catch (e: SecurityException) {
      resolveJson(invoke, """{"ok":false,"error":"Calendar permission is not granted."}""")
    } catch (e: Exception) {
      resolveJson(invoke, JSONObject().put("ok", false).put("error", e.message ?: "calendar update failed").toString())
    }
  }

  @Command
  fun deleteCalendarEvent(invoke: Invoke) {
    try {
      val a = invoke.parseArgs(EventIdArgs::class.java)
      resolveJson(invoke, PhoneActions.deleteEvent(activity, a.id))
    } catch (e: SecurityException) {
      resolveJson(invoke, """{"ok":false,"error":"Calendar permission is not granted."}""")
    } catch (e: Exception) {
      resolveJson(invoke, JSONObject().put("ok", false).put("error", e.message ?: "calendar delete failed").toString())
    }
  }

  private fun resolveJson(invoke: Invoke, json: String) {
    val ret = JSObject()
    ret.put("json", json)
    invoke.resolve(ret)
  }
}
