package ai.grokvoice.app

import android.app.Notification
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Parcelable
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Short in-memory cache of messaging notifications. Bodies are kept only while
 * message reading is on and the app is allowed. Nothing is written to disk.
 * The list matches the Rust `MESSAGING_APPS` default.
 */
object Inbox {
  private const val MAX = 24
  private const val MAX_AGE_MS = 2 * 60 * 60 * 1000L

  val DEFAULTS: List<Pair<String, String>> = listOf(
    "com.whatsapp" to "WhatsApp",
    "com.whatsapp.w4b" to "WhatsApp Business",
    "org.thoughtcrime.securesms" to "Signal",
    "org.telegram.messenger" to "Telegram",
    "org.telegram.messenger.web" to "Telegram",
    "org.thunderdog.challegram" to "Telegram",
    "org.telegram.plus" to "Telegram",
    "com.google.android.apps.messaging" to "Messages",
    "com.android.mms" to "Messages",
    "com.samsung.android.messaging" to "Messages",
    "com.google.android.gm" to "Gmail",
    "com.microsoft.office.outlook" to "Outlook",
    "com.yahoo.mobile.client.android.mail" to "Yahoo Mail",
    "ch.threema.app" to "Threema",
    "com.facebook.orca" to "Messenger",
    "com.discord" to "Discord",
    "com.Slack" to "Slack",
    "im.vector.app" to "Element",
    "com.viber.voip" to "Viber",
  )

  data class Msg(
    val id: String,
    val pkg: String,
    val app: String,
    val sender: String,
    val conversation: String,
    val text: String,
    val group: Boolean,
    val message: Boolean,
    val category: String,
    val ongoing: Boolean,
    val whenMs: Long,
    val reply: Boolean,
    val system: Boolean,
    val promo: Boolean,
  )

  data class CallNote(
    val name: String,
    val answer: Notification.Action?,
    val decline: Notification.Action?,
    val hangup: Notification.Action?,
  )

  private val cache = LinkedHashMap<String, Msg>()
  private val seen = LinkedHashSet<String>()
  private val announced = HashMap<String, String>()
  @Volatile private var announcedCall: String = ""
  @Volatile private var service: NoticeListener? = null

  fun onConnected(listener: NoticeListener) {
    service = listener
    refresh(listener, announce = false)
  }

  fun onDestroy(listener: NoticeListener) {
    if (service == listener) service = null
  }

  fun onPosted(listener: NoticeListener, sbn: StatusBarNotification) {
    service = listener
    ingest(listener, sbn, announce = true)
  }

  fun onRemoved(sbn: StatusBarNotification) {
    synchronized(cache) { cache.remove(sbn.key) }
    synchronized(announced) { announced.remove(sbn.key) }
    if (announcedCall == sbn.key) announcedCall = ""
  }

  fun tracks(pkg: String): Boolean = DEFAULTS.any { it.first == pkg }

  fun snapshot(context: Context): String {
    val settings = readSettings(context)
    val listener = NoticeListener.enabled(context)
    if (service == null && listener) {
      try {
        NotificationListenerService.requestRebind(ComponentName(context, NoticeListener::class.java))
      } catch (_: Exception) {
      }
    }
    service?.let { refresh(it, announce = false) }
    val triage = settings.optBoolean("messageTriage", false)
    val locked = locked(context)
    val redact = settings.optBoolean("redactLocked", true)
    val rows = JSONArray()
    if (listener && triage) {
      val now = System.currentTimeMillis()
      synchronized(cache) {
        val stale = cache.filterValues { now - it.whenMs > MAX_AGE_MS }.keys.toList()
        stale.forEach { cache.remove(it) }
        for (msg in cache.values) {
          if (!allowed(settings, msg.pkg)) continue
          val text = if (locked && redact) "" else msg.text
          rows.put(
            JSONObject()
              .put("id", msg.id)
              .put("package", msg.pkg)
              .put("app", msg.app)
              .put("sender", msg.sender)
              .put("conversation", msg.conversation)
              .put("text", text)
              .put("group", msg.group)
              .put("message", msg.message)
              .put("category", msg.category)
              .put("ongoing", msg.ongoing)
              .put("whenMs", msg.whenMs)
              .put("reply", msg.reply)
              .put("system", msg.system)
              .put("promo", msg.promo)
          )
        }
      }
    }
    return JSONObject()
      .put("ok", listener)
      .put("listener", listener)
      .put("triage", triage)
      .put("locked", locked)
      .put("messages", rows)
      .toString()
  }

