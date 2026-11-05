package ai.grokvoice.app

import android.app.AlarmManager
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.CalendarContract
import android.widget.RemoteViews
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.text.DateFormat
import java.util.Date

/**
 * Home-screen widgets. They read the same agenda.json the voice app writes,
 * plus the phone calendar, and they never start the microphone themselves.
 * Listening starts from the activity so Android still sees a foreground app.
 */
object HomeWidgets {
  const val ACTION_TOGGLE = "ai.grokvoice.app.widget.TOGGLE_LISTEN"
  const val ACTION_TALK = "ai.grokvoice.app.widget.LISTEN_NOW"
  const val ACTION_OPEN = "ai.grokvoice.app.widget.OPEN"
  const val ACTION_REFRESH = "ai.grokvoice.app.widget.REFRESH"
  const val EXTRA_TAB = "tab"

  private const val TICK = "ai.grokvoice.app.widget.TICK"
  private const val ROW_COUNT = 4

  fun updateAll(context: Context) {
    val mgr = AppWidgetManager.getInstance(context)
    push(context, mgr, ListenWidget::class.java) { views, _ -> bindListen(context, views) }
    push(context, mgr, RemindersWidget::class.java) { views, _ ->
      bindRows(context, views, "Reminders", reminderLines(context), "No reminders yet", "reminders")
    }
    push(context, mgr, ScheduleWidget::class.java) { views, _ ->
      bindRows(context, views, "Coming up", scheduleLines(context), "Nothing scheduled", "events")
    }
    scheduleTick(context)
  }

