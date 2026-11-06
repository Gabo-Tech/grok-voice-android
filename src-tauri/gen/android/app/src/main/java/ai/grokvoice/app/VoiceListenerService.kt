package ai.grokvoice.app

import android.app.ActivityOptions
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import java.util.concurrent.atomic.AtomicBoolean
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Microphone foreground service. It owns the AudioRecord that feeds the wake engine,
 * holds a partial wake lock, and stays up when the activity is paused, the screen
 * turns off, or the task is swiped away.
 *
 * v0.1.20 only posted the notification. Capture still used an activity-tied AAudio
 * stream, which Android silences in those cases.
 */
class VoiceListenerService : Service() {
  companion object {
    const val ACTION_START = "ai.grokvoice.app.action.START_LISTENING"
    const val ACTION_STOP = "ai.grokvoice.app.action.STOP_LISTENING"
    const val EXTRA_TITLE = "title"
    const val EXTRA_TEXT = "text"
    const val EXTRA_REASON = "reason"
    const val REASON_NOTIFICATION = "notification Stop"
    const val REASON_USER = "user turned Listen off"
    private const val CHANNEL_ID = "grok_listener"
    private const val ALERT_CHANNEL_ID = "grok_wake_alert"
    private const val NOTIFICATION_ID = 4242
    private const val ALERT_ID = 4243
    private const val REOPEN_REQUEST = 7
    private const val RESTART_REQUEST = 9
    private const val TAG = "GrokVoice"
    private const val WAKE_LOCK_TIMEOUT_MS = 12L * 60 * 60 * 1000
    private const val PREFS = "listen"
    private const val KEY_ARMED = "armed"
    private val active = AtomicBoolean(false)
    private val micOpen = AtomicBoolean(false)
    private val uiCreated = AtomicBoolean(false)
    private val uiResumed = AtomicBoolean(false)

    fun setArmed(context: Context, armed: Boolean) {
      context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_ARMED, armed).apply()
    }