  fun reply(context: Context, id: String, text: String): String {
    val settings = readSettings(context)
    if (!NoticeListener.enabled(context) || !settings.optBoolean("messageTriage", false)) {
      return fail("inbox-off")
    }
    val body = text.trim()
    if (body.isEmpty()) return fail("inbox-text")
    val sbn = service?.activeNotifications?.firstOrNull { it.key == id }
      ?: return JSONObject().put("ok", false).put("code", "no-reply").toString()
    if (!allowed(settings, sbn.packageName ?: "")) return fail("inbox-off")
    val action = replyAction(sbn.notification) ?: return JSONObject().put("ok", false).put("code", "no-reply").toString()
    val input = action.remoteInputs?.firstOrNull { it.allowFreeFormInput } ?: action.remoteInputs?.firstOrNull()
      ?: return JSONObject().put("ok", false).put("code", "no-reply").toString()
    val fillIn = Intent()
    val results = Bundle()
    results.putCharSequence(input.resultKey, body.take(1000))
    RemoteInput.addResultsToIntent(arrayOf(input), fillIn, results)
    return try {
      action.actionIntent.send(context, 0, fillIn)
      markRead(context, sbn.notification)
      JSONObject().put("ok", true).put("via", "remote").toString()
    } catch (_: PendingIntent.CanceledException) {
      JSONObject().put("ok", false).put("code", "no-reply").toString()
    }
  }

