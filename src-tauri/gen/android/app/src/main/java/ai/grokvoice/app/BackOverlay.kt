package ai.grokvoice.app

import android.os.Build
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import androidx.annotation.RequiresApi

/** Predictive-back overlay for API 33+. Kept out of MainActivity so API 26 does not load these types. */
@RequiresApi(Build.VERSION_CODES.TIRAMISU)
object BackOverlay {
  private var callback: OnBackInvokedCallback? = null

  fun sync(activity: MainActivity, canGoBack: Boolean) {
    if (canGoBack && callback == null) {
      val next = OnBackInvokedCallback { activity.dispatchBack() }
      activity.onBackInvokedDispatcher.registerOnBackInvokedCallback(
        OnBackInvokedDispatcher.PRIORITY_OVERLAY,
        next
      )
      callback = next
    } else if (!canGoBack) {
      val current = callback ?: return
      activity.onBackInvokedDispatcher.unregisterOnBackInvokedCallback(current)
      callback = null
    }
  }
}
