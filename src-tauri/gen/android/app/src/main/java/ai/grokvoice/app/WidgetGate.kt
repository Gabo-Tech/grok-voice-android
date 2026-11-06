package ai.grokvoice.app

import android.content.Context

/**
 * Widget, tile, and service launches are delivered to the exported activity.
 * The activity only acts on a value this process just wrote, so a caller that
 * only sets an intent extra does not start the microphone or a session.
 */
object WidgetGate {
    private const val PREFS = "widget_gate"
    private const val KEY_ACTION = "action"
    private const val KEY_AT = "at"
    private const val TTL_MS = 15_000L

    private val ALLOWED = setOf("arm", "disarm", "talk", "reminders", "events", "lock", "headless")

    fun allow(context: Context, action: String) {
        if (action !in ALLOWED) return
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY_ACTION, action)
            .putLong(KEY_AT, android.os.SystemClock.elapsedRealtime())
            .commit()
    }

    fun consume(context: Context, claimed: String?): String? {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val action = prefs.getString(KEY_ACTION, null)
        val at = prefs.getLong(KEY_AT, 0L)
        prefs.edit().clear().commit()
        if (claimed.isNullOrBlank() || action == null || action != claimed || action !in ALLOWED) return null
        if (android.os.SystemClock.elapsedRealtime() - at > TTL_MS) return null
        return action
    }
}