  fun open(context: Context, widget: String) {
    val launch = Intent(context, MainActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      .putExtra(MainActivity.EXTRA_WIDGET, widget)
    context.startActivity(launch)
  }

  fun onReceive(context: Context, intent: Intent) {
    when (intent.action) {
      ACTION_TOGGLE -> {
        val arm = !VoiceListenerService.isArmed(context)
        AppData.setListenArmed(context, arm)
        open(context, if (arm) "arm" else "disarm")
      }
      ACTION_TALK -> open(context, "talk")
      ACTION_OPEN -> open(context, intent.getStringExtra(EXTRA_TAB) ?: "talk")
      ACTION_REFRESH, TICK, Intent.ACTION_BOOT_COMPLETED -> updateAll(context)
    }
  }

  private fun push(
    context: Context,
    mgr: AppWidgetManager,
    cls: Class<out AppWidgetProvider>,
    bind: (RemoteViews, Int) -> Unit,
  ) {
    val ids = mgr.getAppWidgetIds(ComponentName(context, cls))
    val layout = when (cls) {
      ListenWidget::class.java -> R.layout.widget_listen
      RemindersWidget::class.java -> R.layout.widget_list
      else -> R.layout.widget_list
    }
    for (id in ids) {
      val views = RemoteViews(context.packageName, layout)
      bind(views, id)
      mgr.updateAppWidget(id, views)
    }
  }

  private fun bindListen(context: Context, views: RemoteViews) {
    val on = VoiceListenerService.isArmed(context)
    views.setTextViewText(R.id.widget_listen_state, if (on) "Background listening is on" else "Background listening is off")
    views.setTextViewText(R.id.widget_listen_toggle, if (on) "Stop" else "Listen in background")
    views.setOnClickPendingIntent(R.id.widget_listen_toggle, broadcast(context, ACTION_TOGGLE, 1))
    views.setOnClickPendingIntent(R.id.widget_listen_now, broadcast(context, ACTION_TALK, 2))
  }

  private fun bindRows(context: Context, views: RemoteViews, title: String, lines: List<Line>, empty: String, tab: String) {
    val open = PendingIntent.getBroadcast(
      context, if (tab == "reminders") 3 else 4,
      Intent(context, HomeWidgetReceiver::class.java).setAction(ACTION_OPEN).putExtra(EXTRA_TAB, tab),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    views.setOnClickPendingIntent(R.id.widget_root, open)
    views.setTextViewText(R.id.widget_title, title)
    val shown = if (lines.isEmpty()) listOf(Line(0, "", empty, "")) else lines.take(ROW_COUNT)
    val ids = intArrayOf(R.id.widget_row_1, R.id.widget_row_2, R.id.widget_row_3, R.id.widget_row_4)
    for (i in ids.indices) {
      val line = shown.getOrNull(i)
      if (line == null) {
        views.setTextViewText(ids[i], "")
      } else {
        val prefix = if (line.detail.isBlank()) "" else "${line.detail}  "
        views.setTextViewText(ids[i], prefix + line.title)
      }
    }
  }

  private fun broadcast(context: Context, action: String, request: Int): PendingIntent {
    val intent = Intent(context, HomeWidgetReceiver::class.java).setAction(action)
    return PendingIntent.getBroadcast(
      context, request, intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
  }

  private fun scheduleTick(context: Context) {
    val am = context.getSystemService(AlarmManager::class.java) ?: return
    val intent = Intent(context, HomeWidgetReceiver::class.java).setAction(TICK)
    val pi = PendingIntent.getBroadcast(
      context, 9, intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val soon = scheduleLines(context).any { it.kind == "timer" }
    if (!soon) {
      am.cancel(pi)
      return
    }
    am.set(AlarmManager.RTC, System.currentTimeMillis() + 30_000L, pi)
  }

  private fun reminderLines(context: Context): List<Line> {
    val now = System.currentTimeMillis()
    return items(context)
      .filter { it.kind == "reminder" && it.atMs >= now - 60_000L }
      .sortedBy { it.atMs }
      .map { Line(it.atMs, it.kind, it.title.ifBlank { "Reminder" }, clock(it.atMs)) }
  }

  private fun scheduleLines(context: Context): List<Line> {
    val now = System.currentTimeMillis()
    val timed = items(context).filter { it.kind == "alarm" || it.kind == "timer" }.map {
      val title = it.title.ifBlank { if (it.kind == "timer") "Countdown" else "Alarm" }
      val detail = if (it.kind == "timer") remaining(it.atMs, now) else clock(it.atMs)
      Line(it.atMs, it.kind, title, detail)
    }
    val horizon = now + 36L * 60L * 60L * 1000L
    val events = events(context, now - 60_000L, horizon).map {
      Line(it.startMs, "event", it.title.ifBlank { "Event" }, if (it.allDay) "All day" else clock(it.startMs))
    }
    return (timed + events).sortedBy { it.whenMs }
  }

  private fun items(context: Context): List<Item> {
    val fromFile = AppData.readAgendaItems(context)
    if (fromFile.isNotEmpty()) return fromFile
    return PhoneActions.storedItems(context)
  }

  private fun events(context: Context, startMs: Long, endMs: Long): List<Event> {
    val local = AppData.readLocalEvents(context).filter { it.endMs > startMs && it.startMs < endMs }
    val phone = ArrayList<Event>()
    val proj = arrayOf(
      CalendarContract.Events.TITLE,
      CalendarContract.Events.DTSTART,
      CalendarContract.Events.DTEND,
      CalendarContract.Events.ALL_DAY,
    )
    val sel = "${CalendarContract.Events.DTSTART} < ? AND ${CalendarContract.Events.DTSTART} >= ?"
    try {
      context.contentResolver.query(
        CalendarContract.Events.CONTENT_URI,
        proj,
        sel,
        arrayOf(endMs.toString(), (startMs - 86_400_000L).toString()),
        "${CalendarContract.Events.DTSTART} ASC",
      )?.use { c ->
        var n = 0
        while (c.moveToNext() && n < 12) {
          val start = c.getLong(1)
          val end = c.getLong(2).let { if (it == 0L) start + 3_600_000L else it }
          if (end <= startMs) continue
          n++
          phone.add(Event(c.getString(0) ?: "Event", start, end, c.getInt(3) == 1))
        }
      }
    } catch (_: SecurityException) {
    }
    return (local + phone).sortedBy { it.startMs }
  }

  private fun clock(ms: Long): String =
    DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))

  private fun remaining(atMs: Long, now: Long): String {
    val left = atMs - now
    if (left <= 0) return "now"
    val mins = left / 60_000L
    val hours = mins / 60
    val m = mins % 60
    return if (hours > 0) "${hours}h ${m}m" else "${mins.coerceAtLeast(1)}m"
  }

  data class Item(val kind: String, val title: String, val atMs: Long)
  data class Event(val title: String, val startMs: Long, val endMs: Long, val allDay: Boolean)
  data class Line(val whenMs: Long, val kind: String, val title: String, val detail: String)
}

object AppData {
  fun readAgendaItems(context: Context): List<HomeWidgets.Item> {
    val root = readJson(agendaFile(context)) ?: return emptyList()
    val arr = root.optJSONArray("items") ?: return emptyList()
    return parseItems(arr)
  }

  fun readLocalEvents(context: Context): List<HomeWidgets.Event> {
    val root = readJson(agendaFile(context)) ?: return emptyList()
    val arr = root.optJSONArray("events") ?: return emptyList()
    val out = ArrayList<HomeWidgets.Event>()
    for (i in 0 until arr.length()) {
      val o = arr.optJSONObject(i) ?: continue
      out.add(
        HomeWidgets.Event(
          o.optString("title"),
          o.optLong("startMs"),
          o.optLong("endMs"),
          o.optBoolean("allDay"),
        ),
      )
    }
    return out
  }

  fun setListenArmed(context: Context, armed: Boolean) {
    val file = settingsFile(context) ?: return
    val root = readJson(file) ?: JSONObject()
    root.put("listenArmed", armed)
    try {
      file.parentFile?.mkdirs()
      file.writeText(root.toString(2))
    } catch (_: Exception) {
    }
  }

  private fun parseItems(arr: JSONArray): List<HomeWidgets.Item> {
    val out = ArrayList<HomeWidgets.Item>()
    for (i in 0 until arr.length()) {
      val o = arr.optJSONObject(i) ?: continue
      out.add(HomeWidgets.Item(o.optString("kind"), o.optString("title"), o.optLong("atMs")))
    }
    return out
  }

  private fun readJson(file: File?): JSONObject? {
    if (file == null || !file.isFile) return null
    return try {
      JSONObject(file.readText())
    } catch (_: Exception) {
      null
    }
  }

  private fun agendaFile(context: Context): File? = firstExisting(context, "agenda.json")

  private fun settingsFile(context: Context): File? =
    firstExisting(context, "settings.json") ?: File(File(context.filesDir, "ai.grokvoice.app"), "settings.json")

  private fun firstExisting(context: Context, name: String): File? {
    val dirs = listOf(File(context.filesDir, "ai.grokvoice.app"), context.filesDir)
    return dirs.map { File(it, name) }.firstOrNull { it.isFile }
  }
}

class HomeWidgetReceiver : android.content.BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    HomeWidgets.onReceive(context, intent)
  }
}

class ListenWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }
}

class RemindersWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }
}

class ScheduleWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }
}
