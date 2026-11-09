package ai.grokvoice.app

import android.app.Activity
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build
import android.provider.CalendarContract
import org.json.JSONArray
import org.json.JSONObject
import java.util.TimeZone

/** Alarms, timers, reminders, and calendar. The Rust side keeps the list; this rings. */
object PhoneActions {
  private const val PREFS = "grok_agenda"
  private const val KEY = "scheduled"
  private const val ALARM_CHANNEL = "grok_alarms"
  private const val REMINDER_CHANNEL = "grok_reminders"

  /** Alarms stay in Grok Voice so Time, list_reminders, and spoken alerts can see them. */
  fun setAlarm(activity: Activity, hour: Int, minute: Int, label: String, id: Int, atMs: Long): String {
    val scheduled = schedule(activity.applicationContext, id, "alarm", label, "", atMs)
    if (!JSONObject(scheduled).optBoolean("ok")) return scheduled
    return """{"ok":true,"via":"local","detail":"Alarm saved in this app's Time list for %02d:%02d."}"""
      .format(hour, minute)
  }

  /** Countdowns stay in Grok Voice so Time, list_reminders, and spoken alerts can see them. */
  fun setTimer(activity: Activity, seconds: Int, label: String, id: Int, atMs: Long): String {
    val scheduled = schedule(activity.applicationContext, id, "timer", label, "", atMs)
    if (!JSONObject(scheduled).optBoolean("ok")) return scheduled
    val mins = seconds / 60
    val secs = seconds % 60
    val spoken = if (mins > 0 && secs == 0) "$mins min" else "${seconds}s"
    return """{"ok":true,"via":"local","detail":"Countdown saved in this app's Time list for $spoken."}"""
  }

  fun schedule(context: Context, id: Int, kind: String, title: String, note: String, atMs: Long): String {
    if (atMs <= System.currentTimeMillis()) {
      return """{"ok":false,"error":"That time is in the past."}"""
    }
    upsert(context, id, kind, title, note, atMs)
    arm(context, id, kind, title, note, atMs)
    return """{"ok":true,"via":"local"}"""
  }

  fun storedItems(context: Context): List<HomeWidgets.Item> {
    val arr = read(context)
    val out = ArrayList<HomeWidgets.Item>()
    for (i in 0 until arr.length()) {
      val o = arr.optJSONObject(i) ?: continue
      out.add(HomeWidgets.Item(o.optString("kind"), o.optString("title"), o.optLong("atMs")))
    }
    return out
  }

  fun cancel(context: Context, id: Int) {
    val am = context.getSystemService(AlarmManager::class.java)
    val pi = pending(context, id, "reminder", "", "")
    am.cancel(pi)
    pi.cancel()
    val arr = read(context)
    val next = JSONArray()
    for (i in 0 until arr.length()) {
      val o = arr.getJSONObject(i)
      if (o.optInt("id") != id) next.put(o)
    }
    write(context, next)
  }

  fun rescheduleAll(context: Context) {
    val arr = read(context)
    val now = System.currentTimeMillis()
    val keep = JSONArray()
    for (i in 0 until arr.length()) {
      val o = arr.getJSONObject(i)
      val at = o.optLong("atMs")
      if (at <= now) continue
      keep.put(o)
      arm(context, o.optInt("id"), o.optString("kind"), o.optString("title"), o.optString("note"), at)
    }
    write(context, keep)
  }

