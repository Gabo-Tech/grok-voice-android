package ai.grokvoice.app

import android.app.AlarmManager
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.CalendarContract
import android.view.View
import android.widget.RemoteViews
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.text.DateFormat
import java.util.Calendar
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

  fun updateAll(context: Context) {
    val mgr = AppWidgetManager.getInstance(context)
    val theme = WidgetTheme.read(context)
    val finance = AppData.widgetFinance(context)
    val text = AppLocale.wrap(context)
    push(context, mgr, ListenWidget::class.java) { views, id -> bindListen(context, views, theme, heightOf(mgr, id)) }
    push(context, mgr, RemindersWidget::class.java) { views, id ->
      bindRows(context, views, theme, text.getString(R.string.widget_reminders_title), reminderLines(context), text.getString(R.string.widget_reminders_empty), "reminders", rowsFor(heightOf(mgr, id)))
    }
    push(context, mgr, ScheduleWidget::class.java) { views, id ->
      bindRows(context, views, theme, text.getString(R.string.widget_schedule_title), scheduleLines(context), text.getString(R.string.widget_schedule_empty), "events", rowsFor(heightOf(mgr, id)))
    }
    push(context, mgr, NetWorthWidget::class.java) { views, _ ->
      bindFigure(context, views, theme, text.getString(R.string.widget_net_title), figureText(context, finance?.optJSONObject("net"), R.string.widget_net_empty), "", "finance", theme.ink, 11)
    }
    push(context, mgr, MonthWidget::class.java) { views, _ ->
      val node = finance?.optJSONObject("month")
      val value = figureText(context, node, R.string.widget_month_empty)
      val color = when {
        node?.optString("state") == "ready" && node.optString("text").startsWith("-") -> theme.expense
        node?.optString("state") == "ready" -> theme.income
        else -> theme.ink
      }
      bindFigure(context, views, theme, text.getString(R.string.widget_month_title), value, "", "finance", color, 12)
    }
    push(context, mgr, WindowWidget::class.java) { views, _ ->
      val incomingNode = finance?.optJSONObject("in")
      val outgoingNode = finance?.optJSONObject("out")
      val inState = incomingNode?.optString("state").orEmpty()
      val outState = outgoingNode?.optString("state").orEmpty()
      if (inState != "ready" && outState != "ready" && inState != "needs-rate" && outState != "needs-rate") {
        bindFigure(context, views, theme, text.getString(R.string.widget_window_title), text.getString(R.string.widget_window_empty), "", "finance", theme.ink, 13)
        return@push
      }
      val needs = listOf(incomingNode, outgoingNode).filter { it?.optString("state") == "needs-rate" }
      if (needs.isNotEmpty()) {
        val missing = needs.map { it?.optString("missing").orEmpty() }.filter { it.isNotBlank() }.distinct().joinToString(", ")
        bindFigure(context, views, theme, text.getString(R.string.widget_window_title), text.getString(R.string.widget_needs_rate, missing), "", "finance", theme.ink, 13)
      } else {
        val incoming = figureText(context, incomingNode, R.string.widget_window_empty)
        val outgoing = figureText(context, outgoingNode, R.string.widget_window_empty)
        bindFigure(
          context,
          views,
          theme,
          text.getString(R.string.widget_window_title),
          "${text.getString(R.string.widget_in_label)}  $incoming",
          "${text.getString(R.string.widget_out_label)}  $outgoing",
          "finance",
          theme.ink,
          13,
        )
      }
    }
    push(context, mgr, NextWidget::class.java) { views, _ -> bindNext(context, views, theme) }
    scheduleTick(context)
  }

  fun open(context: Context, widget: String) {
    WidgetGate.allow(context, widget)
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
      RemindersWidget::class.java, ScheduleWidget::class.java -> R.layout.widget_list
      else -> R.layout.widget_figure
    }
    for (id in ids) {
      val views = RemoteViews(context.packageName, layout)
      bind(views, id)
      mgr.updateAppWidget(id, views)
    }
  }

  private fun heightOf(mgr: AppWidgetManager, id: Int): Int =
    mgr.getAppWidgetOptions(id).getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 180)

  private fun rowsFor(height: Int): Int = when {
    height < 130 -> 2
    height < 180 -> 3
    else -> 4
  }

  private fun bindListen(context: Context, views: RemoteViews, theme: WidgetTheme.Palette, height: Int) {
    val on = VoiceListenerService.isArmed(context)
    val text = AppLocale.wrap(context)
    WidgetTheme.paint(views, theme)
    views.setTextColor(R.id.widget_brand, theme.ink)
    views.setTextColor(R.id.widget_listen_state, theme.muted)
    views.setTextColor(R.id.widget_next, theme.ink)
    views.setTextViewText(R.id.widget_next, nextReminder(context).ifBlank { text.getString(R.string.widget_next_empty) })
    views.setViewVisibility(R.id.widget_next, if (height < 120) View.GONE else View.VISIBLE)
    views.setTextViewText(R.id.widget_listen_state, text.getString(if (on) R.string.widget_listen_on else R.string.widget_listen_off))
    views.setTextViewText(R.id.widget_listen_toggle, text.getString(if (on) R.string.listener_stop else R.string.widget_listen_start))
    views.setInt(R.id.widget_listen_toggle, "setBackgroundColor", theme.accent)
    views.setTextColor(R.id.widget_listen_toggle, theme.onAccent)
    views.setInt(R.id.widget_listen_now, "setBackgroundColor", theme.panel)
    views.setTextColor(R.id.widget_listen_now, theme.ink)
    views.setOnClickPendingIntent(R.id.widget_listen_toggle, broadcast(context, ACTION_TOGGLE, 1))
    views.setOnClickPendingIntent(R.id.widget_listen_now, broadcast(context, ACTION_TALK, 2))
  }

  private fun bindFigure(context: Context, views: RemoteViews, theme: WidgetTheme.Palette, title: String, value: String, note: String, tab: String, valueColor: Int, request: Int) {
    WidgetTheme.paint(views, theme)
    views.setTextColor(R.id.widget_kicker, theme.accent)
    views.setTextColor(R.id.widget_value, valueColor)
    views.setTextColor(R.id.widget_note, theme.muted)
    views.setTextViewText(R.id.widget_kicker, title)
    views.setTextViewText(R.id.widget_value, value)
    views.setTextViewText(R.id.widget_note, note)
    views.setViewVisibility(R.id.widget_note, if (note.isBlank()) View.GONE else View.VISIBLE)
    views.setOnClickPendingIntent(R.id.widget_root, openIntent(context, tab, request))
  }

  private fun bindNext(context: Context, views: RemoteViews, theme: WidgetTheme.Palette) {
    val text = AppLocale.wrap(context)
    val reminder = reminderLines(context).firstOrNull()
    val event = scheduleLines(context).firstOrNull()
    val next = listOfNotNull(reminder, event).minByOrNull { it.whenMs }
    val tab = if (next == null || next.kind == "reminder") "reminders" else "events"
    val value = next?.title ?: text.getString(R.string.widget_next_empty)
    val note = next?.detail.orEmpty()
    bindFigure(context, views, theme, text.getString(R.string.widget_next_title), value, note, tab, theme.ink, 14)
  }

  private fun figureText(context: Context, node: org.json.JSONObject?, empty: Int): String {
    val text = AppLocale.wrap(context)
    if (node == null) return text.getString(R.string.widget_finance_empty)
    return when (node.optString("state")) {
      "ready" -> {
        val amount = node.optString("text")
        if (node.optBoolean("stale")) "$amount · ${text.getString(R.string.widget_rates_stale)}" else amount
      }
      "needs-rate" -> text.getString(R.string.widget_needs_rate, node.optString("missing"))
      else -> text.getString(empty)
    }
  }

  private fun openIntent(context: Context, tab: String, request: Int): PendingIntent =
    PendingIntent.getBroadcast(
      context,
      request,
      Intent(context, HomeWidgetReceiver::class.java).setAction(ACTION_OPEN).putExtra(EXTRA_TAB, tab),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

  private fun bindRows(context: Context, views: RemoteViews, theme: WidgetTheme.Palette, title: String, lines: List<Line>, empty: String, tab: String, count: Int) {
    val open = PendingIntent.getBroadcast(
      context, if (tab == "reminders") 3 else 4,
      Intent(context, HomeWidgetReceiver::class.java).setAction(ACTION_OPEN).putExtra(EXTRA_TAB, tab),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    WidgetTheme.paint(views, theme)
    views.setTextColor(R.id.widget_title, theme.accent)
    intArrayOf(R.id.widget_row_1, R.id.widget_row_2, R.id.widget_row_3, R.id.widget_row_4).forEach { views.setTextColor(it, theme.ink) }
    views.setOnClickPendingIntent(R.id.widget_root, open)
    views.setTextViewText(R.id.widget_title, title)
    val shown = if (lines.isEmpty()) listOf(Line(0, "", empty, "")) else lines.take(count)
    val ids = intArrayOf(R.id.widget_row_1, R.id.widget_row_2, R.id.widget_row_3, R.id.widget_row_4)
    for (i in ids.indices) {
      val line = shown.getOrNull(i)
      if (line == null) {
        views.setViewVisibility(ids[i], View.GONE)
        views.setTextViewText(ids[i], "")
      } else {
        views.setViewVisibility(ids[i], View.VISIBLE)
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

  fun nextReminder(context: Context): String {
    val next = reminderLines(context).firstOrNull() ?: return ""
    return if (next.detail.isBlank()) next.title else "${next.detail}  ${next.title}"
  }

  private fun reminderLines(context: Context): List<Line> {
    val now = System.currentTimeMillis()
    return items(context)
      .filter { includeReminder(it, now) }
      .sortedBy { it.atMs }
      .map { Line(it.atMs, it.kind, it.title.ifBlank { AppLocale.wrap(context).getString(R.string.widget_fallback_reminder) }, clock(it.atMs)) }
  }

  private fun includeReminder(item: Item, now: Long): Boolean {
    if (item.kind != "reminder") return false
    if (item.personId.isNotEmpty() && item.personReminderId != "reach") {
      return personDateOnTimeList(item, now)
    }
    return item.atMs >= now - 60_000L
  }

  /** Person dates stay off the widget until the notify day, then through the event day. */
  private fun personDateOnTimeList(item: Item, nowMs: Long): Boolean {
    if (item.personId.isEmpty() || item.personReminderId == "reach") return true
    val today = startOfDay(nowMs)
    val notify = startOfDay(item.atMs)
    val event = (notify.clone() as Calendar).apply { add(Calendar.DATE, item.leadDays.coerceIn(0, 30)) }
    return !today.before(notify) && !today.after(event)
  }

  private fun startOfDay(ms: Long): Calendar =
    Calendar.getInstance().apply {
      timeInMillis = ms
      set(Calendar.HOUR_OF_DAY, 0)
      set(Calendar.MINUTE, 0)
      set(Calendar.SECOND, 0)
      set(Calendar.MILLISECOND, 0)
    }

  private fun scheduleLines(context: Context): List<Line> {
    val now = System.currentTimeMillis()
    val text = AppLocale.wrap(context)
    val timed = items(context).filter { it.kind == "alarm" || it.kind == "timer" }.map {
      val title = it.title.ifBlank { text.getString(if (it.kind == "timer") R.string.widget_fallback_countdown else R.string.widget_fallback_alarm) }
      val detail = if (it.kind == "timer") remaining(text, it.atMs, now) else clock(it.atMs)
      Line(it.atMs, it.kind, title, detail)
    }
    val horizon = now + 36L * 60L * 60L * 1000L
    val events = events(context, now - 60_000L, horizon).map {
      Line(it.startMs, "event", it.title.ifBlank { text.getString(R.string.widget_fallback_event) }, if (it.allDay) text.getString(R.string.widget_all_day) else clock(it.startMs))
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
          phone.add(Event(c.getString(0) ?: AppLocale.wrap(context).getString(R.string.widget_fallback_event), start, end, c.getInt(3) == 1))
        }
      }
    } catch (_: SecurityException) {
    }
    return (local + phone).sortedBy { it.startMs }
  }

  private fun clock(ms: Long): String =
    DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))

  private fun remaining(context: Context, atMs: Long, now: Long): String {
    val left = atMs - now
    if (left <= 0) return context.getString(R.string.widget_now)
    val mins = left / 60_000L
    val hours = mins / 60
    val m = mins % 60
    return if (hours > 0) "${hours}h ${m}m" else "${mins.coerceAtLeast(1)}m"
  }

  data class Item(
    val kind: String,
    val title: String,
    val atMs: Long,
    val personId: String = "",
    val personReminderId: String = "",
    val leadDays: Int = 0,
  )
  data class Event(val title: String, val startMs: Long, val endMs: Long, val allDay: Boolean)
  data class Line(val whenMs: Long, val kind: String, val title: String, val detail: String)
}

object AppData {
  fun readAgendaItems(context: Context): List<HomeWidgets.Item> {
    val root = readJson(agendaFile(context)) ?: return emptyList()
    val arr = root.optJSONArray("items") ?: return emptyList()
    return parseItems(arr)
  }

  fun settingsRoot(context: Context): org.json.JSONObject? = readJson(settingsFile(context))

  fun widgetFinance(context: Context): org.json.JSONObject? = readJson(firstExisting(context, "widget-finance.json"))

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
      out.add(
        HomeWidgets.Item(
          o.optString("kind"),
          o.optString("title"),
          o.optLong("atMs"),
          o.optString("personId"),
          o.optString("personReminderId"),
          o.optInt("leadDays"),
        ),
      )
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

  override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
    HomeWidgets.updateAll(context)
  }
}

class NetWorthWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }

  override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
    HomeWidgets.updateAll(context)
  }
}

class MonthWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }

  override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
    HomeWidgets.updateAll(context)
  }
}

class WindowWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }

  override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
    HomeWidgets.updateAll(context)
  }
}

class NextWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
    HomeWidgets.updateAll(context)
  }

  override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
    HomeWidgets.updateAll(context)
  }
}
