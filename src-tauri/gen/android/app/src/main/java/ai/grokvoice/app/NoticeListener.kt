package ai.grokvoice.app

import android.content.ComponentName
import android.content.Context
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/** Reads notifications the user has allowed. Message bodies stay in Inbox's memory cache. */
class NoticeListener : NotificationListenerService() {
  data class Row(val title: String, val text: String)

  override fun onListenerConnected() {
    Inbox.onConnected(this)
    refresh()
  }

  override fun onDestroy() {
    Inbox.onDestroy(this)
    super.onDestroy()
  }

  override fun onNotificationPosted(sbn: StatusBarNotification?) {
    if (sbn != null) Inbox.onPosted(this, sbn)
    refresh()
  }

  override fun onNotificationRemoved(sbn: StatusBarNotification?) {
    if (sbn != null) Inbox.onRemoved(sbn)
    refresh()
  }

  private fun refresh() {
    val rows = activeNotifications?.mapNotNull { posted ->
      val pkg = posted.packageName ?: return@mapNotNull null
      if (Inbox.tracks(pkg)) return@mapNotNull null
      val extras = posted.notification.extras
      val title = extras?.getCharSequence("android.title")?.toString()?.trim().orEmpty()
      if (title.isEmpty()) null else Row(title, "")
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
