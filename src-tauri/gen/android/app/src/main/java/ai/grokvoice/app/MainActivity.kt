package ai.grokvoice.app

import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    const val EXTRA_HEADLESS = "headless"
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    if (intent?.getBooleanExtra(EXTRA_HEADLESS, false) == true) {
      window?.decorView?.post { moveTaskToBack(true) }
    }
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
    // A forced hardware layer blanks this page when audio starts. A forced
    // software layer smears icons and flattens text. Leave the default.
    webView.setLayerType(View.LAYER_TYPE_NONE, null)
  }

  override fun onPause() {
    super.onPause()
    // Resume only while the screen is on and this window is still in front.
    // Resuming after Home or screen-off keeps the process from settling into
    // the microphone foreground service, and that drops wake-word capture.
    if (!isFinishing && screenInteractive() && hasWindowFocus()) resumeWebViews()
  }

  override fun onStart() {
    super.onStart()
    resumeWebViews()
  }

  override fun onResume() {
    super.onResume()
    resumeWebViews()
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) resumeWebViews()
  }

  private fun screenInteractive(): Boolean {
    val pm = getSystemService(POWER_SERVICE) as PowerManager
    return pm.isInteractive
  }

  private fun resumeWebViews() {
    val root = window?.decorView ?: return
    root.post { walk(root) }
    root.postDelayed({ walk(root) }, 120)
  }

  private fun walk(view: View) {
    if (view is WebView) {
      view.onResume()
      view.resumeTimers()
      // After another app covers this activity the WebView surface comes back
      // black until something forces a new frame.
      view.visibility = View.INVISIBLE
      view.visibility = View.VISIBLE
      view.requestLayout()
      view.invalidate()
    }
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) walk(view.getChildAt(i))
    }
  }
}
