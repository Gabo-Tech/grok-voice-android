package ai.grokvoice.app

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ResolveInfo
import android.media.AudioManager
import android.media.browse.MediaBrowser
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.net.Uri
import android.support.v4.media.session.MediaControllerCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Log
import android.view.KeyEvent
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Opens the music app the user picked and, where that app actually allows it, asks it to play.
 *
 * Musify (gokadzev/Musify, checked against the public master tree) ships three application ids:
 * `com.gokadzev.musify` (GitHub), `com.gokadzev.musify.fdroid`, and `com.gokadzev.musify.debug`.
 * The launcher activity is `com.ryanheise.audioservice.AudioServiceActivity`. Exported filters are
 * MAIN, `musify://playlist/custom/…`, ACTION_SEND of plain text, and ACTION_VIEW of youtube.com /
 * youtu.be links. There is no MEDIA_PLAY_FROM_SEARCH activity filter.
 *
 * What Musify does implement, via the audio_service MediaBrowserService
 * `com.ryanheise.audioservice.AudioService`, is `playFromSearch`. That searches inside Musify and
 * plays the first hit. An ACTION_SEND of a YouTube video link is the other path Musify's own code
 * plays, and only through the share stream, which does not replay the intent that cold-started
 * the process. This file does not scrape or download YouTube. A miss falls through to the YouTube
 * app, then to a browser opening the public YouTube page.
 *
 * Pause, next, previous, stop, and shuffle go to a live MediaSession when notification access lets
 * us see one. Musify publishes that session while audio_service is running. Shuffle is
 * `setShuffleMode` on that session. The in-app file player has no shuffle mode.
 */
object MusicLaunch {
  private const val TAG = "GrokVoice"
  private const val MUSIFY_SERVICE = "com.ryanheise.audioservice.AudioService"
  private val MUSIFY_PACKAGES = listOf(
    "com.gokadzev.musify",
    "com.gokadzev.musify.fdroid",
    "com.gokadzev.musify.debug",
  )
  private const val YOUTUBE = "com.google.android.youtube"
  private const val YOUTUBE_MUSIC = "com.google.android.apps.youtube.music"

  private data class Hit(val code: String, val title: String = "", val artist: String = "", val via: String = "")

  fun players(context: Context): String {
    val rows = JSONArray()
    rows.put(row("local", true, ""))
    val musify = installedPackage(context, MUSIFY_PACKAGES)
    rows.put(row("musify", musify != null, musify.orEmpty()))
    val ytMusic = installedPackage(context, listOf(YOUTUBE_MUSIC))
    rows.put(row("youtube_music", ytMusic != null, ytMusic.orEmpty()))
    val youtube = installedPackage(context, listOf(YOUTUBE))
    rows.put(row("youtube", youtube != null, youtube.orEmpty()))
    rows.put(row("system", true, ""))
    return JSONObject().put("ok", true).put("players", rows).toString()
  }

  fun play(context: Context, player: String, query: String, title: String, artist: String, album: String): String {
    val id = canonicalPlayer(player)
    val text = clip(query)
    if (id == "local") {
      return result(false, "music-phone-only", "", text, "local", "", "", "")
    }
    var failed = ""
    for (step in chain(id)) {
      val hit = runStep(context, step, text, clip(title), clip(artist), clip(album))
      if (hit != null) {
        return result(true, hit.code, failed, text, step, hit.via, hit.title, hit.artist)
      }
      if (failed.isEmpty()) failed = step
    }
    return result(false, "music-none", failed, text, id, "", "", "")
  }

  fun canonical(action: String): String {
    return when (action.trim().lowercase()) {
      "resume" -> "play"
      "shuffle_off", "unshuffle" -> "unshuffle"
      "play", "pause", "stop", "next", "previous", "shuffle" -> action.trim().lowercase()
      else -> ""
    }
  }

  fun control(context: Context, action: String, player: String, tryLocal: Boolean): String {
    val act = canonical(action)
    if (act.isEmpty()) return result(false, "media-bad", "", "", canonicalPlayer(player), "", "", "")
    val id = canonicalPlayer(player)
    val sessions = activeSessions(context)
    val preferred = packagesFor(id)
    if (sessions != null && preferred.isNotEmpty()) {
      sessions.firstOrNull { it.packageName in preferred }?.let { return transport(context, it, act, id) }
    }
    if (tryLocal) {
      LocalMusic.control(context, act)?.let { return it }
    }
    if (act == "shuffle" || act == "unshuffle") {
      if (sessions != null) {
        sessions.firstOrNull { it.packageName != context.packageName }?.let { return transport(context, it, act, it.packageName) }
      }
      return result(false, "media-shuffle-none", "", "", id, "", "", "")
    }
    if (sessions != null) {
      val playing = sessions.firstOrNull { it.packageName != context.packageName }
      if (playing != null && (act != "play" || id == "system")) {
        return transport(context, playing, act, playing.packageName)
      }
    }
    dispatchKey(context, act)
    val via = if (sessions == null) "listener" else "key"
    return result(true, "media-sent", "", "", id, via, "", "")
  }

