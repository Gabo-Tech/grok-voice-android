package ai.grokvoice.app

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ContentUris
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioManager
import android.net.Uri
import android.os.Build
import android.provider.ContactsContract
import android.view.KeyEvent
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject

/** Media keys, volume, and drafts that open another app. Sending still happens there. */
object HandOff {
  fun media(context: Context, action: String): String {
    val code = when (action) {
      "play" -> KeyEvent.KEYCODE_MEDIA_PLAY
      "pause" -> KeyEvent.KEYCODE_MEDIA_PAUSE
      "stop" -> KeyEvent.KEYCODE_MEDIA_STOP
      "next" -> KeyEvent.KEYCODE_MEDIA_NEXT
      "previous" -> KeyEvent.KEYCODE_MEDIA_PREVIOUS
      else -> return fail("Use play, pause, stop, next, or previous.")
    }
    val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val now = android.os.SystemClock.uptimeMillis()
    audio.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0))
    audio.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0))
    val spoken = when (action) {
      "play" -> "Playing."
      "pause" -> "Paused."
      "stop" -> "Stopped."
      "next" -> "Next track."
      "previous" -> "Previous track."
      else -> "Done."
    }
    return ok(spoken)
  }

  fun volume(context: Context, direction: String, steps: Int): String {
    val adjust = when (direction) {
      "up" -> AudioManager.ADJUST_RAISE
      "down" -> AudioManager.ADJUST_LOWER
      else -> return fail("Use up or down.")
    }
    val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val n = steps.coerceIn(1, 5)
    repeat(n) {
      audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, adjust, AudioManager.FLAG_SHOW_UI)
    }
    return ok(if (direction == "up") "Volume up." else "Volume down.")
  }

  fun open(context: Context, kind: String, query: String, url: String, phone: String, text: String, to: String, subject: String, body: String): String {
    val intent = try {
      when (kind) {
        "youtube" -> youtube(query, url)
        "browser" -> browser(query, url)
        "whatsapp" -> whatsapp(phone, text)
        "signal" -> signal(phone, text)
        "email" -> email(to, subject, body)
        "call" -> dial(phone)
        "sms" -> sms(phone, text)
        else -> return fail("Unknown app.")
      }
    } catch (e: IllegalArgumentException) {
      return fail(e.message ?: "Missing details.")
    }
    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    val detail = openedDetail(kind)
    return try {
      context.startActivity(intent)
      ok(detail)
    } catch (_: Exception) {
      intent.setPackage(null)
      try {
        context.startActivity(intent)
        ok(detail)
      } catch (_: Exception) {
        notify(context, intent, kind)
        ok("Posted a notification to open $kind. Nothing is sent until they tap it.")
      }
    }
  }

  private fun openedDetail(kind: String) = when (kind) {
    "call" -> "Opened the dialer. The user places the call there."
    "sms" -> "Opened the text. The user still taps Send."
    else -> "Opened $kind. The user still confirms send there."
  }

  fun contacts(context: Context, name: String): String {
    if (ContextCompat.checkSelfPermission(context, Manifest.permission.READ_CONTACTS) != PackageManager.PERMISSION_GRANTED) {
      return fail("Contacts permission is not granted.")
    }
    val needle = name.trim().lowercase()
    if (needle.length < 2) return fail("Say a contact name.")
    val found = linkedMapOf<String, JSONObject>()
    readPeople(context, ContactsContract.CommonDataKinds.Phone.CONTENT_URI, ContactsContract.CommonDataKinds.Phone.CONTACT_ID, ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME, ContactsContract.CommonDataKinds.Phone.NUMBER, "phone", needle, found)
    readPeople(context, ContactsContract.CommonDataKinds.Email.CONTENT_URI, ContactsContract.CommonDataKinds.Email.CONTACT_ID, ContactsContract.CommonDataKinds.Email.DISPLAY_NAME, ContactsContract.CommonDataKinds.Email.ADDRESS, "email", needle, found)
    val matches = JSONArray()
    found.values.take(6).forEach { matches.put(it) }
    return if (matches.length() == 0) {
      ok("No contact matched $name.")
    } else {
      JSONObject().put("ok", true).put("matches", matches).put("detail", "Found ${matches.length()} contact match(es). Speak the name and number, then wait for yes before opening an app.").toString()
    }
  }

  fun listContacts(context: Context, query: String): String {
    if (!canReadContacts(context)) return fail("Contacts permission is not granted.")
    val needle = query.trim().lowercase()
    val cursor = context.contentResolver.query(
      ContactsContract.Contacts.CONTENT_URI,
      arrayOf(ContactsContract.Contacts._ID, ContactsContract.Contacts.DISPLAY_NAME_PRIMARY),
      null,
      null,
      "${ContactsContract.Contacts.DISPLAY_NAME_PRIMARY} COLLATE LOCALIZED ASC"
    ) ?: return JSONObject().put("ok", true).put("contacts", JSONArray()).toString()
    val contacts = JSONArray()
    cursor.use {
      val idIdx = it.getColumnIndex(ContactsContract.Contacts._ID)
      val nameIdx = it.getColumnIndex(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY)
      if (idIdx < 0 || nameIdx < 0) return fail("Could not read phone contacts.")
      while (it.moveToNext() && contacts.length() < 40) {
        val name = it.getString(nameIdx) ?: continue
        if (needle.isNotEmpty() && !name.lowercase().contains(needle)) continue
        val id = it.getString(idIdx) ?: continue
        contacts.put(contactRow(context, id, name))
      }
    }
    return JSONObject().put("ok", true).put("contacts", contacts).toString()
  }

  fun contact(context: Context, id: String): String {
    if (!canReadContacts(context)) return fail("Contacts permission is not granted.")
    val clean = id.trim()
    if (clean.isEmpty()) return fail("That contact has no phone link.")
    val name = lookupName(context, clean) ?: return fail("That phone contact is not on this device.")
    return JSONObject().put("ok", true).put("matches", JSONArray().put(contactRow(context, clean, name))).toString()
  }

  fun viewContact(context: Context, id: String): String {
    val parsed = id.trim().toLongOrNull() ?: return fail("That contact has no phone link.")
    val uri = ContentUris.withAppendedId(ContactsContract.Contacts.CONTENT_URI, parsed)
    val intent = Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      ok("Opened the phone contact. Nothing there was changed.")
    } catch (_: Exception) {
      fail("Could not open the phone contact.")
    }
  }

  private fun canReadContacts(context: Context): Boolean {
    return ContextCompat.checkSelfPermission(context, Manifest.permission.READ_CONTACTS) == PackageManager.PERMISSION_GRANTED
  }

  private fun readPeople(context: Context, uri: Uri, idCol: String, nameCol: String, valueCol: String, field: String, needle: String, found: LinkedHashMap<String, JSONObject>) {
    val cursor = context.contentResolver.query(uri, arrayOf(idCol, nameCol, valueCol), null, null, "$nameCol ASC") ?: return
    cursor.use {
      val idIdx = it.getColumnIndex(idCol)
      val nameIdx = it.getColumnIndex(nameCol)
      val valueIdx = it.getColumnIndex(valueCol)
      if (nameIdx < 0 || valueIdx < 0) return
      while (it.moveToNext() && found.size < 6) {
        val person = it.getString(nameIdx) ?: continue
        if (!person.lowercase().contains(needle)) continue
        val value = it.getString(valueIdx) ?: continue
        val id = if (idIdx >= 0) it.getString(idIdx) ?: "" else ""
        val row = found.getOrPut(person.lowercase()) { JSONObject().put("name", person).put("id", id) }
        if (!row.has(field)) row.put(field, value)
      }
    }
  }

  private fun contactRow(context: Context, id: String, name: String): JSONObject {
    return JSONObject()
      .put("id", id)
      .put("name", name)
      .put("phone", firstValue(context, ContactsContract.CommonDataKinds.Phone.CONTENT_URI, ContactsContract.CommonDataKinds.Phone.NUMBER, id))
      .put("email", firstValue(context, ContactsContract.CommonDataKinds.Email.CONTENT_URI, ContactsContract.CommonDataKinds.Email.ADDRESS, id))
  }

  private fun firstValue(context: Context, uri: Uri, column: String, contactId: String): String {
    val cursor = context.contentResolver.query(
      uri,
      arrayOf(column),
      "${ContactsContract.Data.CONTACT_ID} = ?",
      arrayOf(contactId),
      null
    ) ?: return ""
    cursor.use {
      val idx = it.getColumnIndex(column)
      if (idx < 0 || !it.moveToFirst()) return ""
      return it.getString(idx) ?: ""
    }
  }

  private fun lookupName(context: Context, id: String): String? {
    val cursor = context.contentResolver.query(
      ContactsContract.Contacts.CONTENT_URI,
      arrayOf(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY),
      "${ContactsContract.Contacts._ID} = ?",
      arrayOf(id),
      null
    ) ?: return null
    cursor.use {
      if (!it.moveToFirst()) return null
      return it.getString(0)
    }
  }

  private fun browser(query: String, url: String): Intent {
    val raw = url.trim().ifEmpty { query.trim() }
    if (raw.isEmpty()) throw IllegalArgumentException("Say a site or a search.")
    val target = when {
      raw.startsWith("http://") || raw.startsWith("https://") -> raw
      raw.contains(' ') -> "https://www.google.com/search?q=" + Uri.encode(raw)
      else -> "https://$raw"
    }
    return Intent(Intent.ACTION_VIEW, Uri.parse(target))
  }

  private fun youtube(query: String, url: String): Intent {
    val target = if (url.startsWith("http")) url.trim()
    else {
      val q = query.trim()
      if (q.isEmpty()) throw IllegalArgumentException("Say what to play on YouTube.")
      "https://www.youtube.com/results?search_query=" + Uri.encode(q)
    }
    return Intent(Intent.ACTION_VIEW, Uri.parse(target)).setPackage("com.google.android.youtube")
  }

  private fun whatsapp(phone: String, text: String): Intent {
    val digits = phone.filter { it.isDigit() }
    if (digits.length < 8) throw IllegalArgumentException("WhatsApp needs a phone number with country code.")
    val uri = "https://wa.me/$digits?text=" + Uri.encode(text.trim())
    return Intent(Intent.ACTION_VIEW, Uri.parse(uri)).setPackage("com.whatsapp")
  }

  private fun signal(phone: String, text: String): Intent {
    val digits = phone.filter { it.isDigit() }
    if (digits.length < 8) throw IllegalArgumentException("Signal needs a phone number with country code.")
    return Intent(Intent.ACTION_VIEW, Uri.parse("https://signal.me/#p/+$digits")).apply {
      setPackage("org.thoughtcrime.securesms")
      putExtra(Intent.EXTRA_TEXT, text.trim())
    }
  }

  private fun dial(phone: String): Intent {
    return Intent(Intent.ACTION_DIAL, Uri.parse("tel:${phoneNumber(phone)}"))
  }

  private fun sms(phone: String, text: String): Intent {
    val number = phoneNumber(phone)
    val body = text.trim()
    val uri = if (body.isEmpty()) "sms:$number" else "sms:$number?body=${Uri.encode(body)}"
    return Intent(Intent.ACTION_SENDTO, Uri.parse(uri)).apply {
      if (body.isNotEmpty()) putExtra("sms_body", body)
    }
  }

  private fun phoneNumber(phone: String): String {
    val digits = phone.filter { it.isDigit() }
    if (digits.length < 3) throw IllegalArgumentException("A phone number is required.")
    return if (phone.contains('+')) "+$digits" else digits
  }

  private fun email(to: String, subject: String, body: String): Intent {
    val address = to.trim()
    if (!address.contains("@")) throw IllegalArgumentException("Email needs an address.")
    val uri = "mailto:" + Uri.encode(address) +
      "?subject=" + Uri.encode(subject.trim()) +
      "&body=" + Uri.encode(body.trim())
    return Intent(Intent.ACTION_SENDTO, Uri.parse(uri))
  }

  private fun notify(context: Context, intent: Intent, kind: String) {
    val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      if (nm.getNotificationChannel("handoff") == null) {
        nm.createNotificationChannel(NotificationChannel("handoff", "Open another app", NotificationManager.IMPORTANCE_HIGH))
      }
    }
    val pending = PendingIntent.getActivity(
      context,
      kind.hashCode(),
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      Notification.Builder(context, "handoff")
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(context)
    }
    nm.notify(
      4300 + (kind.hashCode() and 0xff),
      builder
        .setSmallIcon(android.R.drawable.ic_menu_send)
        .setContentTitle("Open $kind")
        .setContentText("Tap to open the draft")
        .setContentIntent(pending)
        .setAutoCancel(true)
        .build()
    )
  }

  private fun ok(detail: String) = JSONObject().put("ok", true).put("detail", detail).toString()

  private fun fail(error: String) = JSONObject().put("ok", false).put("error", error).toString()
}
