package ai.grokvoice.app

import android.Manifest
import android.app.Activity
import android.app.ActivityManager
import android.content.ClipData
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Base64
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import java.io.File
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ShareArgs {
  var path: String = ""
  var mime: String = "application/octet-stream"
  var title: String = "Grok Voice backup"
}

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
    ),
    Permission(strings = [Manifest.permission.READ_CONTACTS], alias = "contacts"),
    Permission(strings = [Manifest.permission.READ_CALL_LOG], alias = "callLog"),
    Permission(strings = [Manifest.permission.READ_SMS], alias = "sms")
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
      requestUnrestrictedBattery(activity)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: "could not start foreground service", e)
    }
  }

  private fun requestUnrestrictedBattery(activity: Activity) {
    val pm = activity.getSystemService(PowerManager::class.java) ?: return
    if (pm.isIgnoringBatteryOptimizations(activity.packageName)) return
    val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
      .setData(Uri.parse("package:${activity.packageName}"))
    try {
      activity.startActivity(intent)
    } catch (_: Exception) {
    }
  }

  @Command
  fun stopListeningService(invoke: Invoke) {
    activity.stopService(Intent(activity, VoiceListenerService::class.java))
    invoke.resolve()
  }

  @Command
  fun refreshWidgets(invoke: Invoke) {
    HomeWidgets.updateAll(activity.applicationContext)
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
  fun shareFile(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareArgs::class.java)
      val file = backupFile(args.path)
      val title = args.title.trim().filterNot { it.isISOControl() }.take(80).ifBlank { "Grok Voice backup" }
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
      val send = Intent(Intent.ACTION_SEND)
        .setType("application/octet-stream")
        .putExtra(Intent.EXTRA_STREAM, uri)
        .putExtra(Intent.EXTRA_TITLE, title)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      send.clipData = ClipData.newRawUri("", uri)
      activity.startActivity(Intent.createChooser(send, title))
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: "could not share the backup", e)
    }
  }

  private fun backupFile(path: String): File {
    val root = File(activity.cacheDir, "backups").canonicalFile
    val file = File(path).canonicalFile
    val nameOk = Regex("""grok-voice-backup-\d{4}-\d{2}-\d{2}\.gvb""").matches(file.name)
    if (!nameOk || file.parentFile?.canonicalFile != root || !file.isFile) {
      throw IllegalArgumentException("Backup file is not in the backup folder")
    }
    return file
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

  @InvokeArg
  class MediaArgs {
    var action: String = ""
  }

  @InvokeArg
  class VolumeArgs {
    var direction: String = ""
    var steps: Int = 1
  }

  @InvokeArg
  class HandOffArgs {
    var kind: String = ""
    var query: String = ""
    var url: String = ""
    var phone: String = ""
    var text: String = ""
    var to: String = ""
    var subject: String = ""
    var body: String = ""
  }

  @Command
  fun mediaControl(invoke: Invoke) {
    val a = invoke.parseArgs(MediaArgs::class.java)
    resolveJson(invoke, HandOff.media(activity.applicationContext, a.action))
  }

  @Command
  fun adjustVolume(invoke: Invoke) {
    val a = invoke.parseArgs(VolumeArgs::class.java)
    resolveJson(invoke, HandOff.volume(activity.applicationContext, a.direction, a.steps))
  }

  @InvokeArg
  class ContactArgs {
    var name: String = ""
  }

  @InvokeArg
  class ContactQueryArgs {
    var query: String = ""
  }

  @InvokeArg
  class ContactIdArgs {
    var id: String = ""
  }

  @Command
  fun lookupContacts(invoke: Invoke) {
    val a = invoke.parseArgs(ContactArgs::class.java)
    resolveJson(invoke, HandOff.contacts(activity.applicationContext, a.name))
  }

  @Command
  fun listPhoneContacts(invoke: Invoke) {
    val a = invoke.parseArgs(ContactQueryArgs::class.java)
    resolveJson(invoke, HandOff.listContacts(activity.applicationContext, a.query))
  }

  @Command
  fun contactHandles(invoke: Invoke) {
    val a = invoke.parseArgs(ContactIdArgs::class.java)
    resolveJson(invoke, HandOff.contact(activity.applicationContext, a.id))
  }

  @Command
  fun openPhoneContact(invoke: Invoke) {
    val a = invoke.parseArgs(ContactIdArgs::class.java)
    resolveJson(invoke, HandOff.viewContact(activity.applicationContext, a.id))
  }

  @Command
  fun openHandOff(invoke: Invoke) {
    val a = invoke.parseArgs(HandOffArgs::class.java)
    resolveJson(
      invoke,
      HandOff.open(activity.applicationContext, a.kind, a.query, a.url, a.phone, a.text, a.to, a.subject, a.body)
    )
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

  @InvokeArg
  class SpeakArgs {
    var text: String = ""
  }

  @Command
  fun readNotifications(invoke: Invoke) {
    resolveJson(invoke, DeviceExtras.notifications(activity.applicationContext))
  }

  @Command
  fun setDnd(invoke: Invoke) {
    val mode = invoke.parseArgs(ModeArgs::class.java).mode
    resolveJson(invoke, DeviceExtras.dnd(activity.applicationContext, mode))
  }

  @Command
  fun goHome(invoke: Invoke) {
    resolveJson(invoke, DeviceExtras.home(activity.applicationContext))
  }

  @Command
  fun phoneStatus(invoke: Invoke) {
    resolveJson(invoke, DeviceExtras.status(activity.applicationContext))
  }

  @Command
  fun accessStatus(invoke: Invoke) {
    resolveJson(invoke, DeviceExtras.access(activity.applicationContext))
  }

  @Command
  fun openAccessSettings(invoke: Invoke) {
    val kind = invoke.parseArgs(KindArgs::class.java).kind
    resolveJson(invoke, DeviceExtras.openAccess(activity, kind))
  }

  @Command
  fun contactBirthday(invoke: Invoke) {
    val id = invoke.parseArgs(ContactIdArgs::class.java).id
    resolveJson(invoke, DeviceExtras.birthday(activity.applicationContext, id))
  }

  @Command
  fun lastContact(invoke: Invoke) {
    val args = invoke.parseArgs(LastContactArgs::class.java)
    resolveJson(invoke, DeviceExtras.lastContact(activity.applicationContext, args.phone, args.name))
  }

  @InvokeArg
  class ModeArgs {
    var mode: String = ""
  }

  @InvokeArg
  class KindArgs {
    var kind: String = ""
  }

  @InvokeArg
  class LastContactArgs {
    var id: String = ""
    var phone: String = ""
    var name: String = ""
  }

  @Command
  fun deviceFacts(invoke: Invoke) {
    val am = activity.getSystemService(ActivityManager::class.java)
    val info = ActivityManager.MemoryInfo()
    am?.getMemoryInfo(info)
    val ret = JSObject()
    ret.put("availMb", info.availMem / (1024 * 1024))
    ret.put("lowMemory", info.lowMemory)
    ret.put("online", online())
    invoke.resolve(ret)
  }

  @Command
  fun speakFallback(invoke: Invoke) {
    val text = invoke.parseArgs(SpeakArgs::class.java).text.trim().take(180)
    if (text.isEmpty()) {
      invoke.resolve()
      return
    }
    val latch = CountDownLatch(1)
    activity.runOnUiThread {
      var engine: TextToSpeech? = null
      engine = TextToSpeech(activity) { status ->
        val tts = engine
        if (tts == null || status != TextToSpeech.SUCCESS) {
          latch.countDown()
          return@TextToSpeech
        }
        tts.language = Locale.US
        tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
          override fun onStart(utteranceId: String?) {}
          override fun onDone(utteranceId: String?) {
            tts.shutdown()
            latch.countDown()
          }
          override fun onError(utteranceId: String?) {
            tts.shutdown()
            latch.countDown()
          }
        })
        val params = Bundle()
        val queued = tts.speak(text, TextToSpeech.QUEUE_FLUSH, params, "gv-local")
        if (queued == TextToSpeech.ERROR) {
          tts.shutdown()
          latch.countDown()
        }
      }
    }
    latch.await(20, TimeUnit.SECONDS)
    invoke.resolve()
  }

  private fun online(): Boolean {
    val cm = activity.getSystemService(ConnectivityManager::class.java) ?: return false
    val network = cm.activeNetwork ?: return false
    val caps = cm.getNetworkCapabilities(network) ?: return false
    return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
  }

  private fun resolveJson(invoke: Invoke, json: String) {
    val ret = JSObject()
    ret.put("json", json)
    invoke.resolve(ret)
  }
}