    fun isArmed(context: Context): Boolean {
      return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_ARMED, false)
    }

    fun isActive(): Boolean = active.get()

    fun isMicOpen(): Boolean = micOpen.get()

    fun setUiCreated(created: Boolean) {
      uiCreated.set(created)
    }

    fun setUiResumed(resumed: Boolean) {
      uiResumed.set(resumed)
    }

    fun uiIsCreated(): Boolean = uiCreated.get()

    fun uiIsResumed(): Boolean = uiResumed.get()

    fun rememberReopen(context: Context) {
      if (Build.VERSION.SDK_INT < 34) return
      val intent = headlessIntent(context)
      val existing = PendingIntent.getActivity(
        context, REOPEN_REQUEST, intent,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE
      )
      if (existing != null) return
      try {
        val options = ActivityOptions.makeBasic().apply {
          setPendingIntentCreatorBackgroundActivityStartMode(
            ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED
          )
        }
        PendingIntent.getActivity(
          context, REOPEN_REQUEST, intent,
          PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
          options.toBundle()
        )
      } catch (e: Exception) {
        Log.w(TAG, "could not prepare reopen", e)
      }
    }

    fun presentTalk(context: Context) {
      if (!isArmed(context)) return
      val pm = context.getSystemService(PowerManager::class.java)
      val away = !uiResumed.get() || pm?.isInteractive != true
      if (away) {
        ListenTrace.event(context, "wake while the screen is off or the app is away, bringing Talk forward")
        postAlert(context)
      }
      WidgetGate.allow(context, "lock")
      try {
        context.startActivity(talkIntent(context))
      } catch (e: Exception) {
        ListenTrace.event(context, "could not open Talk (${e.javaClass.simpleName})")
      }
    }

    fun cancelAlert(context: Context) {
      val nm = context.getSystemService(NotificationManager::class.java) ?: return
      nm.cancel(ALERT_ID)
    }

    private fun headlessIntent(context: Context): Intent {
      return Intent(context, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        .putExtra(MainActivity.EXTRA_HEADLESS, true)
    }

    private fun talkIntent(context: Context): Intent {
      return Intent(context, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        .putExtra(MainActivity.EXTRA_WIDGET, "lock")
    }

    private fun postAlert(context: Context) {
      ensureAlertChannel(context)
      val immutable = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
      val open = PendingIntent.getActivity(context, 2, talkIntent(context), immutable)
      val notification = NotificationCompat.Builder(context, ALERT_CHANNEL_ID)
        .setSmallIcon(android.R.drawable.ic_btn_speak_now)
        .setContentTitle(context.getString(R.string.listener_alert_title))
        .setContentText(context.getString(R.string.listener_alert_text))
        .setContentIntent(open)
        .setFullScreenIntent(open, true)
        .setCategory(NotificationCompat.CATEGORY_ALARM)
        .setPriority(NotificationCompat.PRIORITY_HIGH)
        .setAutoCancel(true)
        .setSilent(true)
        .build()
      val nm = context.getSystemService(NotificationManager::class.java) ?: return
      nm.notify(ALERT_ID, notification)
    }

    private fun ensureAlertChannel(context: Context) {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
      val nm = context.getSystemService(NotificationManager::class.java) ?: return
      val channel = NotificationChannel(
        ALERT_CHANNEL_ID,
        context.getString(R.string.listener_alert_channel_name),
        NotificationManager.IMPORTANCE_HIGH
      ).apply {
        description = context.getString(R.string.listener_alert_text)
        setSound(null, null)
        enableVibration(false)
        setShowBadge(false)
      }
      nm.createNotificationChannel(channel)
    }
  }

  private val mainHandler = Handler(Looper.getMainLooper())
  private var wakeLock: PowerManager.WakeLock? = null
  private var explicitStop = false
  private var capture: MicCapture? = null
  private var screenReceiver: BroadcastReceiver? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    val filter = IntentFilter().apply {
      addAction(Intent.ACTION_SCREEN_OFF)
      addAction(Intent.ACTION_SCREEN_ON)
    }
    val receiver = object : BroadcastReceiver() {
      override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
          Intent.ACTION_SCREEN_OFF -> {
            if (!explicitStop && isArmed(this@VoiceListenerService)) {
              ListenTrace.event(this@VoiceListenerService, "screen off, listening continues")
              acquireWakeLock()
              ensureCapture()
            }
          }
          Intent.ACTION_SCREEN_ON -> {
            if (isArmed(this@VoiceListenerService)) {
              ListenTrace.event(this@VoiceListenerService, "screen on")
            }
          }
        }
      }
    }
    screenReceiver = receiver
    try {
      if (Build.VERSION.SDK_INT >= 33) {
        registerReceiver(receiver, filter, RECEIVER_NOT_EXPORTED)
      } else {
        @Suppress("UnspecifiedRegisterReceiverFlag")
        registerReceiver(receiver, filter)
      }
    } catch (e: Exception) {
      Log.w(TAG, "could not watch the screen", e)
    }
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP || (intent == null && !isArmed(this))) {
      val reason = intent?.getStringExtra(EXTRA_REASON) ?: REASON_USER
      explicitStop = true
      stopCapture()
      ListenTrace.stop(this, reason)
      setArmed(this, false)
      cancelRestart(this)
      RustBridge.stopListeningSafe()
      shutdown()
      return START_NOT_STICKY
    }

    explicitStop = false
    setArmed(this, true)
    ListenTrace.clearStop(this)
    if (uiResumed.get()) rememberReopen(this)

    ensureChannel()
    val type = when {
      Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
      else -> 0
    }
    try {
      ServiceCompat.startForeground(this, NOTIFICATION_ID, buildNotification(), type)
    } catch (e: Exception) {
      ListenTrace.event(this, "foreground service failed to start (${e.javaClass.simpleName})")
      RustBridge.serviceFailedSafe()
      explicitStop = true
      active.set(false)
      stopSelf()
      return START_NOT_STICKY
    }
    active.set(true)
    acquireWakeLock()
    ListenTrace.event(this, "foreground service running")
    ensureCapture()
    HomeWidgets.updateAll(this)
    ensureEngine()
    return START_STICKY
  }

  override fun onTaskRemoved(rootIntent: Intent?) {
    if (!explicitStop && isArmed(this)) {
      scheduleRestart(this)
      ListenTrace.event(this, "task removed, listening continues")
      ensureCapture()
    }
    super.onTaskRemoved(rootIntent)
  }

  override fun onDestroy() {
    val restart = !explicitStop && isArmed(this)
    stopCapture()
    releaseWakeLock()
    active.set(false)
    screenReceiver?.let {
      try {
        unregisterReceiver(it)
      } catch (_: Exception) {
      }
    }
    screenReceiver = null
    super.onDestroy()
    if (restart) {
      ListenTrace.event(this, "foreground service destroyed while Listen is on, scheduling restart")
      scheduleRestart(this)
    }
  }

  private fun ensureEngine() {
    if (RustBridge.engineAliveSafe()) return
    if (uiCreated.get() || uiResumed.get()) {
      ListenTrace.event(this, "waiting for the open window to arm listening")
      return
    }
    ListenTrace.event(this, "process restarted, re-arming listener")
    if (!sendReopen()) {
      ListenTrace.event(this, "could not reopen the app to re-arm listening")
    }
  }

  private fun sendReopen(): Boolean {
    WidgetGate.allow(this, "headless")
    val intent = headlessIntent(this)
    if (Build.VERSION.SDK_INT >= 34) {
      val existing = PendingIntent.getActivity(
        this, REOPEN_REQUEST, intent,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE
      )
      if (existing != null) {
        return try {
          val options = ActivityOptions.makeBasic().apply {
            setPendingIntentBackgroundActivityStartMode(
              ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED
            )
          }
          existing.send(this, 0, null, null, null, null, options.toBundle())
          true
        } catch (e: Exception) {
          ListenTrace.event(this, "reopen pending intent failed (${e.javaClass.simpleName})")
          false
        }
      }
    }
    return try {
      startActivity(intent)
      true
    } catch (e: Exception) {
      ListenTrace.event(this, "reopen activity blocked (${e.javaClass.simpleName})")
      false
    }
  }

  private fun ensureCapture() {
    if (explicitStop || !isArmed(this)) return
    if (micOpen.get()) return
    val mic = capture ?: MicCapture(
      onEvent = { ListenTrace.event(this, it) },
      onOpenChanged = { open -> micOpen.set(open) },
      onPresent = { mainHandler.post { presentTalk(this) } },
      onStopped = { reopen ->
        micOpen.set(false)
        if (reopen) mainHandler.postDelayed({ ensureCapture() }, 1000)
      }
    ).also { capture = it }
    mic.start()
  }

  private fun stopCapture() {
    micOpen.set(false)
    capture?.stop()
    capture = null
  }

  private fun shutdown() {
    explicitStop = true
    active.set(false)
    setArmed(this, false)
    cancelAlert(this)
    HomeWidgets.updateAll(this)
    releaseWakeLock()
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  private fun scheduleRestart(context: Context) {
    val am = context.getSystemService(AlarmManager::class.java) ?: return
    val restart = Intent(context, VoiceListenerService::class.java).setAction(ACTION_START)
    val pi = PendingIntent.getService(
      context, RESTART_REQUEST, restart,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val at = SystemClock.elapsedRealtime() + 1500
    try {
      if (Build.VERSION.SDK_INT >= 23) {
        am.setExactAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
      } else {
        am.setExact(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
      }
    } catch (_: SecurityException) {
      am.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
    }
  }

  private fun cancelRestart(context: Context) {
    val am = context.getSystemService(AlarmManager::class.java) ?: return
    val restart = Intent(context, VoiceListenerService::class.java).setAction(ACTION_START)
    val pi = PendingIntent.getService(
      context, RESTART_REQUEST, restart,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE
    ) ?: return
    am.cancel(pi)
    pi.cancel()
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val nm = getSystemService(NotificationManager::class.java)
    val channel = NotificationChannel(
      CHANNEL_ID,
      getString(R.string.listener_channel_name),
      NotificationManager.IMPORTANCE_LOW
    ).apply {
      description = getString(R.string.listener_channel_desc)
      setShowBadge(false)
    }
    nm.createNotificationChannel(channel)
  }

  private fun buildNotification(): Notification {
    val immutable = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      immutable
    )
    val stop = PendingIntent.getService(
      this, 1,
      Intent(this, VoiceListenerService::class.java)
        .setAction(ACTION_STOP)
        .putExtra(EXTRA_REASON, REASON_NOTIFICATION),
      immutable
    )
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.ic_btn_speak_now)
      .setContentTitle(getString(R.string.listener_title))
      .setContentText(getString(R.string.listener_text))
      .setContentIntent(open)
      .addAction(0, getString(R.string.listener_stop), stop)
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