  private fun chain(player: String): List<String> {
    return when (player) {
      "musify" -> listOf("musify", "youtube", "browser")
      "youtube_music" -> listOf("youtube_music", "youtube", "browser")
      "youtube" -> listOf("youtube", "browser")
      "system" -> listOf("system", "youtube", "browser")
      else -> emptyList()
    }
  }

  private fun runStep(context: Context, step: String, query: String, title: String, artist: String, album: String): Hit? {
    return try {
      when (step) {
        "musify" -> playMusify(context, query)
        "youtube_music" -> playYouTubeMusic(context, query, title, artist, album)
        "youtube" -> openYouTube(context, query)
        "browser" -> openBrowser(context, query)
        "system" -> playSystem(context, query, title, artist, album)
        else -> null
      }
    } catch (error: Exception) {
      Log.w(TAG, "music step $step failed", error)
      null
    }
  }

  private fun playMusify(context: Context, query: String): Hit? {
    val pkg = installedPackage(context, MUSIFY_PACKAGES) ?: return null
    val video = videoUrl(query)
    val cold = !probe(context, pkg, 2_500)
    if (cold) {
      if (!launch(context, pkg)) return null
      if (!probe(context, pkg, 7_000)) return null
      // The share listener is attached after the first Flutter frame, which is later than the
      // media browser. A cold share otherwise opens Musify and does not play.
      if (video != null) Thread.sleep(1_500)
    }
    return if (video != null) {
      if (!sendText(context, pkg, video)) return null
      commandAndWatch(context, pkg, { _ -> }, expectChange = true, timeoutMs = 8_000, blank = false)
        ?: return null
    } else {
      commandAndWatch(
        context,
        pkg,
        { controller -> controller.transportControls.playFromSearch(query, Bundle()) },
        expectChange = query.isNotBlank(),
        timeoutMs = if (query.isBlank()) 6_000 else 10_000,
        blank = query.isBlank(),
      )
    }
  }

  private fun playYouTubeMusic(context: Context, query: String, title: String, artist: String, album: String): Hit? {
    if (installedPackage(context, listOf(YOUTUBE_MUSIC)) == null) return null
    val intent = Intent("android.media.action.MEDIA_PLAY_FROM_SEARCH")
      .addCategory(Intent.CATEGORY_DEFAULT)
      .setPackage(YOUTUBE_MUSIC)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      .putExtra(android.app.SearchManager.QUERY, query)
    if (title.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_TITLE, title)
    if (artist.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_ARTIST, artist)
    if (album.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_ALBUM, album)
    val focus = when {
      album.isNotBlank() -> "vnd.android.cursor.item/album"
      artist.isNotBlank() && title.isBlank() -> "vnd.android.cursor.item/artist"
      else -> "vnd.android.cursor.item/audio"
    }
    intent.putExtra(MediaStore.EXTRA_MEDIA_FOCUS, focus)
    context.startActivity(intent)
    val code = if (query.isBlank() && title.isBlank() && artist.isBlank() && album.isBlank()) "music-ytmusic-resume" else "music-ytmusic"
    return Hit(code, via = "play-from-search")
  }

  private fun openYouTube(context: Context, query: String): Hit? {
    if (installedPackage(context, listOf(YOUTUBE)) == null) return null
    val intent = Intent(Intent.ACTION_VIEW, youtubeUri(query))
      .setPackage(YOUTUBE)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(intent)
    val code = when {
      videoUrl(query) != null -> "music-youtube-video"
      query.isBlank() -> "music-youtube-home"
      else -> "music-youtube-app"
    }
    return Hit(code, via = "view")
  }

  private fun openBrowser(context: Context, query: String): Hit? {
    val pkg = browserPackage(context) ?: return null
    val intent = Intent(Intent.ACTION_VIEW, youtubeUri(query))
      .addCategory(Intent.CATEGORY_BROWSABLE)
      .setPackage(pkg)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(intent)
    val code = when {
      videoUrl(query) != null -> "music-browser-video"
      query.isBlank() -> "music-browser-home"
      else -> "music-youtube-browser"
    }
    return Hit(code, via = "browser")
  }

