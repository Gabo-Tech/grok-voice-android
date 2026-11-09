package ai.grokvoice.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat

class BootReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val action = intent.action ?: return
    val restart = action == Intent.ACTION_BOOT_COMPLETED ||
      action == Intent.ACTION_MY_PACKAGE_REPLACED ||
      action == "android.intent.action.QUICKBOOT_POWERON" ||
      action == "com.htc.intent.action.QUICKBOOT_POWERON"
    if (!restart) return
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
