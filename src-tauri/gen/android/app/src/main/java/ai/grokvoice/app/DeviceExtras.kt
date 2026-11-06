package ai.grokvoice.app

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.BatteryManager
import android.os.Build
import android.provider.CallLog
import android.provider.ContactsContract
import android.provider.Settings
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.text.DateFormat
import java.util.Date

/** Phone status, Do Not Disturb, home, birthdays, and last call or text. No message bodies are kept. */
object DeviceExtras {
  fun notifications(context: Context): String {
    if (!NoticeListener.enabled(context)) {
      return fail("Notification access is off. Turn it on in Settings. Nothing was read.")
    }
    val rows = JSONArray()
    for (item in NoticeListener.current().take(12)) {
      rows.put(JSONObject().put("title", item.title).put("text", item.text))
    }
    val detail = if (rows.length() == 0) "No notifications are posted." else "${rows.length()} notifications are posted."
    return JSONObject().put("ok", true).put("notifications", rows).put("detail", detail).toString()
  }

  fun dnd(context: Context, mode: String): String {
    val nm = context.getSystemService(NotificationManager::class.java)
      ?: return fail("Do Not Disturb is not available on this phone.")
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M && !nm.isNotificationPolicyAccessGranted) {
      return fail("Do Not Disturb access is off. Turn it on in Settings. Nothing was changed.")
    }
    val filter = when (mode.trim().lowercase()) {
      "off", "all" -> NotificationManager.INTERRUPTION_FILTER_ALL
      "priority" -> NotificationManager.INTERRUPTION_FILTER_PRIORITY
      "alarms" -> NotificationManager.INTERRUPTION_FILTER_ALARMS
      "none", "total" -> NotificationManager.INTERRUPTION_FILTER_NONE
      else -> return fail("Use off, priority, alarms, or none.")
    }
    return try {
      nm.setInterruptionFilter(filter)
      ok("Do Not Disturb is ${mode.trim().lowercase()}.")
    } catch (_: SecurityException) {
      fail("Do Not Disturb access is off. Turn it on in Settings. Nothing was changed.")
    }
  }

  fun home(context: Context): String {
    val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      ok("Showing the home screen.")
    } catch (_: Exception) {
      fail("Could not open the home screen.")
    }
  }

  fun status(context: Context): String {
    val battery = context.getSystemService(BatteryManager::class.java)
      ?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
      ?: -1
    val alarm = context.getSystemService(AlarmManager::class.java)?.nextAlarmClock
    val batteryText = if (battery in 0..100) "Battery is $battery percent." else "Battery level is unavailable."
    val alarmText = if (alarm == null) {
      "No next alarm is set."
    } else {
      val whenText = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(alarm.triggerTime))
      "The next alarm is $whenText."
    }
    return JSONObject()
      .put("ok", true)
      .put("battery", if (battery in 0..100) battery else JSONObject.NULL)
      .put("nextAlarmMs", alarm?.triggerTime ?: JSONObject.NULL)
      .put("detail", "$batteryText $alarmText")
      .toString()
  }

  fun access(context: Context): String {
    val nm = context.getSystemService(NotificationManager::class.java)
    val dnd = if (nm != null && (Build.VERSION.SDK_INT < Build.VERSION_CODES.M || nm.isNotificationPolicyAccessGranted)) "granted" else "denied"
    return JSONObject()
      .put("notificationListener", if (NoticeListener.enabled(context)) "granted" else "denied")
      .put("dnd", dnd)
      .put("callLog", if (granted(context, Manifest.permission.READ_CALL_LOG)) "granted" else "denied")
      .put("sms", if (granted(context, Manifest.permission.READ_SMS)) "granted" else "denied")
      .toString()
  }

  fun openAccess(context: Context, kind: String): String {
    val action = when (kind) {
      "notifications" -> Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS
      "dnd" -> Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS
      else -> return fail("Unknown settings page.")
    }
    val intent = Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      ok("Opened system settings. Allow access there.")
    } catch (_: Exception) {
      fail("Could not open system settings.")
    }
  }

  fun birthday(context: Context, contactId: String): String {
    if (!granted(context, Manifest.permission.READ_CONTACTS)) {
      return fail("Contacts permission is not granted.")
    }
    val cursor = context.contentResolver.query(
      ContactsContract.Data.CONTENT_URI,
      arrayOf(ContactsContract.CommonDataKinds.Event.START_DATE),
      "${ContactsContract.Data.CONTACT_ID} = ? AND ${ContactsContract.Data.MIMETYPE} = ? AND ${ContactsContract.CommonDataKinds.Event.TYPE} = ?",
      arrayOf(contactId, ContactsContract.CommonDataKinds.Event.CONTENT_ITEM_TYPE, ContactsContract.CommonDataKinds.Event.TYPE_BIRTHDAY.toString()),
      null
    ) ?: return fail("That contact has no birthday on this phone.")
    cursor.use {
      if (!it.moveToFirst()) return fail("That contact has no birthday on this phone.")
      val raw = it.getString(0) ?: return fail("That contact has no birthday on this phone.")
      val date = normalizeBirthday(raw) ?: return fail("That birthday could not be read.")
      return JSONObject().put("ok", true).put("date", date).put("detail", "Birthday is $date.").toString()
    }
  }

  fun lastContact(context: Context, phone: String, name: String): String {
    val callOk = granted(context, Manifest.permission.READ_CALL_LOG)
    val smsOk = granted(context, Manifest.permission.READ_SMS)
    if (!callOk && !smsOk) {
      return fail("Call and text history is off. Allow it in Settings if you want last-contacted dates. Nothing was read.")
    }
    val digits = phone.filter { it.isDigit() }
    var best = 0L
    var source = ""
    if (callOk) {
      val call = latestCall(context, digits, name)
      if (call > best) {
        best = call
        source = "call"
      }
    }
    if (smsOk) {
      val text = latestSms(context, digits)
      if (text > best) {
        best = text
        source = "text"
      }
    }
    if (best <= 0L) return fail("No recent call or text matched that person.")
    val whenText = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(best))
    return JSONObject().put("ok", true).put("atMs", best).put("source", source).put("detail", "Last $source was $whenText.").toString()
  }

  private fun latestCall(context: Context, digits: String, name: String): Long {
    val cursor = context.contentResolver.query(
      CallLog.Calls.CONTENT_URI,
      arrayOf(CallLog.Calls.DATE, CallLog.Calls.NUMBER, CallLog.Calls.CACHED_NAME),
      null,
      null,
      "${CallLog.Calls.DATE} DESC"
    ) ?: return 0L
    cursor.use {
      val dateIdx = it.getColumnIndex(CallLog.Calls.DATE)
      val numberIdx = it.getColumnIndex(CallLog.Calls.NUMBER)
      val nameIdx = it.getColumnIndex(CallLog.Calls.CACHED_NAME)
      var seen = 0
      while (it.moveToNext() && seen < 80) {
        seen++
        val number = if (numberIdx >= 0) it.getString(numberIdx) ?: "" else ""
        val cached = if (nameIdx >= 0) it.getString(nameIdx) ?: "" else ""
        val numberHit = digits.length >= 6 && number.filter { ch -> ch.isDigit() }.endsWith(digits.takeLast(8))
        val nameHit = name.length >= 2 && cached.contains(name, ignoreCase = true)
        if (numberHit || nameHit) {
          return if (dateIdx >= 0) it.getLong(dateIdx) else 0L
        }
      }
    }
    return 0L
  }

  private fun latestSms(context: Context, digits: String): Long {
    if (digits.length < 6) return 0L
    val cursor = context.contentResolver.query(
      android.net.Uri.parse("content://sms"),
      arrayOf("date", "address"),
      null,
      null,
      "date DESC"
    ) ?: return 0L
    cursor.use {
      val dateIdx = it.getColumnIndex("date")
      val addressIdx = it.getColumnIndex("address")
      var seen = 0
      while (it.moveToNext() && seen < 80) {
        seen++
        val address = if (addressIdx >= 0) it.getString(addressIdx) ?: "" else ""
        if (address.filter { ch -> ch.isDigit() }.endsWith(digits.takeLast(8))) {
          return if (dateIdx >= 0) it.getLong(dateIdx) else 0L
        }
      }
    }
    return 0L
  }

  private fun normalizeBirthday(raw: String): String? {
    val text = raw.trim()
    val parts = text.split("-", ".", "/")
    if (parts.size >= 3) {
      val year = parts[0].toIntOrNull()
      val month = parts[parts.size - 2].toIntOrNull()
      val day = parts[parts.size - 1].toIntOrNull()
      if (month != null && day != null && month in 1..12 && day in 1..31) {
        val md = "%02d-%02d".format(month, day)
        return if (year != null && year > 1800) "%04d-%s".format(year, md) else md
      }
    }
    if (text.startsWith("--") && text.length >= 7) {
      return normalizeBirthday(text.removePrefix("--"))
    }
    return null
  }

  private fun granted(context: Context, permission: String): Boolean {
    return ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
  }

  private fun ok(detail: String) = JSONObject().put("ok", true).put("detail", detail).toString()
  private fun fail(error: String) = JSONObject().put("ok", false).put("error", error).toString()
}
