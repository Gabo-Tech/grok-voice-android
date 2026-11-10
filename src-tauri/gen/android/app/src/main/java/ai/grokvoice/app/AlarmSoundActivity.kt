package ai.grokvoice.app

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.net.http.SslError
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.WindowManager
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.SslErrorHandler
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.TextView
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import java.util.Locale

/**
 * Alarm screen for a chosen sound. A local file shows the title and a stop
 * button. A YouTube link loads the official IFrame Player and plays that
 * video here. Nothing is downloaded. If the player cannot start, the screen
 * says so and the default alarm sound keeps playing.
 */
class AlarmSoundActivity : AppCompatActivity() {
  private val main = Handler(Looper.getMainLooper())
  private var web: WebView? = null
  private var triedYoutube = false
  @Volatile private var youtubeStarted = false
  private var closing = false

  override fun attachBaseContext(newBase: Context) {
    super.attachBaseContext(AlarmSounds.localeContext(newBase))
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    turnScreenOn()
    setContentView(R.layout.activity_alarm_sound)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        AlarmSounds.stop(this@AlarmSoundActivity)
      }
    })
    findViewById<Button>(R.id.alarm_stop).setOnClickListener {
      AlarmSounds.stop(this)
    }
    AlarmSounds.attach(this)
    show(intent)
  }

  override fun onNewIntent(intent: android.content.Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    show(intent)
  }

  override fun onDestroy() {
    main.removeCallbacks(youtubeTimeout)
    val page = web
    web = null
    page?.let { view ->
      view.removeJavascriptInterface("AlarmSound")
      view.destroy()
    }
    if (!closing && triedYoutube && AlarmSounds.isRinging()) {
      AlarmSounds.onYoutubeClosed(this)
    }
    AlarmSounds.detach(this)
    super.onDestroy()
  }

  fun prepareToClose() {
    closing = true
  }

  fun showStatus(message: String) {
    runOnUiThread {
      findViewById<TextView>(R.id.alarm_status)?.text = message
    }
  }

  fun hidePlayer() {
    runOnUiThread {
      main.removeCallbacks(youtubeTimeout)
      web?.visibility = android.view.View.GONE
    }
  }

  fun pauseVideo() {
    runOnUiThread {
      web?.evaluateJavascript("if (window.player && player.pauseVideo) player.pauseVideo();", null)
    }
  }

  fun playVideo() {
    runOnUiThread {
      web?.evaluateJavascript("if (window.player && player.playVideo) { player.unMute(); player.playVideo(); }", null)
    }
  }

  private fun show(intent: android.content.Intent) {
    val title = intent.getStringExtra("title").orEmpty().ifBlank { "Alarm" }
    val kind = intent.getStringExtra("kind").orEmpty()
    val ref = intent.getStringExtra("ref").orEmpty()
    val status = intent.getStringExtra("status").orEmpty().ifBlank { AlarmSounds.statusText() }
    val liveId = AlarmSounds.youtubeVideoId()
    val tryYoutube = intent.getBooleanExtra("tryYoutube", false) || liveId.isNotEmpty()
    findViewById<TextView>(R.id.alarm_title).text = title
    findViewById<TextView>(R.id.alarm_status).text = status.ifBlank { AlarmSounds.statusText() }
    val video = liveId.ifBlank { ref }
    if (web != null && youtubeStarted) return
    if (tryYoutube && video.matches(Regex("^[A-Za-z0-9_-]{11}$"))) {
      triedYoutube = true
      loadYoutube(video)
    } else if (kind != "youtube" && liveId.isEmpty()) {
      web?.visibility = android.view.View.GONE
    }
  }

  @SuppressLint("SetJavaScriptEnabled")
  private fun loadYoutube(id: String) {
    val view = findViewById<WebView>(R.id.alarm_player)
    web = view
    view.setBackgroundColor(Color.BLACK)
    view.visibility = android.view.View.VISIBLE
    val settings = view.settings
    settings.javaScriptEnabled = true
    settings.mediaPlaybackRequiresUserGesture = false
    settings.domStorageEnabled = true
    settings.allowFileAccess = false
    settings.allowContentAccess = false
    settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
    CookieManager.getInstance().setAcceptCookie(true)
    CookieManager.getInstance().setAcceptThirdPartyCookies(view, true)
    view.addJavascriptInterface(YtBridge(), "AlarmSound")
    view.webViewClient = object : WebViewClient() {
      override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        return !allowedHost(request.url.host)
      }

      override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
        handler.cancel()
        AlarmSounds.onYoutubeFailed(this@AlarmSoundActivity, getString(R.string.alarm_youtube_failed))
      }
    }
    val html = playerHtml(id)
    if (html.isEmpty()) {
      AlarmSounds.onYoutubeFailed(this, getString(R.string.alarm_youtube_failed))
      return
    }
    view.loadDataWithBaseURL("https://www.youtube.com", html, "text/html", "utf-8", null)
    main.removeCallbacks(youtubeTimeout)
    main.postDelayed(youtubeTimeout, AlarmSounds.youtubeWaitMs())
  }

  private val youtubeTimeout = Runnable {
    if (!youtubeStarted) {
      AlarmSounds.onYoutubeFailed(this, getString(R.string.alarm_youtube_failed))
    }
  }

  private fun turnScreenOn() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
      setShowWhenLocked(true)
      setTurnScreenOn(true)
    } else {
      @Suppress("DEPRECATION")
      window.addFlags(
        WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
      )
    }
    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
  }

  private inner class YtBridge {
    @JavascriptInterface
    fun onPlaying() {
      youtubeStarted = true
      main.removeCallbacks(youtubeTimeout)
      AlarmSounds.onYoutubePlaying(this@AlarmSoundActivity)
    }

    @JavascriptInterface
    fun onError(code: Int) {
      main.post {
        if (!youtubeStarted) {
          AlarmSounds.onYoutubeFailed(this@AlarmSoundActivity, getString(R.string.alarm_youtube_failed))
        }
      }
    }
  }

  companion object {
    fun playerHtml(id: String): String {
      if (!id.matches(Regex("^[A-Za-z0-9_-]{11}$"))) return ""
      return """
        <!DOCTYPE html>
        <html>
        <head>
          <meta name="viewport" content="width=device-width, initial-scale=1">
          <style>html,body{margin:0;background:#000;height:100%}#player{width:100%;height:220px}</style>
        </head>
        <body>
          <div id="player"></div>
          <script src="https://www.youtube.com/iframe_api"></script>
          <script>
            var player;
            function onYouTubeIframeAPIReady() {
              player = new YT.Player('player', {
                videoId: '$id',
                playerVars: { autoplay: 1, playsinline: 1, controls: 1, rel: 0, modestbranding: 1, origin: 'https://www.youtube.com' },
                events: {
                  onReady: function (e) { e.target.unMute(); e.target.setVolume(100); e.target.playVideo(); },
                  onError: function (e) { if (window.AlarmSound) AlarmSound.onError(e.data || 0); },
                  onStateChange: function (e) { if (e.data === 1 && window.AlarmSound) AlarmSound.onPlaying(); }
                }
              });
            }
          </script>
        </body>
        </html>
      """.trimIndent()
    }

    private fun allowedHost(host: String?): Boolean {
      val name = host?.lowercase(Locale.US) ?: return false
      return name == "youtube.com" || name.endsWith(".youtube.com") ||
        name == "youtu.be" || name.endsWith(".youtu.be") ||
        name == "youtube-nocookie.com" || name.endsWith(".youtube-nocookie.com") ||
        name.endsWith(".google.com") || name.endsWith(".gstatic.com") ||
        name.endsWith(".ytimg.com") || name.endsWith(".ggpht.com") ||
        name.endsWith(".googlevideo.com")
    }
  }
}
