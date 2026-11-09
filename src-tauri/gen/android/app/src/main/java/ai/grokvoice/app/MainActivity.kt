package ai.grokvoice.app

import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.view.View
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.WebSettings
import android.webkit.WebView
import org.json.JSONObject
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    const val EXTRA_HEADLESS = "headless"
    const val EXTRA_WIDGET = "widget"
  }

  private val backCallback = object : OnBackPressedCallback(true) {
    override fun handleOnBackPressed() {
      dispatchBack()
    }
  }

  fun syncBackCallback(canGoBack: Boolean) {
    val listening = VoiceListenerService.isActive() || VoiceListenerService.isArmed(this)
    backCallback.isEnabled = canGoBack || listening
    // Above the WebView's own callback while a screen is open, so the gesture
    // hits our history. At the root it is removed and predictive back can leave.
    if (Build.VERSION.SDK_INT >= 33) BackOverlay.sync(this, canGoBack)
  }

  fun dispatchBack() {
    val web = findWebView(window?.decorView)
    if (web == null) {
      leaveRoot()
      return
    }
    web.evaluateJavascript(
      "(function(){try{return window.__grokBack?!!window.__grokBack():false}catch(e){return false}})()"
    ) { raw ->
      if (raw == "true") return@evaluateJavascript
      runOnUiThread { leaveRoot() }
    }
  }

  private fun leaveRoot() {
    if (VoiceListenerService.isActive() || VoiceListenerService.isArmed(this)) {
      moveTaskToBack(true)
      return
    }
    if (!backCallback.isEnabled) return
    backCallback.isEnabled = false
    onBackPressedDispatcher.onBackPressed()
    backCallback.isEnabled = true
  }

  private fun findWebView(view: View?): WebView? {
    if (view == null) return null
    if (view is WebView) return view
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) {
        val found = findWebView(view.getChildAt(i))
        if (found != null) return found
      }
    }
    return null
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    VoiceListenerService.setUiCreated(true)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { v, insets ->
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(0, 0, 0, ime.bottom)
      insets
    }
    // In-app history is handled in the WebView. At the root, back leaves the task
    // only when Listen is off. Predictive back (Android 13+) uses this callback.
    onBackPressedDispatcher.addCallback(this, backCallback)
    applyWidget(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    applyWidget(intent)
  }

  private fun applyWidget(intent: Intent?) {
    val claimed = intent?.getStringExtra(EXTRA_WIDGET)
    val headless = intent?.getBooleanExtra(EXTRA_HEADLESS, false) == true
    val expected = when {
      !claimed.isNullOrBlank() -> claimed
      headless -> "headless"
      else -> null
    }
    val action = if (expected == null) null else WidgetGate.consume(this, expected)
    val granted = action ?: run {
      if (Build.VERSION.SDK_INT >= 27) {
        setShowWhenLocked(false)
        setTurnScreenOn(false)
      }
      return
    }
    val overLock = granted == "lock"
    if (Build.VERSION.SDK_INT >= 27) {
      setShowWhenLocked(overLock)
      setTurnScreenOn(overLock)
    }
    when (granted) {
      "arm" -> {
        val start = Intent(this, VoiceListenerService::class.java).setAction(VoiceListenerService.ACTION_START)
        androidx.core.content.ContextCompat.startForegroundService(this, start)
        window?.decorView?.postDelayed({ moveTaskToBack(true) }, 700)
      }
      "disarm" -> {
        val stop = Intent(this, VoiceListenerService::class.java)
          .setAction(VoiceListenerService.ACTION_STOP)
          .putExtra(VoiceListenerService.EXTRA_REASON, VoiceListenerService.REASON_USER)
        try {
          startService(stop)
        } catch (_: Exception) {
          VoiceListenerService.setArmed(this, false)
          stopService(Intent(this, VoiceListenerService::class.java))
        }
        window?.decorView?.post { moveTaskToBack(true) }
      }
      "headless" -> window?.decorView?.post { moveTaskToBack(true) }
      "lock" -> {
        deliverWidget("talk")
        return
      }
    }
    if (granted != "headless") deliverWidget(granted)
  }

  private fun deliverWidget(action: String) {
    val js = "window.dispatchEvent(new CustomEvent('grok-widget',{detail:${JSONObject.quote(action)}}))"
    val root = window?.decorView ?: return
    val attempts = intArrayOf(0)
    val tick = object : Runnable {
      override fun run() {
        val hit = dispatchJs(root, js)
        attempts[0]++
        if (!hit && attempts[0] < 24) root.postDelayed(this, 400)
      }
    }
    root.postDelayed(tick, 250)
  }

  private fun dispatchJs(view: View, js: String): Boolean {
    var hit = false
    if (view is WebView) {
      view.evaluateJavascript(js, null)
      hit = true
    }
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) {
        if (dispatchJs(view.getChildAt(i), js)) hit = true
      }
    }
    return hit
  }

  override fun onWebViewCreate(webView: WebView) {
    webView.setBackgroundColor(Color.parseColor("#0E0E0E"))
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      webView.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
    }
    val settings = webView.settings
    settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
    @Suppress("DEPRECATION")
    settings.allowFileAccessFromFileURLs = false
    @Suppress("DEPRECATION")
    settings.allowUniversalAccessFromFileURLs = false
    if (!BuildConfig.DEBUG) {
      WebView.setWebContentsDebuggingEnabled(false)
    }
    // A forced hardware layer blanks this page when audio starts. A forced
    // software layer smears icons and flattens text. Leave the default.
    webView.setLayerType(View.LAYER_TYPE_NONE, null)
    webView.addJavascriptInterface(BackBridge(this), "GrokNav")
  }

  override fun onPause() {
    VoiceListenerService.setUiResumed(false)
    super.onPause()
    // Don't resume the WebView while the screen is off or another app is in front.
    // Capture itself lives in the microphone service; resuming the page here used
    // to fight that and drop wake-word audio.
    if (!isFinishing && screenInteractive() && hasWindowFocus()) resumeWebViews()
  }

  override fun onStart() {
    super.onStart()
    if (screenInteractive()) resumeWebViews()
  }

  override fun onResume() {
    super.onResume()
    VoiceListenerService.setUiResumed(true)
    VoiceListenerService.setUiCreated(true)
    VoiceListenerService.rememberReopen(this)
    VoiceListenerService.onAppForeground(this)
    VoiceListenerService.cancelAlert(this)
    resumeWebViews()
    // A living process can outlast the activity. If the window was destroyed
    // with the task, open it again without restarting wake enrollment.
    if (RustBridge.engineAliveSafe()) {
      window?.decorView?.post { RustBridge.ensureUiSafe() }
    }
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

  override fun onDestroy() {
    syncBackCallback(false)
    VoiceListenerService.setUiResumed(false)
    if (!isChangingConfigurations) {
      VoiceListenerService.setUiCreated(false)
      if (VoiceListenerService.isArmed(this)) {
        ListenTrace.event(this, "activity destroyed, listening continues")
      }
    }
    super.onDestroy()
  }
}

private class BackBridge(private val activity: MainActivity) {
  @JavascriptInterface
  fun setCanGoBack(can: Boolean) {
    activity.runOnUiThread { activity.syncBackCallback(can) }
  }
}
