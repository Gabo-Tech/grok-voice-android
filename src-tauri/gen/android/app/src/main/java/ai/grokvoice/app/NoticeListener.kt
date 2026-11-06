package ai.grokvoice.app

import android.content.ComponentName
import android.content.Context
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/** Reads notifications that are already posted. The user turns this on in system settings. */
class NoticeListener : NotificationListenerService() {
  data class Row(val title: String, val text: String)

  override fun onListenerConnected() {
    refresh()
  }

  override fun onNotificationPosted(sbn: StatusBarNotification?) {
    refresh()
  }

  override fun onNotificationRemoved(sbn: StatusBarNotification?) {
    refresh()
  }

  private fun refresh() {
    val rows = activeNotifications?.mapNotNull { posted ->
      val extras = posted.notification.extras
      val title = extras?.getCharSequence("android.title")?.toString()?.trim().orEmpty()
      val text = extras?.getCharSequence("android.text")?.toString()?.trim().orEmpty()
      if (title.isEmpty() && text.isEmpty()) null else Row(title.ifEmpty { posted.packageName }, text.take(140))
    }.orEmpty()
    current = rows
  }

  companion object {
    @Volatile
    private var current: List<Row> = emptyList()

    fun current(): List<Row> = current

    fun enabled(context: Context): Boolean {
      val flat = android.provider.Settings.Secure.getString(context.contentResolver, "enabled_notification_listeners") ?: return false
      val mine = ComponentName(context, NoticeListener::class.java).flattenToString()
      return flat.split(":").any { it == mine }
    }
  }
}