  private fun playSystem(context: Context, query: String, title: String, artist: String, album: String): Hit? {
    val intent = Intent("android.media.action.MEDIA_PLAY_FROM_SEARCH")
      .addCategory(Intent.CATEGORY_DEFAULT)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      .putExtra(android.app.SearchManager.QUERY, query)
    if (title.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_TITLE, title)
    if (artist.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_ARTIST, artist)
    if (album.isNotBlank()) intent.putExtra(MediaStore.EXTRA_MEDIA_ALBUM, album)
    val handlers = queryActivities(context, intent).filter { it.activityInfo?.packageName != context.packageName }
    if (handlers.isEmpty()) return null
    if (handlers.size == 1) {
      val info = handlers[0].activityInfo ?: return null
      intent.setClassName(info.packageName, info.name)
      context.startActivity(intent)
      val code = if (query.isBlank() && title.isBlank()) "music-system-resume" else "music-system"
      return Hit(code, via = "play-from-search")
    }
    val chooser = Intent.createChooser(intent, AppLocale.wrap(context).getString(R.string.music_pick_player))
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(chooser)
    return Hit("music-system-ask", via = "chooser")
  }

  private fun probe(context: Context, pkg: String, timeoutMs: Long): Boolean {
    if (Looper.myLooper() == Looper.getMainLooper()) return false
    val done = CountDownLatch(1)
    val ok = AtomicBoolean(false)
    val main = Handler(Looper.getMainLooper())
    val holder = arrayOfNulls<MediaBrowser>(1)
    main.post {
      val browser = MediaBrowser(
        context,
        ComponentName(pkg, MUSIFY_SERVICE),
        object : MediaBrowser.ConnectionCallback() {
          override fun onConnected() {
            ok.set(true)
            finishProbe(holder, done)
          }

          override fun onConnectionFailed() {
            finishProbe(holder, done)
          }
        },
        null,
      )
      holder[0] = browser
      try {
        browser.connect()
      } catch (_: Exception) {
        done.countDown()
      }
      main.postDelayed({ finishProbe(holder, done) }, timeoutMs)
    }
    done.await(timeoutMs + 400, TimeUnit.MILLISECONDS)
    return ok.get()
  }

  private fun finishProbe(holder: Array<MediaBrowser?>, done: CountDownLatch) {
    if (done.count == 0L) return
    try {
      holder[0]?.disconnect()
    } catch (_: Exception) {
    }
    done.countDown()
  }

