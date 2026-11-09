package ai.grokvoice.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.ServiceCompat
import java.util.concurrent.atomic.AtomicInteger

/**
 * Holds the process in the foreground while a spoken alert plays.
 *
 * Android 17 silences audio started from a background alarm receiver unless a
 * foreground service is running. Target SDK 37 also requires that service to
 * have while-in-use capabilities, waived for the alarm stream when the app
 * holds the exact-alarm permission. This service is the foreground half of
 * that exemption. It is not a Talk session.
 */
class SpokenNoticeService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent == null) {
      Log.w(TAG, "spoken notice service started without an alert")
      stopSelf()
      return START_NOT_STICKY
    }
    synchronized(gate) { pending.incrementAndGet() }
    val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
    } else {
      0
    }
    try {
      ServiceCompat.startForeground(this, NOTICE_ID, speakingNotification(intent), type)
    } catch (t: Exception) {
      Log.w(TAG, "spoken notice could not enter the foreground; Android may silence playback", t)
    }
    SpokenAlerts.speakQueued(applicationContext, intent) { finishOne() }
    return START_NOT_STICKY
  }

  private fun finishOne() {
    synchronized(gate) {
      if (pending.decrementAndGet() > 0) return
      try {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
      } catch (t: Exception) {
        Log.w(TAG, "spoken notice foreground stop failed", t)
      }
      stopSelf()
    }
  }

  private fun speakingNotification(intent: Intent): Notification {
    val channelId = CHANNEL_ID
    val nm = getSystemService(NotificationManager::class.java)
    if (nm?.getNotificationChannel(channelId) == null && nm != null) {
      val channel = NotificationChannel(channelId, "Spoken alerts", NotificationManager.IMPORTANCE_DEFAULT)
      channel.setSound(null, null)
      channel.enableVibration(false)
      channel.description = "Stays up while a reminder is spoken aloud"
      nm.createNotificationChannel(channel)
    }
    val title = intent.getStringExtra("title")?.ifBlank { "Grok" } ?: "Grok"
    val body = intent.getStringExtra("body").orEmpty().ifBlank { "Speaking" }
    val category = when (intent.getStringExtra("category")) {
      "alarm" -> Notification.CATEGORY_ALARM
      "event" -> Notification.CATEGORY_EVENT
      else -> Notification.CATEGORY_REMINDER
    }
    val open = PendingIntent.getActivity(
      this,
      0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    return Notification.Builder(this, channelId)
      .setSmallIcon(android.R.drawable.ic_lock_idle_alarm)
      .setContentTitle(title)
      .setContentText(body)
      .setContentIntent(open)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setCategory(category)
      .build()
  }

  companion object {
    private const val TAG = "GrokVoice"
    private const val CHANNEL_ID = "grok_spoken"
    private const val NOTICE_ID = 91025
    private val pending = AtomicInteger(0)
    private val gate = Any()

    fun intent(
      context: Context,
      id: Int,
      kind: String,
      title: String,
      body: String,
      channelId: String,
      category: String,
      bannerAudible: Boolean,
    ): Intent {
      return Intent(context, SpokenNoticeService::class.java)
        .putExtra("id", id)
        .putExtra("kind", kind)
        .putExtra("title", title)
        .putExtra("body", body)
        .putExtra("channelId", channelId)
        .putExtra("category", category)
        .putExtra("bannerAudible", bannerAudible)
    }
  }
}