  fun notify(context: Context, id: Int, kind: String, title: String, note: String) {
    val alarm = kind == "alarm" || kind == "timer" || kind == "countdown"
    val event = kind == "event" || kind == "calendar"
    val audibleChannel = if (alarm) ALARM_CHANNEL else REMINDER_CHANNEL
    val spoken = SpokenAlerts.isSpoken(context)
    // Spoken posts on a silent sibling so the existing audible channel, and any
    // sound the user set on it, stays exactly as it is for Normal delivery.
    val channel = if (spoken) spokenChannelId(audibleChannel) else audibleChannel
    val nm = context.getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      if (spoken) {
        ensureSpokenChannel(nm, channel, audibleChannel, alarm)
      } else if (nm.getNotificationChannel(channel) == null) {
        val importance = NotificationManager.IMPORTANCE_HIGH
        val created = NotificationChannel(
          channel,
          if (alarm) "Alarms and timers" else "Reminders",
          importance
        )
        if (alarm) {
          val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ALARM)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
          created.setSound(RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM), attrs)
          created.enableVibration(true)
        }
        nm.createNotificationChannel(created)
      }
    }
    val open = PendingIntent.getActivity(
      context, 0,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val text = if (note.isBlank()) kind.replaceFirstChar { it.uppercase() } else note
    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      Notification.Builder(context, channel)
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(context)
    }
      .setSmallIcon(android.R.drawable.ic_lock_idle_alarm)
      .setContentTitle(title.ifBlank { "Grok" })
      .setContentText(text)
      .setContentIntent(open)
      .setAutoCancel(true)
      .setCategory(
        when {
          alarm -> Notification.CATEGORY_ALARM
          event -> Notification.CATEGORY_EVENT
          else -> Notification.CATEGORY_REMINDER
        }
      )
    if (spoken) {
      // The SDK stubs omit Notification.Builder.setSilent. This is the same
      // override NotificationCompat uses: the post does not chime, and a sound
      // the user later sets on the channel is left in place for speech to wait out.
      builder.setSound(null)
      builder.setVibrate(null)
      builder.setGroup("silent")
      builder.setGroupAlertBehavior(Notification.GROUP_ALERT_SUMMARY)
    }
    val n = builder.build()
    val posted = try {
      nm.notify(if (id == 0) 1 else id, n)
      true
    } catch (_: SecurityException) {
      // Notifications not allowed. The schedule itself still happened.
      false
    }
    if (posted) {
      val category = when {
        alarm -> "alarm"
        event -> "event"
        else -> "reminder"
      }
      val bannerAudible = spoken && channelHasSound(nm, channel)
      SpokenAlerts.enqueue(context, id, kind, title.ifBlank { "Grok" }, text, channel, category, bannerAudible)
    }
  }

  private fun spokenChannelId(base: String): String = "${base}_spoken"

  /**
   * New channel, so creating it with no sound does not rewrite grok_alarms or
   * grok_reminders. Importance and Do Not Disturb bypass are copied from the
   * audible channel when that one already exists. A later user sound on this
   * channel is left in place; speech waits for it.
   */
  private fun ensureSpokenChannel(nm: NotificationManager, id: String, audibleId: String, alarm: Boolean) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val audible = nm.getNotificationChannel(audibleId)
    val existing = nm.getNotificationChannel(id)
    if (existing != null) {
      if (audible?.canBypassDnd() == true && !existing.canBypassDnd()) {
        try {
          existing.setBypassDnd(true)
          nm.createNotificationChannel(existing)
        } catch (_: SecurityException) {
        }
      }
      return
    }
    val importance = if (audible?.importance == NotificationManager.IMPORTANCE_NONE) {
      NotificationManager.IMPORTANCE_NONE
    } else {
      NotificationManager.IMPORTANCE_HIGH
    }
    val created = NotificationChannel(
      id,
      if (alarm) "Alarms and timers, spoken" else "Reminders, spoken",
      importance
    )
    created.setSound(null, null)
    created.enableVibration(false)
    created.description = "Visual banner while this alert is spoken. No chime."
    if (audible?.canBypassDnd() == true) {
      try {
        created.setBypassDnd(true)
      } catch (_: SecurityException) {
      }
    }
    nm.createNotificationChannel(created)
  }

  private fun channelHasSound(nm: NotificationManager, channelId: String): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return false
    val channel = nm.getNotificationChannel(channelId) ?: return false
    if (channel.importance < NotificationManager.IMPORTANCE_DEFAULT) return false
    return channel.sound != null
  }

  fun listEvents(context: Context, startMs: Long, endMs: Long): String {
    val cr = context.contentResolver
    val builder = CalendarContract.Instances.CONTENT_URI.buildUpon()
    android.content.ContentUris.appendId(builder, startMs)
    android.content.ContentUris.appendId(builder, endMs)
    val uri = builder.build()
    val proj = arrayOf(
      CalendarContract.Instances.EVENT_ID,
      CalendarContract.Instances.TITLE,
      CalendarContract.Instances.BEGIN,
      CalendarContract.Instances.END,
      CalendarContract.Instances.EVENT_LOCATION,
      CalendarContract.Instances.ALL_DAY
    )
    val rules = eventRules(context)
    val events = JSONArray()
    var truncated = false
    try {
      cr.query(uri, proj, null, null, "${CalendarContract.Instances.BEGIN} ASC")?.use { c ->
        var n = 0
        while (c.moveToNext()) {
          if (n >= 400) {
            truncated = true
            break
          }
          n++
          val id = c.getLong(0)
          val end = c.getLong(3).let { if (it == 0L) c.getLong(2) + 3_600_000L else it }
          val row = JSONObject()
            .put("id", id.toString())
            .put("title", c.getString(1) ?: "(untitled)")
            .put("startMs", c.getLong(2))
            .put("endMs", end)
            .put("location", c.getString(4) ?: "")
            .put("allDay", c.getInt(5) == 1)
            .put("source", "phone")
          rules[id]?.let { row.put("rrule", it) }
          events.put(row)
        }
      }
    } catch (e: SecurityException) {
      return """{"ok":false,"error":"Calendar permission is not granted."}"""
    }
    return JSONObject().put("ok", true).put("events", events).put("truncated", truncated).toString()
  }

  private fun eventRules(context: Context): Map<Long, String> {
    val out = HashMap<Long, String>()
    try {
      context.contentResolver.query(
        CalendarContract.Events.CONTENT_URI,
        arrayOf(CalendarContract.Events._ID, CalendarContract.Events.RRULE),
        "${CalendarContract.Events.RRULE} IS NOT NULL AND ${CalendarContract.Events.RRULE} != ''",
        null,
        null
      )?.use { c ->
        var n = 0
        while (c.moveToNext() && n < 300) {
          val rule = c.getString(1) ?: continue
          if (rule.isBlank()) continue
          n++
          out[c.getLong(0)] = rule
        }
      }
    } catch (_: SecurityException) {
    }
    return out
  }

  fun createEvent(context: Context, title: String, startMs: Long, endMs: Long, location: String, rrule: String): String {
    val calId = writableCalendar(context)
      ?: return """{"ok":false,"error":"No writable calendar on this phone. Add a Google account or allow calendar write."}"""
    val values = android.content.ContentValues().apply {
      put(CalendarContract.Events.CALENDAR_ID, calId)
      put(CalendarContract.Events.TITLE, title)
      put(CalendarContract.Events.DTSTART, startMs)
      put(CalendarContract.Events.EVENT_TIMEZONE, TimeZone.getDefault().id)
      if (rrule.isNotBlank()) {
        put(CalendarContract.Events.RRULE, rrule)
        put(CalendarContract.Events.DURATION, duration(startMs, endMs))
      } else {
        put(CalendarContract.Events.DTEND, endMs)
      }
      if (location.isNotBlank()) put(CalendarContract.Events.EVENT_LOCATION, location)
    }
    return try {
      val uri = context.contentResolver.insert(CalendarContract.Events.CONTENT_URI, values)
        ?: return """{"ok":false,"error":"The calendar did not accept the event."}"""
      val id = uri.lastPathSegment ?: ""
      JSONObject().put("ok", true).put("id", id).put("source", "phone").put("uri", uri.toString()).toString()
    } catch (e: SecurityException) {
      """{"ok":false,"error":"Calendar permission is not granted."}"""
    } catch (e: Exception) {
      JSONObject().put("ok", false).put("error", e.message ?: "calendar write failed").toString()
    }
  }

  fun updateEvent(context: Context, id: String, title: String, startMs: Long, endMs: Long, location: String, rrule: String): String {
    val eventId = id.toLongOrNull()
      ?: return """{"ok":false,"error":"Bad calendar event id."}"""
    val values = android.content.ContentValues().apply {
      if (title.isNotBlank()) put(CalendarContract.Events.TITLE, title)
      if (startMs > 0) put(CalendarContract.Events.DTSTART, startMs)
      put(CalendarContract.Events.EVENT_TIMEZONE, TimeZone.getDefault().id)
      put(CalendarContract.Events.EVENT_LOCATION, location)
      if (rrule.isNotBlank()) {
        put(CalendarContract.Events.RRULE, rrule)
        put(CalendarContract.Events.DURATION, duration(startMs, endMs))
        putNull(CalendarContract.Events.DTEND)
      } else if (endMs > 0) {
        put(CalendarContract.Events.DTEND, endMs)
        put(CalendarContract.Events.RRULE, "")
      }
    }
    return try {
      val uri = android.content.ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI, eventId)
      val n = context.contentResolver.update(uri, values, null, null)
      if (n <= 0) """{"ok":false,"error":"Could not update that calendar event."}"""
      else JSONObject().put("ok", true).put("id", id).put("source", "phone").toString()
    } catch (e: SecurityException) {
      """{"ok":false,"error":"Calendar permission is not granted."}"""
    } catch (e: Exception) {
      JSONObject().put("ok", false).put("error", e.message ?: "calendar update failed").toString()
    }
  }

  fun deleteEvent(context: Context, id: String): String {
    val eventId = id.toLongOrNull()
      ?: return """{"ok":false,"error":"Bad calendar event id."}"""
    return try {
      val uri = android.content.ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI, eventId)
      val n = context.contentResolver.delete(uri, null, null)
      if (n <= 0) """{"ok":false,"error":"Could not delete that calendar event."}"""
      else JSONObject().put("ok", true).put("id", id).toString()
    } catch (e: SecurityException) {
      """{"ok":false,"error":"Calendar permission is not granted."}"""
    } catch (e: Exception) {
      JSONObject().put("ok", false).put("error", e.message ?: "calendar delete failed").toString()
    }
  }

  private fun duration(startMs: Long, endMs: Long): String {
    val seconds = ((endMs - startMs).coerceAtLeast(60_000L) / 1000L)
    return "PT${seconds}S"
  }

  private fun writableCalendar(context: Context): Long? {
    val uri = CalendarContract.Calendars.CONTENT_URI
    val proj = arrayOf(
      CalendarContract.Calendars._ID,
      CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL,
      CalendarContract.Calendars.VISIBLE,
      CalendarContract.Calendars.IS_PRIMARY
    )
    var primary: Long? = null
    var visible: Long? = null
    var any: Long? = null
    try {
      context.contentResolver.query(uri, proj, null, null, null)?.use { c ->
        while (c.moveToNext()) {
          val id = c.getLong(0)
          val level = if (c.isNull(1)) CalendarContract.Calendars.CAL_ACCESS_OWNER else c.getInt(1)
          if (level < CalendarContract.Calendars.CAL_ACCESS_CONTRIBUTOR) continue
          val isVisible = c.isNull(2) || c.getInt(2) == 1
          val isPrimary = !c.isNull(3) && c.getInt(3) == 1
          if (any == null) any = id
          if (isVisible && visible == null) visible = id
          if (isPrimary && isVisible) primary = id
        }
      }
    } catch (_: SecurityException) {
      return null
    }
    return primary ?: visible ?: any
  }

  private fun arm(context: Context, id: Int, kind: String, title: String, note: String, atMs: Long) {
    val am = context.getSystemService(AlarmManager::class.java)
    val pi = pending(context, id, kind, title, note)
    val show = PendingIntent.getActivity(
      context, id + 100_000,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    try {
      if (kind == "alarm" || kind == "timer") {
        am.setAlarmClock(AlarmManager.AlarmClockInfo(atMs, show), pi)
      } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && !am.canScheduleExactAlarms()) {
        am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pi)
      } else {
        am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pi)
      }
    } catch (_: SecurityException) {
      am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pi)
    }
  }

  private fun pending(context: Context, id: Int, kind: String, title: String, note: String): PendingIntent {
    val intent = Intent(context, ReminderReceiver::class.java)
      .putExtra("id", id)
      .putExtra("kind", kind)
      .putExtra("title", title)
      .putExtra("note", note)
    return PendingIntent.getBroadcast(
      context, id, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )
  }

  private fun upsert(context: Context, id: Int, kind: String, title: String, note: String, atMs: Long) {
    val arr = read(context)
    val next = JSONArray()
    for (i in 0 until arr.length()) {
      val o = arr.getJSONObject(i)
      if (o.optInt("id") != id) next.put(o)
    }
    next.put(JSONObject().put("id", id).put("kind", kind).put("title", title).put("note", note).put("atMs", atMs))
    write(context, next)
  }

  private fun read(context: Context): JSONArray {
    val raw = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, null) ?: return JSONArray()
    return try {
      JSONArray(raw)
    } catch (_: Exception) {
      JSONArray()
    }
  }

  private fun write(context: Context, arr: JSONArray) {
    context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY, arr.toString()).apply()
    HomeWidgets.updateAll(context)
  }
}