  private fun commandAndWatch(
    context: Context,
    pkg: String,
    command: (MediaController) -> Unit,
    expectChange: Boolean,
    timeoutMs: Long,
    blank: Boolean,
  ): Hit? {
    if (Looper.myLooper() == Looper.getMainLooper()) return null
    val done = CountDownLatch(1)
    val hit = AtomicReference<Hit?>(null)
    val main = Handler(Looper.getMainLooper())
    val holder = arrayOfNulls<MediaBrowser>(1)
    val finished = AtomicBoolean(false)
    fun finish(value: Hit?) {
      if (!finished.compareAndSet(false, true)) return
      hit.set(value)
      main.post {
        try {
          holder[0]?.disconnect()
        } catch (_: Exception) {
        }
        done.countDown()
      }
    }
    main.post {
      val browser = MediaBrowser(
        context,
        ComponentName(pkg, MUSIFY_SERVICE),
        object : MediaBrowser.ConnectionCallback() {
          override fun onConnected() {
            if (finished.get()) return
            val current = holder[0] ?: return
            try {
              val controller = MediaController(context, current.sessionToken)
              val beforeId = controller.metadata?.getString(android.media.MediaMetadata.METADATA_KEY_MEDIA_ID).orEmpty()
              val beforeTitle = controller.metadata?.getString(android.media.MediaMetadata.METADATA_KEY_TITLE).orEmpty()
              val beforeState = controller.playbackState?.state
              val beforePlaying = beforeState == PlaybackState.STATE_PLAYING || beforeState == PlaybackState.STATE_BUFFERING
              if (blank && beforePlaying) {
                finish(Hit("music-musify-resume", beforeTitle, artistOf(controller), "session"))
                return
              }
              val callback = object : MediaController.Callback() {
                override fun onMetadataChanged(metadata: android.media.MediaMetadata?) {
                  consider(controller, beforeId, beforeTitle, beforePlaying, expectChange, blank)
                }

                override fun onPlaybackStateChanged(state: PlaybackState?) {
                  consider(controller, beforeId, beforeTitle, beforePlaying, expectChange, blank)
                }

                private fun consider(
                  controller: MediaController,
                  beforeId: String,
                  beforeTitle: String,
                  beforePlaying: Boolean,
                  expectChange: Boolean,
                  blank: Boolean,
                ) {
                  val metadata = controller.metadata
                  val id = metadata?.getString(android.media.MediaMetadata.METADATA_KEY_MEDIA_ID).orEmpty()
                  val title = metadata?.getString(android.media.MediaMetadata.METADATA_KEY_TITLE).orEmpty()
                  val state = controller.playbackState?.state
                  val playing = state == PlaybackState.STATE_PLAYING || state == PlaybackState.STATE_BUFFERING
                  val changed = (id.isNotEmpty() && id != beforeId) || (title.isNotEmpty() && title != beforeTitle)
                  val ready = when {
                    blank -> playing
                    expectChange -> playing && (changed || (!beforePlaying && title.isNotEmpty()))
                    else -> playing
                  }
                  if (!ready) return
                  val code = when {
                    blank -> "music-musify-resume"
                    title.isBlank() -> "music-musify-started"
                    else -> "music-musify-playing"
                  }
                  finish(Hit(code, title, artistOf(controller), "media-browser"))
                }
              }
              controller.registerCallback(callback, main)
              if (blank && controller.playbackState?.state == PlaybackState.STATE_PAUSED) {
                controller.transportControls.play()
              } else {
                command(controller)
              }
              main.postDelayed({
                try {
                  controller.unregisterCallback(callback)
                } catch (_: Exception) {
                }
                finish(null)
              }, timeoutMs)
            } catch (error: Exception) {
              Log.w(TAG, "musify session command failed", error)
              finish(null)
            }
          }

          override fun onConnectionFailed() {
            if (!finished.get()) finish(null)
          }
        },
        null,
      )
      holder[0] = browser
      try {
        browser.connect()
      } catch (error: Exception) {
        Log.w(TAG, "musify browser connect failed", error)
        finish(null)
      }
      main.postDelayed({ finish(null) }, timeoutMs + 1_000)
    }
    done.await(timeoutMs + 1_500, TimeUnit.MILLISECONDS)
    return hit.get()
  }

  private fun artistOf(controller: MediaController): String {
    return controller.metadata?.getString(android.media.MediaMetadata.METADATA_KEY_ARTIST).orEmpty()
  }

