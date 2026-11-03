package ai.grokvoice.app

import android.app.ActivityManager
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
import android.os.PowerManager
import android.util.Log
import java.util.concurrent.atomic.AtomicBoolean
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Microphone-typed foreground service. It does no audio work itself: its job is to keep
 * the process (and the Rust cpal capture thread inside it) alive and allowed to record
 * while the activity is backgrounded or the screen is off.
 */
class VoiceListenerService : Service() {
  companion object {
    const val ACTION_START = "ai.grokvoice.app.action.START_LISTENING"
    const val ACTION_STOP = "ai.grokvoice.app.action.STOP_LISTENING"
    const val EXTRA_TITLE = "title"
    const val EXTRA_TEXT = "text"
    private const val CHANNEL_ID = "grok_listener"
    private const val NOTIFICATION_ID = 4242
    private const val TAG = "GrokVoice"
    private const val WAKE_LOCK_TIMEOUT_MS = 12L * 60 * 60 * 1000
    private const val PREFS = "listen"
    private const val KEY_ARMED = "armed"
    private val active = AtomicBoolean(false)

    fun setArmed(context: Context, armed: Boolean) {
      context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_ARMED, armed).apply()
    }

    fun isArmed(context: Context): Boolean {
      return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_ARMED, false)
    }

    fun isActive(): Boolean = active.get()
  }

  private var wakeLock: PowerManager.WakeLock? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      setArmed(this, false)
      RustBridge.stopListeningSafe()
      shutdown()
      return START_NOT_STICKY
    }

    setArmed(this, true)

    ensureChannel()
    val title = intent?.getStringExtra(EXTRA_TITLE) ?: "Hey Grok is listening"
    val text = intent?.getStringExtra(EXTRA_TEXT) ?: "Say “Hey Grok”, or tap to open"
    val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    } else {
      0
    }
    try {
      ServiceCompat.startForeground(this, NOTIFICATION_ID, buildNotification(title, text), type)
    } catch (e: Exception) {
      // Android 14+ throws if RECORD_AUDIO isn't granted or the app is not in the foreground.
      Log.e(TAG, "startForeground failed", e)
      stopSelf()
      return START_NOT_STICKY
    }
    acquireWakeLock()
    active.set(true)
    hideFromRecents()
    // Clearing the app can kill this process. If it comes back, the activity
    // has to start once so the wake-word engine is running, then leave again.
    if (!RustBridge.engineAliveSafe()) {
      val launch = Intent(this, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        .putExtra(MainActivity.EXTRA_HEADLESS, true)
      startActivity(launch)
    }
    return START_STICKY
  }

  override fun onTaskRemoved(rootIntent: Intent?) {
    // Swiping the card away must not end wake listening.
    if (active.get() || isArmed(this)) {
      val restart = Intent(applicationContext, VoiceListenerService::class.java).setAction(ACTION_START)
      ContextCompat.startForegroundService(applicationContext, restart)
    }
    super.onTaskRemoved(rootIntent)
  }

  override fun onDestroy() {
    releaseWakeLock()
    super.onDestroy()
  }

  private fun shutdown() {
    active.set(false)
    setArmed(this, false)
    showInRecents()
    releaseWakeLock()
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  private fun hideFromRecents() {
    val am = getSystemService(ActivityManager::class.java) ?: return
    am.appTasks.forEach { it.setExcludeFromRecents(true) }
  }

  private fun showInRecents() {
    val am = getSystemService(ActivityManager::class.java) ?: return
    am.appTasks.forEach { it.setExcludeFromRecents(false) }
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val nm = getSystemService(NotificationManager::class.java)
    if (nm.getNotificationChannel(CHANNEL_ID) != null) return
    val channel = NotificationChannel(CHANNEL_ID, "Wake word listener", NotificationManager.IMPORTANCE_LOW).apply {
      description = "Shown while Grok Voice listens for “Hey Grok”"
      setShowBadge(false)
    }
    nm.createNotificationChannel(channel)
  }

  private fun buildNotification(title: String, text: String): Notification {
    val immutable = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      immutable
    )
    val stop = PendingIntent.getService(
      this, 1,
      Intent(this, VoiceListenerService::class.java).setAction(ACTION_STOP),
      immutable
    )
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.ic_btn_speak_now)
      .setContentTitle(title)
      .setContentText(text)
      .setContentIntent(open)
      .addAction(0, "Stop", stop)
      .setOngoing(true)
      .setSilent(true)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .build()
  }

  private fun acquireWakeLock() {
    if (wakeLock?.isHeld == true) return
    val pm = getSystemService(POWER_SERVICE) as PowerManager
    wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "GrokVoice:listener").apply {
      setReferenceCounted(false)
      acquire(WAKE_LOCK_TIMEOUT_MS)
    }
  }

  private fun releaseWakeLock() {
    wakeLock?.let { if (it.isHeld) it.release() }
    wakeLock = null
  }
}