  fun share(context: Context, pkg: String, text: String): String {
    val body = text.trim()
    if (body.isEmpty()) return fail("inbox-text")
    val intent = Intent(Intent.ACTION_SEND).apply {
      type = "text/plain"
      putExtra(Intent.EXTRA_TEXT, body.take(1000))
      if (pkg.isNotBlank()) setPackage(pkg)
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    return try {
      context.startActivity(intent)
      JSONObject().put("ok", true).toString()
    } catch (_: Exception) {
      fail("inbox-no-target")
    }
  }

  fun apps(context: Context): String {
    val extra = synchronized(seen) { seen.toList() }
    val rows = JSONArray()
    val used = HashSet<String>()
    for (pkg in DEFAULTS.map { it.first } + extra) {
      if (!used.add(pkg)) continue
      val installed = installed(context, pkg)
      val fallback = DEFAULTS.find { it.first == pkg }?.second ?: pkg
      rows.put(
        JSONObject()
          .put("packageName", pkg)
          .put("label", if (installed) appLabel(context, pkg) else fallback)
          .put("installed", installed)
          .put("messaging", DEFAULTS.any { it.first == pkg })
      )
    }
    return JSONObject().put("ok", true).put("apps", rows).toString()
  }

  fun ringingCall(): CallNote? {
    val notes = service?.activeNotifications ?: return null
    for (sbn in notes) {
      val note = callNote(sbn) ?: continue
      return note
    }
    return null
  }

  private fun refresh(listener: NoticeListener, announce: Boolean) {
    val active = listener.activeNotifications ?: return
    val live = HashSet<String>()
    for (sbn in active) {
      live.add(sbn.key)
      ingest(listener, sbn, announce)
    }
    synchronized(cache) {
      val gone = cache.keys.filter { it !in live }
      gone.forEach { cache.remove(it) }
    }
  }

  private fun ingest(context: Context, sbn: StatusBarNotification, announce: Boolean) {
    val msg = parse(context, sbn) ?: return
    if (msg.category == "call") {
      if (announce) maybeAnnounceCall(context, sbn)
      return
    }
    val settings = readSettings(context)
    if (msg.message && !tracks(msg.pkg)) {
      synchronized(seen) {
        seen.add(msg.pkg)
        while (seen.size > 12) {
          val first = seen.firstOrNull() ?: break
          seen.remove(first)
        }
      }
    }
    if (!settings.optBoolean("messageTriage", false) || !allowed(settings, msg.pkg)) return
    if (msg.system || msg.promo) return
    val previous = synchronized(cache) { cache[msg.id]?.text }
    synchronized(cache) {
      cache[msg.id] = msg
      while (cache.size > MAX) {
        val eldest = cache.entries.firstOrNull()?.key ?: break
        cache.remove(eldest)
      }
    }
    if (announce && previous != msg.text) maybeAnnounce(context, msg, settings)
  }

  private fun maybeAnnounce(context: Context, msg: Msg, settings: JSONObject) {
    if (!settings.optBoolean("announceMessages", false)) return
    if (!settings.optBoolean("listenArmed", false)) return
    if (msg.group || !msg.message || msg.text.isBlank()) return
    if (SpokenAlerts.quietForAnnounce(context, calls = false)) return
    val fresh = synchronized(announced) {
      if (announced[msg.id] == msg.text) false else {
        announced[msg.id] = msg.text
        true
      }
    }
    if (!fresh) return
    val who = msg.sender.ifBlank { msg.app }
    val line = if (locked(context) && settings.optBoolean("redactLocked", true)) {
      context.getString(R.string.inbox_announce_hidden, who, msg.app)
    } else {
      context.getString(R.string.inbox_announce, who, msg.app, msg.text.take(180))
    }
    SpokenAlerts.speakLine(context, line)
  }

  private fun maybeAnnounceCall(context: Context, sbn: StatusBarNotification) {
    val settings = readSettings(context)
    if (!settings.optBoolean("announceCalls", false)) return
    if (!settings.optBoolean("listenArmed", false)) return
    if (SpokenAlerts.quietForAnnounce(context, calls = true)) return
    val note = callNote(sbn) ?: return
    if (note.answer == null && note.decline == null) return
    if (announcedCall == sbn.key) return
    announcedCall = sbn.key
    SpokenAlerts.speakLine(context, context.getString(R.string.inbox_call_announce, note.name), duringRing = true)
  }

  private fun parse(context: Context, sbn: StatusBarNotification): Msg? {
    val pkg = sbn.packageName ?: return null
    if (pkg == context.packageName) return null
    val notification = sbn.notification ?: return null
    val extras = notification.extras ?: Bundle()
    val category = categoryOf(notification)
    val lines = messageLines(extras)
    val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty()
    val sender = lines.lastOrNull()?.first?.ifBlank { title } ?: title
    val text = lines.lastOrNull()?.second
      ?: extras.getCharSequence(Notification.EXTRA_TEXT)?.toString()?.trim().orEmpty().ifBlank {
        extras.getCharSequence(Notification.EXTRA_BIG_TEXT)?.toString()?.trim().orEmpty()
      }
    val conversation = extras.getCharSequence("android.conversationTitle")?.toString()?.trim().orEmpty()
    val group = isGroup(extras, conversation, sender)
    val messaging = lines.isNotEmpty() || category == "msg" || category == "email" || tracks(pkg)
    return Msg(
      id = sbn.key,
      pkg = pkg,
      app = appLabel(context, pkg),
      sender = sender.take(80),
      conversation = conversation.take(80),
      text = text.take(400),
      group = group,
      message = messaging && category != "call",
      category = category,
      ongoing = (notification.flags and Notification.FLAG_ONGOING_EVENT) != 0,
      whenMs = if (sbn.postTime > 0) sbn.postTime else System.currentTimeMillis(),
      reply = replyAction(notification) != null,
      system = category == "system" || pkg == "android" || pkg == "com.android.systemui",
      promo = category == "promo",
    )
  }

  private fun categoryOf(notification: Notification): String {
    return when (notification.category) {
      Notification.CATEGORY_MESSAGE -> "msg"
      Notification.CATEGORY_EMAIL -> "email"
      Notification.CATEGORY_CALL -> "call"
      Notification.CATEGORY_PROMO, Notification.CATEGORY_RECOMMENDATION -> "promo"
      Notification.CATEGORY_ALARM -> "alarm"
      Notification.CATEGORY_REMINDER, Notification.CATEGORY_EVENT -> "reminder"
      Notification.CATEGORY_SYSTEM, Notification.CATEGORY_SERVICE, Notification.CATEGORY_ERROR,
      Notification.CATEGORY_PROGRESS, Notification.CATEGORY_STATUS, Notification.CATEGORY_TRANSPORT,
      Notification.CATEGORY_NAVIGATION -> "system"
      else -> ""
    }
  }

  private fun messageLines(extras: Bundle): List<Pair<String, String>> {
    val raw = if (Build.VERSION.SDK_INT >= 33) {
      extras.getParcelableArray(Notification.EXTRA_MESSAGES, Parcelable::class.java)
    } else {
      @Suppress("DEPRECATION")
      extras.getParcelableArray(Notification.EXTRA_MESSAGES)
    } ?: return emptyList()
    val out = ArrayList<Pair<String, String>>()
    for (item in raw) {
      val bundle = item as? Bundle ?: continue
      val text = bundle.getCharSequence("text")?.toString()?.trim().orEmpty()
      if (text.isEmpty()) continue
      var sender = bundle.getCharSequence("sender")?.toString()?.trim().orEmpty()
      if (sender.isEmpty() && Build.VERSION.SDK_INT >= 28) {
        val person = personOf(bundle)
        sender = person?.name?.toString()?.trim().orEmpty()
      }
      out.add(sender to text.take(400))
    }
    return out
  }

  private fun personOf(bundle: Bundle): android.app.Person? {
    if (Build.VERSION.SDK_INT < 28) return null
    return if (Build.VERSION.SDK_INT >= 33) {
      bundle.getParcelable("sender_person", android.app.Person::class.java)
    } else {
      @Suppress("DEPRECATION")
      bundle.getParcelable("sender_person") as? android.app.Person
    }
  }

  private fun isGroup(extras: Bundle, conversation: String, sender: String): Boolean {
    if (Build.VERSION.SDK_INT >= 28 && extras.getBoolean("android.isGroupConversation")) return true
    return conversation.isNotEmpty() && !conversation.equals(sender, ignoreCase = true)
  }

  private fun replyAction(notification: Notification): Notification.Action? {
    val actions = notification.actions ?: return null
    var fallback: Notification.Action? = null
    for (action in actions) {
      val inputs = action.remoteInputs ?: continue
      if (inputs.isEmpty()) continue
      if (Build.VERSION.SDK_INT >= 28 && action.semanticAction == Notification.Action.SEMANTIC_ACTION_REPLY) return action
      if (fallback == null) fallback = action
    }
    return fallback
  }

  private fun markRead(context: Context, notification: Notification) {
    val actions = notification.actions ?: return
    for (action in actions) {
      val semantic = Build.VERSION.SDK_INT >= 28 && action.semanticAction == Notification.Action.SEMANTIC_ACTION_MARK_AS_READ
      val title = action.title?.toString()?.lowercase().orEmpty()
      val titled = title.contains("read") || title.contains("leíd") || title.contains("gelesen") || title.contains("lida") || title.contains("llegit")
      if (!semantic && !titled) continue
      try {
        action.actionIntent?.send(context, 0, Intent())
      } catch (_: Exception) {
      }
      return
    }
  }

  private fun callNote(sbn: StatusBarNotification): CallNote? {
    val notification = sbn.notification ?: return null
    val call = notification.category == Notification.CATEGORY_CALL
    val answer = actionMatching(notification, call, "answer")
    val decline = actionMatching(notification, call, "decline")
    val hangup = actionMatching(notification, call, "hangup")
    if (!call && answer == null && decline == null && hangup == null) return null
    if (answer == null && decline == null && hangup == null) return null
    val title = notification.extras?.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty()
    return CallNote(title.ifBlank { "the caller" }, answer, decline, hangup)
  }

  private fun actionMatching(notification: Notification, call: Boolean, kind: String): Notification.Action? {
    val actions = notification.actions ?: return null
    for (action in actions) {
      val title = action.title?.toString()?.lowercase()?.trim().orEmpty()
      val semantic = if (Build.VERSION.SDK_INT >= 28) action.semanticAction else 0
      val match = when (kind) {
        "answer" -> semantic == 10 || title in ANSWER || (call && title in ANSWER_LOOSE)
        "decline" -> title in DECLINE
        else -> title in HANGUP
      }
      if (match) return action
    }
    return null
  }

  private fun allowed(settings: JSONObject, pkg: String): Boolean {
    if (!settings.optBoolean("messageAppsCustom", false)) return tracks(pkg)
    return settings.optString("messageApps", "").split(',').any { it.trim() == pkg }
  }

  private fun locked(context: Context): Boolean {
    val km = context.getSystemService(android.app.KeyguardManager::class.java) ?: return false
    return km.isKeyguardLocked
  }

  private fun installed(context: Context, pkg: String): Boolean {
    return try {
      if (Build.VERSION.SDK_INT >= 33) {
        context.packageManager.getPackageInfo(pkg, PackageManager.PackageInfoFlags.of(0))
      } else {
        @Suppress("DEPRECATION")
        context.packageManager.getPackageInfo(pkg, 0)
      }
      true
    } catch (_: Exception) {
      false
    }
  }

  private fun appLabel(context: Context, pkg: String): String {
    return try {
      val info = context.packageManager.getApplicationInfo(pkg, 0)
      context.packageManager.getApplicationLabel(info)?.toString()?.trim().orEmpty().ifBlank {
        DEFAULTS.find { it.first == pkg }?.second ?: pkg
      }
    } catch (_: Exception) {
      DEFAULTS.find { it.first == pkg }?.second ?: pkg
    }
  }

  fun readSettings(context: Context): JSONObject {
    val file = roots(context).firstOrNull { File(it, "settings.json").isFile }?.let { File(it, "settings.json") }
      ?: return JSONObject()
    return try {
      JSONObject(file.readText())
    } catch (_: Exception) {
      JSONObject()
    }
  }

  private fun roots(context: Context): List<File> {
    val data = context.dataDir
    val files = context.filesDir
    return listOf(data, File(files, "ai.grokvoice.app"), files, File(data, "ai.grokvoice.app")).distinctBy { it.absolutePath }
  }

  private fun fail(key: String) = JSONObject().put("ok", false).put("error", key).toString()

  private val ANSWER = setOf("answer", "accept", "annehmen", "atender", "acceptar", "contestar")
  private val ANSWER_LOOSE = setOf("responder")
  private val DECLINE = setOf("decline", "reject", "dismiss", "rechazar", "ablehnen", "recusar", "rebutjar", "rechazar llamada")
  private val HANGUP = setOf("hang up", "hangup", "end call", "end", "colgar", "auflegen", "desligar", "penjar", "beenden")
}
