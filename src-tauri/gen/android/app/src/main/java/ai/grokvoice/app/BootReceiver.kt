package ai.grokvoice.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat

class BootReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
    PhoneActions.rescheduleAll(context)
    if (!VoiceListenerService.isArmed(context)) return
    try {
      val start = Intent(context, VoiceListenerService::class.java).setAction(VoiceListenerService.ACTION_START)
      ContextCompat.startForegroundService(context, start)
    } catch (e: Exception) {
      Log.w("GrokVoice", "could not restore wake listening after boot", e)
    }
  }
}
