package ai.grokvoice.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat

/** In-app budget warning. The text is already translated by the caller. */
object BudgetNotice {
  private const val CHANNEL_ID = "grok_budget"
  private const val NOTICE_BASE = 91080

  fun show(context: Context, title: String, body: String) {
    val safeTitle = title.trim().take(80)
    val safeBody = body.trim().take(180)
    if (safeTitle.isEmpty() || safeBody.isEmpty()) return
    val nm = context.getSystemService(NotificationManager::class.java) ?: return
    ensureChannel(context, nm)
    val open = PendingIntent.getActivity(
      context,
      NOTICE_BASE,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val id = NOTICE_BASE + (safeTitle.hashCode() and 0xff)
    val notification = NotificationCompat.Builder(context, CHANNEL_ID)
      .setSmallIcon(R.drawable.ic_stat_grok)
      .setContentTitle(safeTitle)
      .setContentText(safeBody)
      .setStyle(NotificationCompat.BigTextStyle().bigText(safeBody))
      .setContentIntent(open)
      .setAutoCancel(true)
      .setOnlyAlertOnce(true)
      .setCategory(NotificationCompat.CATEGORY_STATUS)
      .setPriority(NotificationCompat.PRIORITY_DEFAULT)
      .build()
    try {
      nm.notify(id, notification)
    } catch (_: SecurityException) {
      // Notifications are off. The plan still shows the warning.
    }
  }

  private fun ensureChannel(context: Context, nm: NotificationManager) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    if (nm.getNotificationChannel(CHANNEL_ID) != null) return
    val channel = NotificationChannel(CHANNEL_ID, context.getString(R.string.budget_channel_name), NotificationManager.IMPORTANCE_DEFAULT)
    channel.description = context.getString(R.string.budget_channel_desc)
    nm.createNotificationChannel(channel)
  }
}
