package ai.grokvoice.app

import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.lifecycle.Lifecycle

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { v, insets ->
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(0, 0, 0, ime.bottom)
      insets
    }
    // Leaving with the back gesture should not tear down a running listener.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (VoiceListenerService.isActive()) {
          moveTaskToBack(true)
        } else {
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
        }
      }
    })
  }

  override fun onWebViewCreate(webView: WebView) {
    webView.setBackgroundColor(Color.parseColor("#0E0E0E"))
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      webView.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
    }
    // Hardware layers on this WebView drop the page once audio starts.
    webView.setLayerType(View.LAYER_TYPE_SOFTWARE, null)
  }

  override fun onPause() {
    super.onPause()
    // Only keep the WebView painting while the activity is still the thing on
    // screen. Forcing it awake after Home/recent-apps stops the page from blanking
    // during a mic session, but also keeps the process from settling into the
    // microphone foreground service — and that was killing wake-word capture.
    if (!isFinishing && lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED) && hasWindowFocus()) {
      resumeWebViews()
    }
  }

  override fun onResume() {
    super.onResume()
    resumeWebViews()
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) resumeWebViews()
  }

  private fun resumeWebViews() {
    val root = window?.decorView ?: return
    root.post { walk(root) }
  }

  private fun walk(view: View) {
    if (view is WebView) {
      view.onResume()
      view.resumeTimers()
      view.invalidate()
    }
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) walk(view.getChildAt(i))
    }
  }
}
