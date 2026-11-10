package ai.grokvoice.app

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ContentUris
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.Looper
import android.provider.MediaStore
import android.provider.Settings
import android.util.Log
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/**
 * Plays audio files and playlists already on the phone.
 * Music uses the media stream and drops the player while a spoken alert holds the alarm stream.
 */
object LocalMusic {
  const val ACTION_SHOW = "ai.grokvoice.app.music.SHOW"
  const val ACTION_PAUSE = "ai.grokvoice.app.music.PAUSE"
  const val ACTION_STOP = "ai.grokvoice.app.music.STOP"
  const val NOTICE_ID = 91026

  private const val TAG = "GrokVoice"
  private const val CHANNEL = "grok_music"
  private const val MAX_TRACKS = 2500
  private const val MAX_PLAYLISTS = 80
  private const val MAX_MEMBERS = 200

  private val playerThread = HandlerThread("GrokMusic").apply { start() }
  private val playerHandler = Handler(playerThread.looper)
  private var player: MediaPlayer? = null
  private var session: MediaSession? = null
  private var queue: List<Item> = emptyList()
  private var index = 0
  private var generation = 0
  private var active = false
  private var pausedByFocus = false
  private var focus: AudioFocusRequest? = null
  private var current: Item? = null
  private var pendingPermission: ((Boolean, Boolean) -> Unit)? = null

  private data class Item(val uri: String, val title: String, val artist: String, val album: String)

