package ai.grokvoice.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class ReminderReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val pending = goAsync()
    val id = intent.getIntExtra("id", 0)
    val kind = intent.getStringExtra("kind") ?: "reminder"
    val title = intent.getStringExtra("title")?.takeIf { it.isNotBlank() }
      ?: AppLocale.wrap(context).getString(R.string.widget_fallback_reminder)
    val note = intent.getStringExtra("note") ?: ""
    PhoneActions.notify(context, id, kind, title, note)
    PhoneActions.cancel(context, id)
    SpokenAlerts.afterQueue { pending.finish() }
  }
}