  private fun sendText(context: Context, pkg: String, text: String): Boolean {
    val intent = Intent(Intent.ACTION_SEND)
      .setType("text/plain")
      .putExtra(Intent.EXTRA_TEXT, text)
      .setPackage(pkg)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      true
    } catch (error: Exception) {
      Log.w(TAG, "musify share failed", error)
      false
    }
  }

  private fun launch(context: Context, pkg: String): Boolean {
    val intent = context.packageManager.getLaunchIntentForPackage(pkg) ?: return false
    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      true
    } catch (error: Exception) {
      Log.w(TAG, "musify launch failed", error)
      false
    }
  }

  private fun transport(context: Context, controller: MediaController, action: String, player: String): String {
    return try {
      val controls = controller.transportControls
      val code = when (action) {
        "play" -> {
          controls.play()
          "media-play"
        }
        "pause" -> {
          controls.pause()
          "media-pause"
        }
        "stop" -> {
          controls.stop()
          "media-stop"
        }
        "next" -> {
          controls.skipToNext()
          "media-next"
        }
        "previous" -> {
          controls.skipToPrevious()
          "media-previous"
        }
        "shuffle" -> {
          shuffle(context, controller, PlaybackStateCompat.SHUFFLE_MODE_ALL)
          "media-shuffle"
        }
        "unshuffle" -> {
          shuffle(context, controller, PlaybackStateCompat.SHUFFLE_MODE_NONE)
          "media-unshuffle"
        }
        else -> return result(false, "media-bad", "", "", player, "", "", "")
      }
      result(true, code, "", "", player, "session", "", "")
    } catch (error: Exception) {
      Log.w(TAG, "media session $action failed", error)
      if (action == "shuffle" || action == "unshuffle") {
        result(false, "media-shuffle-none", "", "", player, "", "", "")
      } else {
        result(false, "media-bad", "", "", player, "", "", "")
      }
    }
  }

  private fun shuffle(context: Context, controller: MediaController, mode: Int) {
    val token = MediaSessionCompat.Token.fromToken(controller.sessionToken)
    MediaControllerCompat(context, token).transportControls.setShuffleMode(mode)
  }

  private fun activeSessions(context: Context): List<MediaController>? {
    val manager = context.getSystemService(MediaSessionManager::class.java) ?: return null
    val listener = ComponentName(context, NoticeListener::class.java)
    return try {
      manager.getActiveSessions(listener)
    } catch (_: SecurityException) {
      null
    }
  }

  private fun dispatchKey(context: Context, action: String) {
    val code = when (action) {
      "play" -> KeyEvent.KEYCODE_MEDIA_PLAY
      "pause" -> KeyEvent.KEYCODE_MEDIA_PAUSE
      "stop" -> KeyEvent.KEYCODE_MEDIA_STOP
      "next" -> KeyEvent.KEYCODE_MEDIA_NEXT
      "previous" -> KeyEvent.KEYCODE_MEDIA_PREVIOUS
      else -> return
    }
    val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val now = android.os.SystemClock.uptimeMillis()
    audio.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0))
    audio.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0))
  }

  private fun packagesFor(player: String): List<String> {
    return when (player) {
      "musify" -> MUSIFY_PACKAGES
      "youtube" -> listOf(YOUTUBE)
      "youtube_music" -> listOf(YOUTUBE_MUSIC)
      else -> emptyList()
    }
  }

  private fun browserPackage(context: Context): String? {
    val probe = Intent(Intent.ACTION_VIEW, Uri.parse("https://example.com/")).addCategory(Intent.CATEGORY_BROWSABLE)
    val skip = MUSIFY_PACKAGES + listOf(YOUTUBE, YOUTUBE_MUSIC)
    return queryActivities(context, probe)
      .firstOrNull { info ->
        val pkg = info.activityInfo?.packageName ?: return@firstOrNull false
        pkg !in skip
      }
      ?.activityInfo
      ?.packageName
  }

  private fun youtubeUri(query: String): Uri {
    videoUrl(query)?.let { return Uri.parse(it) }
    if (query.isBlank()) return Uri.parse("https://www.youtube.com/")
    return Uri.parse("https://www.youtube.com/results?search_query=" + Uri.encode(query))
  }

  private fun videoUrl(raw: String): String? {
    val text = raw.trim()
    val lower = text.lowercase()
    for (marker in listOf("youtu.be/", "watch?v=", "watch?v%3d", "/shorts/", "/embed/", "/live/")) {
      val pos = lower.indexOf(marker)
      if (pos < 0) continue
      val rest = text.substring(pos + marker.length)
      val id = rest.takeWhile { it.isLetterOrDigit() || it == '_' || it == '-' }
      if (id.length == 11) return "https://www.youtube.com/watch?v=$id"
    }
    return null
  }

  private fun installedPackage(context: Context, packages: List<String>): String? {
    val pm = context.packageManager
    return packages.firstOrNull { pkg ->
      try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
          pm.getPackageInfo(pkg, PackageManager.PackageInfoFlags.of(0))
        } else {
          @Suppress("DEPRECATION")
          pm.getPackageInfo(pkg, 0)
        }
        true
      } catch (_: PackageManager.NameNotFoundException) {
        false
      }
    }
  }

  private fun queryActivities(context: Context, intent: Intent): List<ResolveInfo> {
    val pm = context.packageManager
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      pm.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong()))
    } else {
      @Suppress("DEPRECATION")
      pm.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
    }
  }

  private fun canonicalPlayer(raw: String): String {
    return when (raw.trim().lowercase()) {
      "musify" -> "musify"
      "youtube" -> "youtube"
      "youtube_music", "youtubemusic" -> "youtube_music"
      "system", "ask" -> "system"
      else -> "local"
    }
  }

  private fun clip(value: String): String {
    return value.trim().filter { !it.isISOControl() }.take(180)
  }

  private fun row(id: String, installed: Boolean, pkg: String): JSONObject {
    return JSONObject().put("id", id).put("installed", installed).put("package", pkg)
  }

  private fun result(
    ok: Boolean,
    code: String,
    fallbackFrom: String,
    query: String,
    player: String,
    via: String,
    title: String,
    artist: String,
  ): String {
    return JSONObject()
      .put("ok", ok)
      .put("code", code)
      .put("fallbackFrom", fallbackFrom)
      .put("query", query)
      .put("player", player)
      .put("via", via)
      .put("title", title)
      .put("artist", artist)
      .toString()
  }
}
