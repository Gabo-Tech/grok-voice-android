package ai.grokvoice.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.LocaleList
import androidx.core.app.NotificationCompat
import java.util.Locale

/**
 * Tells the user a newer sideload is published. Tapping opens a browser.
 * The APK is not downloaded or installed from here.
 *
 * Allowed links match `update_url_allowed` in `src-tauri/src/updates.rs`.
 */
object UpdateNotice {
  private const val CHANNEL_ID = "grok_updates"
  private const val NOTICE_ID = 91024
  private val versionToken = Regex("""^v?\d{1,6}(?:\.\d{1,6}){0,3}$""")

  fun show(context: Context, localeTag: String, version: String, url: String) {
    if (!allowed(url)) return
    val safeVersion = version.filter { it.isDigit() || it == '.' }.take(32)
    if (safeVersion.isEmpty()) return
    val localized = localeContext(context, localeTag)
    val nm = context.getSystemService(NotificationManager::class.java) ?: return
    ensureChannel(localized, nm)
    val open = pendingView(context, url, NOTICE_ID) ?: return
    val action = pendingView(context, url, NOTICE_ID + 1) ?: return
    val text = localized.getString(R.string.update_text, safeVersion)
    val notification = NotificationCompat.Builder(context, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_sys_download_done)
      .setContentTitle(localized.getString(R.string.update_title))
      .setContentText(text)
      .setStyle(NotificationCompat.BigTextStyle().bigText(text))
      .setContentIntent(open)
      .addAction(android.R.drawable.stat_sys_download_done, localized.getString(R.string.update_action), action)
      .setAutoCancel(true)
      .setOnlyAlertOnce(true)
      .setCategory(NotificationCompat.CATEGORY_STATUS)
      .setPriority(NotificationCompat.PRIORITY_DEFAULT)
      .build()
    try {
      nm.notify(NOTICE_ID, notification)
      SpokenAlerts.enqueue(
        context,
        -1,
        "update",
        localized.getString(R.string.update_title),
        text,
        CHANNEL_ID,
        "status"
      )
    } catch (_: SecurityException) {
      // Notifications are off. The in-app banner still offers the download.
    }
  }

  fun cancel(context: Context) {
    context.getSystemService(NotificationManager::class.java)?.cancel(NOTICE_ID)
  }

  fun allowed(raw: String): Boolean {
    if (raw.length > 200 || raw.any { it.isWhitespace() || it == '\\' || it == '@' }) return false
    val uri = Uri.parse(raw)
    if (!uri.scheme.equals("https", ignoreCase = true)) return false
    if (!uri.userInfo.isNullOrEmpty()) return false
    if (!uri.encodedQuery.isNullOrEmpty() || !uri.encodedFragment.isNullOrEmpty()) return false
    if (uri.port != -1) return false
    val host = uri.host?.trimEnd('.')?.lowercase(Locale.US) ?: return false
    val path = uri.path ?: return false
    if (path.contains("..")) return false
    val parts = uri.pathSegments
    if (host == "github.com") {
      if (parts.size < 4 || parts[0] != "Gabo-Tech" || parts[1] != "grok-voice-android" || parts[2] != "releases") {
        return false
      }
      if (parts.size == 5 && parts[3] == "tag" && versionToken.matches(parts[4])) return true
      if (parts.size == 6 && parts[3] == "download" && versionToken.matches(parts[4])) {
        val bare = parts[4].removePrefix("v")
        return parts[5] == "GrokVoice-$bare.apk"
      }
      return false
    }
    return host == "gabo-tech.github.io" && parts.size == 1 && parts[0] == "grok-voice-android"
  }

  private fun pendingView(context: Context, url: String, request: Int): PendingIntent? {
    if (!allowed(url)) return null
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    return PendingIntent.getActivity(context, request, intent, flags)
  }

  private fun ensureChannel(localized: Context, nm: NotificationManager) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val channel = NotificationChannel(
      CHANNEL_ID,
      localized.getString(R.string.update_channel_name),
      NotificationManager.IMPORTANCE_DEFAULT
    )
    channel.description = localized.getString(R.string.update_channel_desc)
    nm.createNotificationChannel(channel)
  }

  private fun localeContext(context: Context, localeTag: String): Context {
    val tag = localeTag.trim().replace('_', '-')
    if (tag.isEmpty()) return context
    val locale = Locale.forLanguageTag(tag)
    if (locale.language.isNullOrEmpty()) return context
    val config = Configuration(context.resources.configuration)
    config.setLocale(locale)
    config.setLocales(LocaleList(locale))
    return context.createConfigurationContext(config)
  }
}
