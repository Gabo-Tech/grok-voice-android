package ai.grokvoice.app

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.ServiceCompat

/**
 * Keeps the process in the foreground while a chosen alarm sound plays.
 * The notification is the alarm itself, not a second banner.
 */
class AlarmSoundService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == AlarmSounds.ACTION_STOP) {
      AlarmSounds.stop(applicationContext)
      stopSelf()
      return START_NOT_STICKY
    }
    val id = AlarmSounds.noticeId().takeIf { it > 0 } ?: FALLBACK_ID
    val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
    } else {
      0
    }
    try {
      ServiceCompat.startForeground(this, id, AlarmSounds.foregroundNote(this), type)
    } catch (t: Exception) {
      Log.w(TAG, "alarm sound could not enter the foreground", t)
    }
    return START_NOT_STICKY
  }

  companion object {
    private const val TAG = "GrokVoice"
    private const val FALLBACK_ID = 71041
  }
}