  fun permissionName(): String {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      Manifest.permission.READ_MEDIA_AUDIO
    } else {
      Manifest.permission.READ_EXTERNAL_STORAGE
    }
  }

  fun canRead(context: Context): Boolean {
    return ContextCompat.checkSelfPermission(context, permissionName()) == PackageManager.PERMISSION_GRANTED
  }

  fun requestAccess(activity: android.app.Activity, done: (Boolean, Boolean) -> Unit) {
    if (canRead(activity)) {
      done(true, false)
      return
    }
    val permission = permissionName()
    val prefs = activity.getSharedPreferences("grok_music", Context.MODE_PRIVATE)
    val asked = prefs.getBoolean("asked", false)
    val rationale = androidx.core.app.ActivityCompat.shouldShowRequestPermissionRationale(activity, permission)
    if (asked && !rationale) {
      openSettings(activity)
      done(false, true)
      return
    }
    prefs.edit().putBoolean("asked", true).apply()
    val host = activity as? MainActivity
    if (host == null) {
      done(false, false)
      return
    }
    pendingPermission?.invoke(false, false)
    pendingPermission = done
    host.askAudioPermission(permission)
  }

  fun onPermissionResult(granted: Boolean) {
    val done = pendingPermission ?: return
    pendingPermission = null
    done(granted, false)
  }

  fun catalog(context: Context): String {
    if (!canRead(context)) return denied()
    return try {
      val tracks = JSONArray()
      queryTracks(context, tracks)
      JSONObject()
        .put("ok", true)
        .put("permission", "granted")
        .put("tracks", tracks)
        .put("playlists", queryPlaylists(context))
        .toString()
    } catch (_: SecurityException) {
      denied()
    } catch (t: Exception) {
      Log.w(TAG, "music scan failed", t)
      JSONObject().put("ok", false).put("permission", "granted").put("error", "failed").toString()
    }
  }

  fun play(context: Context, json: String): String {
    val app = context.applicationContext
    val latch = CountDownLatch(1)
    val result = AtomicReference(fail())
    playerHandler.post { startPlayback(app, json, result, latch) }
    if (!latch.await(12, TimeUnit.SECONDS)) return fail()
    return result.get()
  }

  /** JSON when this player owns the audio, or null so media keys reach another app. */
  fun control(context: Context, action: String): String? {
    val app = context.applicationContext
    if (Looper.myLooper() == playerThread.looper) return controlOnMain(app, action)
    val latch = CountDownLatch(1)
    val out = AtomicReference<String?>(null)
    playerHandler.post {
      out.set(controlOnMain(app, action))
      latch.countDown()
    }
    latch.await(2, TimeUnit.SECONDS)
    return out.get()
  }

  fun notification(context: Context): Notification {
    val nm = context.getSystemService(NotificationManager::class.java)
    if (nm?.getNotificationChannel(CHANNEL) == null && nm != null) {
      val channel = NotificationChannel(CHANNEL, context.getString(R.string.music_channel_name), NotificationManager.IMPORTANCE_LOW)
      channel.description = context.getString(R.string.music_channel_desc)
      channel.setSound(null, null)
      channel.enableVibration(false)
      nm.createNotificationChannel(channel)
    }
    val item = current
    val title = item?.title?.ifBlank { context.getString(R.string.music_playing) } ?: context.getString(R.string.music_playing)
    val open = PendingIntent.getActivity(
      context,
      0,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    val style = Notification.MediaStyle().setShowActionsInCompactView(0, 1)
    session?.sessionToken?.let { style.setMediaSession(it) }
    return Notification.Builder(context, CHANNEL)
      .setSmallIcon(android.R.drawable.ic_media_play)
      .setContentTitle(title)
      .setContentText(item?.artist.orEmpty())
      .setContentIntent(open)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setCategory(Notification.CATEGORY_TRANSPORT)
      .setStyle(style)
      .addAction(Notification.Action(android.R.drawable.ic_media_pause, context.getString(R.string.music_pause), serviceIntent(context, ACTION_PAUSE, 1)))
      .addAction(Notification.Action(android.R.drawable.ic_menu_close_clear_cancel, context.getString(R.string.music_stop), serviceIntent(context, ACTION_STOP, 2)))
      .build()
  }

  private fun startPlayback(context: Context, json: String, result: AtomicReference<String>, latch: CountDownLatch) {
    val items = parseItems(json)
    if (items.isEmpty()) {
      finish(result, latch, fail())
      return
    }
    stopPlayer(context, releaseSession = false)
    queue = items
    index = 0
    generation += 1
    ensureSession(context)
    requestFocus(context)
    beginItem(context, generation, result, latch, 0)
  }

  private fun beginItem(
    context: Context,
    gen: Int,
    result: AtomicReference<String>?,
    latch: CountDownLatch?,
    attempts: Int,
  ) {
    val item = queue.getOrNull(index)
    if (item == null || attempts >= queue.size.coerceAtMost(4)) {
      finish(result, latch, fail())
      stopPlayer(context, releaseSession = true)
      return
    }
    val created = MediaPlayer()
    player = created
    created.setAudioAttributes(mediaAttributes())
    created.setOnPreparedListener {
      if (gen != generation) return@setOnPreparedListener
      it.start()
      active = true
      pausedByFocus = false
      current = item
      publish(item, playing = true)
      showForeground(context)
      finish(result, latch, ok())
    }
    created.setOnCompletionListener {
      if (gen != generation) return@setOnCompletionListener
      playerHandler.post {
        if (gen != generation) return@post
        index += 1
        releasePlayer()
        if (index < queue.size) beginItem(context, gen, null, null, 0) else stopPlayer(context, releaseSession = true)
      }
    }
    created.setOnErrorListener { _, _, _ ->
      if (gen != generation) return@setOnErrorListener true
      playerHandler.post {
        if (gen != generation) return@post
        index += 1
        releasePlayer()
        beginItem(context, gen, result, latch, attempts + 1)
      }
      true
    }
    try {
      created.setDataSource(context, Uri.parse(item.uri))
      created.prepareAsync()
    } catch (t: Exception) {
      Log.w(TAG, "music file skipped", t)
      created.setOnErrorListener(null)
      releasePlayer()
      index += 1
      beginItem(context, gen, result, latch, attempts + 1)
    }
  }

  private fun controlOnMain(context: Context, action: String): String? {
    if (!active) return null
    return when (action) {
      "pause" -> {
        player?.pause()
        pausedByFocus = false
        current?.let { publish(it, playing = false) }
        abandonFocus(context)
        hideForeground(context)
        ok("Paused.")
      }
      "stop" -> {
        stopPlayer(context, releaseSession = true)
        ok("Stopped.")
      }
      "next" -> skip(context, 1)
      "previous" -> {
        val position = try { player?.currentPosition ?: 0 } catch (_: Exception) { 0 }
        if (position > 3_000) {
          player?.seekTo(0)
          ok("Previous track.")
        } else {
          skip(context, -1)
        }
      }
      "play" -> {
        if (player == null) return null
        requestFocus(context)
        player?.start()
        pausedByFocus = false
        current?.let { publish(it, playing = true) }
        showForeground(context)
        ok("Playing.")
      }
      else -> null
    }
  }

  private fun skip(context: Context, delta: Int): String {
    if (queue.isEmpty()) return ok("Next track.")
    index = if (delta < 0) {
      if (index == 0) 0 else index - 1
    } else if (index + 1 < queue.size) {
      index + 1
    } else {
      index
    }
    generation += 1
    releasePlayer()
    beginItem(context, generation, null, null, 0)
    return if (delta < 0) ok("Previous track.") else ok("Next track.")
  }

  private fun stopPlayer(context: Context, releaseSession: Boolean) {
    generation += 1
    active = false
    pausedByFocus = false
    queue = emptyList()
    index = 0
    current = null
    releasePlayer()
    abandonFocus(context)
    if (releaseSession) {
      session?.isActive = false
      session?.release()
      session = null
    }
    hideForeground(context)
  }

  private fun releasePlayer() {
    val currentPlayer = player ?: return
    player = null
    try {
      currentPlayer.setOnCompletionListener(null)
      currentPlayer.setOnErrorListener(null)
      currentPlayer.reset()
      currentPlayer.release()
    } catch (_: Exception) {
    }
  }

  private fun ensureSession(context: Context) {
    if (session != null) return
    val created = MediaSession(context, "GrokVoiceMusic")
    created.setCallback(object : MediaSession.Callback() {
      override fun onPlay() { control(context, "play") }
      override fun onPause() { control(context, "pause") }
      override fun onStop() { control(context, "stop") }
      override fun onSkipToNext() { control(context, "next") }
      override fun onSkipToPrevious() { control(context, "previous") }
    })
    created.isActive = true
    session = created
  }

  private fun publish(item: Item, playing: Boolean) {
    val created = session ?: return
    val state = if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED
    val position = try { player?.currentPosition?.toLong() ?: 0L } catch (_: Exception) { 0L }
    created.setPlaybackState(
      PlaybackState.Builder()
        .setActions(
          PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_STOP or
            PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS
        )
        .setState(state, position, if (playing) 1f else 0f)
        .build()
    )
    created.setMetadata(
      android.media.MediaMetadata.Builder()
        .putString(android.media.MediaMetadata.METADATA_KEY_TITLE, item.title)
        .putString(android.media.MediaMetadata.METADATA_KEY_ARTIST, item.artist)
        .putString(android.media.MediaMetadata.METADATA_KEY_ALBUM, item.album)
        .build()
    )
  }

  private fun requestFocus(context: Context) {
    if (focus != null) return
    val am = context.getSystemService(AudioManager::class.java) ?: return
    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
      .setAudioAttributes(mediaAttributes())
      .setWillPauseWhenDucked(true)
      .setOnAudioFocusChangeListener({ change -> onFocus(context, change) }, playerHandler)
      .build()
    if (am.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
      focus = request
    }
  }

  private fun onFocus(context: Context, change: Int) {
    when (change) {
      AudioManager.AUDIOFOCUS_LOSS_TRANSIENT, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
        if (playingNow()) {
          pausedByFocus = true
          player?.pause()
          current?.let { publish(it, playing = false) }
        }
      }
      AudioManager.AUDIOFOCUS_GAIN -> {
        if (pausedByFocus && active) {
          pausedByFocus = false
          player?.start()
          current?.let { publish(it, playing = true) }
        }
      }
      AudioManager.AUDIOFOCUS_LOSS -> {
        pausedByFocus = false
        if (active) {
          player?.pause()
          current?.let { publish(it, playing = false) }
          abandonFocus(context)
          hideForeground(context)
        }
      }
    }
  }

  private fun abandonFocus(context: Context) {
    val request = focus ?: return
    focus = null
    val am = context.getSystemService(AudioManager::class.java) ?: return
    try {
      am.abandonAudioFocusRequest(request)
    } catch (_: Exception) {
    }
  }

  private fun showForeground(context: Context) {
    val intent = Intent(context, LocalMusicService::class.java).setAction(ACTION_SHOW)
    try {
      ContextCompat.startForegroundService(context, intent)
    } catch (t: Exception) {
      Log.w(TAG, "music stays in this process without a foreground notification", t)
    }
  }

  private fun hideForeground(context: Context) {
    try {
      context.stopService(Intent(context, LocalMusicService::class.java))
    } catch (_: Exception) {
    }
  }

  private fun playingNow(): Boolean {
    return try {
      player?.isPlaying == true
    } catch (_: Exception) {
      false
    }
  }

  private fun mediaAttributes(): AudioAttributes {
    return AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_MEDIA)
      .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
      .build()
  }

  private fun queryTracks(context: Context, rows: JSONArray) {
    val uri = MediaStore.Audio.Media.EXTERNAL_CONTENT_URI
    val selection = "${MediaStore.Audio.Media.IS_ALARM}=0 AND ${MediaStore.Audio.Media.IS_NOTIFICATION}=0 AND ${MediaStore.Audio.Media.IS_RINGTONE}=0 AND ${MediaStore.Audio.Media.DURATION}>=3000"
    val cursor = context.contentResolver.query(
      uri,
      arrayOf(
        MediaStore.Audio.Media._ID,
        MediaStore.Audio.Media.TITLE,
        MediaStore.Audio.Media.ARTIST,
        MediaStore.Audio.Media.ALBUM,
        MediaStore.Audio.Media.DISPLAY_NAME,
        MediaStore.Audio.Media.TRACK,
      ),
      selection,
      null,
      "${MediaStore.Audio.Media.TITLE} COLLATE LOCALIZED ASC",
    ) ?: return
    cursor.use {
      val idIdx = it.getColumnIndex(MediaStore.Audio.Media._ID)
      val titleIdx = it.getColumnIndex(MediaStore.Audio.Media.TITLE)
      val artistIdx = it.getColumnIndex(MediaStore.Audio.Media.ARTIST)
      val albumIdx = it.getColumnIndex(MediaStore.Audio.Media.ALBUM)
      val fileIdx = it.getColumnIndex(MediaStore.Audio.Media.DISPLAY_NAME)
      val trackIdx = it.getColumnIndex(MediaStore.Audio.Media.TRACK)
      if (idIdx < 0) return
      while (it.moveToNext() && rows.length() < MAX_TRACKS) {
        val id = it.getLong(idIdx)
        rows.put(row(
          id.toString(),
          text(it, titleIdx),
          text(it, artistIdx),
          text(it, albumIdx),
          text(it, fileIdx),
          ContentUris.withAppendedId(uri, id).toString(),
          if (trackIdx >= 0) it.getInt(trackIdx) else 0,
        ))
      }
    }
  }

  private fun queryPlaylists(context: Context): JSONArray {
    val out = JSONArray()
    try {
      val cursor = context.contentResolver.query(
        MediaStore.Audio.Playlists.EXTERNAL_CONTENT_URI,
        arrayOf(MediaStore.Audio.Playlists._ID, MediaStore.Audio.Playlists.NAME),
        null,
        null,
        "${MediaStore.Audio.Playlists.NAME} COLLATE LOCALIZED ASC",
      ) ?: return out
      cursor.use {
        val idIdx = it.getColumnIndex(MediaStore.Audio.Playlists._ID)
        val nameIdx = it.getColumnIndex(MediaStore.Audio.Playlists.NAME)
        if (idIdx < 0) return out
        while (it.moveToNext() && out.length() < MAX_PLAYLISTS) {
          val id = it.getLong(idIdx)
          val name = text(it, nameIdx)
          if (name.isEmpty()) continue
          val members = playlistMembers(context, id)
          if (members.length() == 0) continue
          out.put(JSONObject().put("id", id.toString()).put("name", name).put("tracks", members))
        }
      }
    } catch (t: Exception) {
      Log.w(TAG, "playlist scan failed", t)
    }
    return out
  }

  private fun playlistMembers(context: Context, playlistId: Long): JSONArray {
    val rows = JSONArray()
    val uri = MediaStore.Audio.Playlists.Members.getContentUri("external", playlistId)
    val cursor = try {
      context.contentResolver.query(
        uri,
        arrayOf(
          MediaStore.Audio.Playlists.Members.AUDIO_ID,
          MediaStore.Audio.Playlists.Members.TITLE,
          MediaStore.Audio.Playlists.Members.ARTIST,
          MediaStore.Audio.Playlists.Members.ALBUM,
          MediaStore.Audio.Playlists.Members.DISPLAY_NAME,
          MediaStore.Audio.Playlists.Members.TRACK,
        ),
        null,
        null,
        "${MediaStore.Audio.Playlists.Members.PLAY_ORDER} ASC",
      )
    } catch (_: Exception) {
      null
    } ?: return rows
    cursor.use {
      val idIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.AUDIO_ID)
      val titleIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.TITLE)
      val artistIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.ARTIST)
      val albumIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.ALBUM)
      val fileIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.DISPLAY_NAME)
      val trackIdx = it.getColumnIndex(MediaStore.Audio.Playlists.Members.TRACK)
      if (idIdx < 0) return rows
      while (it.moveToNext() && rows.length() < MAX_MEMBERS) {
        val id = it.getLong(idIdx)
        if (id <= 0L) continue
        rows.put(row(
          id.toString(),
          text(it, titleIdx),
          text(it, artistIdx),
          text(it, albumIdx),
          text(it, fileIdx),
          ContentUris.withAppendedId(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, id).toString(),
          if (trackIdx >= 0) it.getInt(trackIdx) else 0,
        ))
      }
    }
    return rows
  }

  private fun row(id: String, title: String, artist: String, album: String, file: String, uri: String, track: Int): JSONObject {
    return JSONObject()
      .put("id", id)
      .put("title", clean(title))
      .put("artist", clean(artist))
      .put("album", clean(album))
      .put("file", file)
      .put("uri", uri)
      .put("track", track)
  }

  private fun text(cursor: android.database.Cursor, index: Int): String {
    if (index < 0 || cursor.isNull(index)) return ""
    return cursor.getString(index) ?: ""
  }

  private fun clean(raw: String): String {
    val trimmed = raw.trim()
    val folded = trimmed.lowercase()
    if (folded.isEmpty() || folded == "<unknown>" || folded == "unknown" || folded == "<unknown artist>" || folded == "unknown artist" || folded == "<unknown album>") {
      return ""
    }
    return trimmed
  }

  private fun parseItems(json: String): List<Item> {
    val root = try { JSONObject(json) } catch (_: Exception) { return emptyList() }
    val rows = root.optJSONArray("items") ?: return emptyList()
    val items = ArrayList<Item>()
    for (i in 0 until rows.length()) {
      val row = rows.optJSONObject(i) ?: continue
      val uri = row.optString("uri")
      if (!uri.startsWith("content://media/")) continue
      items.add(Item(uri, row.optString("title"), row.optString("artist"), row.optString("album")))
    }
    return items
  }

  private fun openSettings(activity: android.app.Activity) {
    val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", activity.packageName, null))
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    try {
      activity.startActivity(intent)
    } catch (t: Exception) {
      Log.w(TAG, "music settings not opened", t)
    }
  }

  private fun serviceIntent(context: Context, action: String, request: Int): PendingIntent {
    val intent = Intent(context, LocalMusicService::class.java).setAction(action)
    return PendingIntent.getService(context, request, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  }

  private fun finish(result: AtomicReference<String>?, latch: CountDownLatch?, message: String) {
    if (latch == null || latch.count == 0L) return
    result?.set(message)
    latch.countDown()
  }

  private fun denied() = JSONObject().put("ok", false).put("permission", "denied").put("error", "denied").toString()
  private fun ok(detail: String = "") = JSONObject().put("ok", true).put("detail", detail).toString()
  private fun fail() = JSONObject().put("ok", false).put("error", "failed").toString()
}

/** Keeps local music audible if Talk leaves the screen. Separate from spoken alerts. */
class LocalMusicService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    when (intent?.action) {
      LocalMusic.ACTION_PAUSE -> LocalMusic.control(this, "pause")
      LocalMusic.ACTION_STOP -> LocalMusic.control(this, "stop")
      else -> {
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
          ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
        } else {
          0
        }
        try {
          ServiceCompat.startForeground(this, LocalMusic.NOTICE_ID, LocalMusic.notification(this), type)
        } catch (t: Exception) {
          Log.w("GrokVoice", "music could not enter the foreground", t)
          stopSelf()
        }
      }
    }
    return START_NOT_STICKY
  }
}
