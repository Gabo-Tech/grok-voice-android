package ai.grokvoice.app

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.LocaleList
import android.os.Looper
import android.os.PowerManager
import android.provider.OpenableColumns
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Plays the sound saved on an in-app alarm.
 *
 * A local file is a copy the user picked, played on the alarm stream.
 * A YouTube choice opens the official IFrame Player in [AlarmSoundActivity].
 * The video is not downloaded. If the file is missing, the link is invalid,
 * or the player cannot start, the phone's default alarm sound plays instead
 * and the alarm screen says why.
 *
 * Spoken mode pauses this sound, speaks its alert, then lets the sound continue.
 */
object AlarmSounds {
  const val ACTION_STOP = "ai.grokvoice.app.STOP_ALARM_SOUND"
  private const val TAG = "GrokVoice"
  private const val PLAY_LIMIT_MS = 120_000L
  private const val PREVIEW_MS = 8_000L
  private const val YOUTUBE_WAIT_MS = 12_000L
  private const val MAX_BYTES = 20L * 1024L * 1024L

  private val main = Handler(Looper.getMainLooper())
  private val speechHold = AtomicBoolean(false)
  private val audioExt = setOf("mp3", "m4a", "aac", "wav", "ogg", "opus", "flac", "mp4", "3gp", "webm", "amr", "oga")

  private var player: MediaPlayer? = null
  private var previewPlayer: MediaPlayer? = null
  private var wake: PowerManager.WakeLock? = null
  private var screen: AlarmSoundActivity? = null
  private var posted: Notification? = null
  private var appContext: Context? = null

  @Volatile private var activeId: Int = 0
  @Volatile private var channelId: String = ""
  @Volatile private var alarmTitle: String = ""
  @Volatile private var status: String = ""
  @Volatile private var kind: String = ""
  @Volatile private var soundRef: String = ""
  @Volatile private var videoId: String = ""
  @Volatile private var ringing: Boolean = false
  @Volatile private var youtubeFailed: Boolean = false
  @Volatile private var youtubeArmed: Boolean = false
  @Volatile private var usingFile: Boolean = false

  fun noticeId(): Int = if (activeId == 0) 1 else activeId

  fun isRinging(): Boolean = ringing

  fun statusText(): String = status

  fun youtubeVideoId(): String = if (kind == "youtube" && !youtubeFailed) videoId else ""

  fun foregroundNote(context: Context): Notification {
    return posted ?: minimalNote(context)
  }

  fun choice(kind: String, ref: String, label: String): Choice? {
    val cleanKind = kind.trim()
    val cleanRef = ref.trim()
    if (cleanRef.isEmpty()) return null
    if (cleanKind != "file" && cleanKind != "youtube") return null
    return Choice(cleanKind, cleanRef, label.trim())
  }

  /**
   * Starts the custom sound. Returns the in-app error when playback had to
   * fall back to the default alarm sound, or null when the chosen sound started.
   */
  fun begin(context: Context, id: Int, channel: String, title: String, choice: Choice, note: Notification): String? {
    val app = context.applicationContext
    appContext = app
    stopPlayback()
    ringing = true
    youtubeFailed = false
    youtubeArmed = false
    usingFile = false
    speechHold.set(false)
    activeId = id
    channelId = channel
    alarmTitle = title.ifBlank { "Alarm" }
    posted = note
    kind = choice.kind
    soundRef = choice.ref
    videoId = if (choice.kind == "youtube") choice.ref else ""
    holdWake(app)
    val failure = startChosen(app, choice)
    status = failure ?: when (choice.kind) {
      "file" -> text(app, R.string.alarm_file_playing)
      else -> text(app, R.string.alarm_youtube_starting)
    }
    if (failure != null) {
      updateNote(app, failure)
    }
    main.removeCallbacks(stopLater)
    main.postDelayed(stopLater, PLAY_LIMIT_MS)
    try {
      androidx.core.content.ContextCompat.startForegroundService(app, Intent(app, AlarmSoundService::class.java))
    } catch (t: Throwable) {
      Log.w(TAG, "alarm sound foreground service was blocked; playing on the alarm stream anyway", t)
    }
    openScreen(app, choice, status)
    return failure
  }

  fun stop(context: Context) {
    val app = context.applicationContext
    val id = noticeId()
    stopPlayback()
    ringing = false
    status = ""
    try {
      app.getSystemService(NotificationManager::class.java)?.cancel(id)
    } catch (_: Exception) {
    }
    posted = null
    screen?.let { activity ->
      screen = null
      activity.prepareToClose()
      if (!activity.isFinishing) activity.finish()
    }
    app.stopService(Intent(app, AlarmSoundService::class.java))
  }

  fun yieldForSpeech() {
    if (!ringing) return
    speechHold.set(true)
    val done = CountDownLatch(1)
    main.post {
      pausePlayer()
      screen?.pauseVideo()
      done.countDown()
    }
    try {
      done.await(500, TimeUnit.MILLISECONDS)
    } catch (_: InterruptedException) {
    }
  }

  fun resumeAfterSpeech() {
    if (!ringing) {
      speechHold.set(false)
      return
    }
    speechHold.set(false)
    main.post {
      if (youtubeArmed && !youtubeFailed) {
        releasePlayer()
        screen?.playVideo()
      } else {
        resumePlayer()
      }
    }
  }

  fun onYoutubePlaying(context: Context) {
    if (!ringing || youtubeFailed) return
    youtubeArmed = true
    if (speechHold.get()) {
      screen?.pauseVideo()
      return
    }
    releasePlayer()
    val message = text(context, R.string.alarm_youtube_playing)
    status = message
    updateNote(context, message)
    screen?.showStatus(message)
  }

  fun onYoutubeFailed(context: Context, message: String) {
    if (!ringing || youtubeFailed) return
    youtubeFailed = true
    youtubeArmed = false
    videoId = ""
    screen?.hidePlayer()
    if (!usingFile) startDefault(context.applicationContext)
    status = message
    updateNote(context, message)
    screen?.showStatus(message)
    Log.w(TAG, "alarm youtube fell back: $message")
  }

  fun onYoutubeClosed(context: Context) {
    if (!ringing || kind != "youtube" || youtubeFailed) return
    onYoutubeFailed(context, text(context, R.string.alarm_youtube_failed))
  }

  fun attach(activity: AlarmSoundActivity) {
    screen = activity
  }

  fun detach(activity: AlarmSoundActivity) {
    if (screen === activity) screen = null
  }

  fun preview(context: Context, relativePath: String): String {
    if (ringing) return "An alarm is already playing."
    val file = resolve(context, relativePath) ?: return "That audio file is missing. Pick it again."
    if (!file.isFile || file.length() == 0L) return "That audio file is missing. Pick it again."
    if (Looper.myLooper() == Looper.getMainLooper()) {
      return try {
        startPreview(context.applicationContext, file)
        ""
      } catch (t: Exception) {
        Log.w(TAG, "alarm preview failed", t)
        "Could not read that audio file."
      }
    }
    val error = arrayOfNulls<String>(1)
    val done = CountDownLatch(1)
    main.post {
      try {
        startPreview(context.applicationContext, file)
      } catch (t: Exception) {
        Log.w(TAG, "alarm preview failed", t)
        error[0] = "Could not read that audio file."
      }
      done.countDown()
    }
    try {
      done.await(3, TimeUnit.SECONDS)
    } catch (_: InterruptedException) {
    }
    return error[0] ?: ""
  }

  fun copyPicked(context: Context, uri: Uri, dir: File): Pair<String, String> {
    val resolver = context.contentResolver
    val display = displayName(context, uri)
    val mime = resolver.getType(uri).orEmpty()
    val ext = extension(display, mime) ?: throw IllegalArgumentException("Choose an audio file.")
    if (!dir.isDirectory && !dir.mkdirs()) {
      throw IllegalStateException("Could not read that audio file.")
    }
    val stem = sanitizeStem(display)
    var name = "$stem.$ext"
    var n = 2
    while (File(dir, name).exists()) {
      name = "$stem-$n.$ext"
      n += 1
      if (n > 50) throw IllegalStateException("Could not read that audio file.")
    }
    val dest = File(dir, name)
    val input = resolver.openInputStream(uri) ?: throw IllegalStateException("Could not read that audio file.")
    try {
      input.use { stream ->
        dest.outputStream().use { output ->
          val buf = ByteArray(64 * 1024)
          var total = 0L
          while (true) {
            val read = stream.read(buf)
            if (read < 0) break
            total += read.toLong()
            if (total > MAX_BYTES) {
              dest.delete()
              throw IllegalStateException("That audio file is too large.")
            }
            output.write(buf, 0, read)
          }
        }
      }
    } catch (t: Exception) {
      dest.delete()
      throw t
    }
    if (!dest.isFile || dest.length() == 0L) {
      dest.delete()
      throw IllegalStateException("Could not read that audio file.")
    }
    return "alarm-sounds/$name" to display.ifBlank { name }
  }

  fun fileProblem(context: Context, relativePath: String): String? {
    val file = resolve(context, relativePath)
    if (file == null || !file.isFile || file.length() == 0L) {
      return text(context, R.string.alarm_file_missing)
    }
    if (file.length() > MAX_BYTES) return text(context, R.string.alarm_file_missing)
    return null
  }

  private fun startChosen(context: Context, choice: Choice): String? {
    return when (choice.kind) {
      "file" -> {
        val problem = fileProblem(context, choice.ref)
        if (problem != null) {
          startDefault(context)
          problem
        } else {
          val file = resolve(context, choice.ref)
          if (file == null || !playFile(context, file)) {
            startDefault(context)
            text(context, R.string.alarm_file_missing)
          } else {
            usingFile = true
            null
          }
        }
      }
      "youtube" -> {
        if (!videoId.matches(Regex("^[A-Za-z0-9_-]{11}$"))) {
          startDefault(context)
          text(context, R.string.alarm_youtube_failed)
        } else if (!online(context)) {
          youtubeFailed = true
          startDefault(context)
          text(context, R.string.alarm_offline)
        } else if (mediaSilent(context)) {
          youtubeFailed = true
          startDefault(context)
          text(context, R.string.alarm_media_silent)
        } else {
          startDefault(context)
          null
        }
      }
      else -> {
        startDefault(context)
        text(context, R.string.alarm_youtube_failed)
      }
    }
  }

  private fun playFile(context: Context, file: File): Boolean {
    return try {
      val next = MediaPlayer()
      next.setAudioAttributes(alarmAttributes(AudioAttributes.CONTENT_TYPE_MUSIC))
      next.setDataSource(file.absolutePath)
      next.isLooping = true
      next.prepare()
      next.setVolume(1f, 1f)
      next.start()
      player = next
      true
    } catch (t: Exception) {
      Log.w(TAG, "alarm file could not play", t)
      false
    }
  }

  private fun startDefault(context: Context) {
    releasePlayer()
    usingFile = false
    val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
      ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
      ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
    if (uri == null) {
      Log.w(TAG, "alarm default sound is missing")
      return
    }
    try {
      val next = MediaPlayer()
      next.setAudioAttributes(alarmAttributes(AudioAttributes.CONTENT_TYPE_SONIFICATION))
      next.setDataSource(context, uri)
      next.isLooping = true
      next.prepare()
      next.setVolume(1f, 1f)
      next.start()
      player = next
    } catch (t: Exception) {
      Log.w(TAG, "alarm default sound could not play", t)
    }
  }

  private fun startPreview(context: Context, file: File) {
    releasePreview()
    val next = MediaPlayer()
    next.setAudioAttributes(alarmAttributes(AudioAttributes.CONTENT_TYPE_MUSIC))
    next.setDataSource(file.absolutePath)
    next.isLooping = false
    next.prepare()
    next.setVolume(1f, 1f)
    next.start()
    previewPlayer = next
    main.postDelayed({ releasePreview() }, PREVIEW_MS)
  }

  private fun pausePlayer() {
    val current = player ?: return
    try {
      if (current.isPlaying) current.pause()
    } catch (_: Exception) {
    }
  }

  private fun resumePlayer() {
    val current = player ?: return
    try {
      current.start()
    } catch (t: Exception) {
      Log.w(TAG, "alarm sound could not resume", t)
    }
  }

  private fun stopPlayback() {
    main.removeCallbacks(stopLater)
    releasePlayer()
    releasePreview()
    releaseWake()
    youtubeArmed = false
  }

  private fun releasePlayer() {
    val current = player
    player = null
    if (current == null) return
    try {
      current.stop()
    } catch (_: Exception) {
    }
    try {
      current.release()
    } catch (_: Exception) {
    }
  }

  private fun releasePreview() {
    val current = previewPlayer
    previewPlayer = null
    if (current == null) return
    try {
      current.stop()
    } catch (_: Exception) {
    }
    try {
      current.release()
    } catch (_: Exception) {
    }
  }

  private val stopLater = Runnable {
    val ctx = appContext
    if (ctx != null) stop(ctx) else stopPlayback()
  }

  fun youtubeWaitMs(): Long = YOUTUBE_WAIT_MS

  private fun openScreen(context: Context, choice: Choice, message: String) {
    val intent = Intent(context, AlarmSoundActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      .putExtra("title", alarmTitle)
      .putExtra("kind", choice.kind)
      .putExtra("ref", if (choice.kind == "youtube") videoId else choice.ref)
      .putExtra("status", message)
      .putExtra("tryYoutube", choice.kind == "youtube" && !youtubeFailed)
    try {
      context.startActivity(intent)
    } catch (t: Exception) {
      Log.w(TAG, "alarm screen was blocked; the notification can still open it", t)
    }
  }

  fun contentIntent(context: Context, id: Int, title: String, choice: Choice): PendingIntent {
    val intent = Intent(context, AlarmSoundActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      .putExtra("title", title)
      .putExtra("kind", choice.kind)
      .putExtra("ref", choice.ref)
    return PendingIntent.getActivity(
      context,
      id + 200_000,
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
  }

  fun stopIntent(context: Context, id: Int): PendingIntent {
    val intent = Intent(context, AlarmSoundService::class.java).setAction(ACTION_STOP)
    return PendingIntent.getService(
      context,
      id + 300_000,
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
  }

  private fun updateNote(context: Context, message: String) {
    val channel = channelId
    if (channel.isEmpty()) return
    val localized = localeContext(context)
    val open = contentIntent(context, activeId, alarmTitle, Choice(kind, soundRef, ""))
    val stop = stopIntent(context, activeId)
    val note = Notification.Builder(context, channel)
      .setSmallIcon(android.R.drawable.ic_lock_idle_alarm)
      .setContentTitle(alarmTitle.ifBlank { "Alarm" })
      .setContentText(message)
      .setStyle(Notification.BigTextStyle().bigText(message))
      .setContentIntent(open)
      .setFullScreenIntent(open, true)
      .addAction(android.R.drawable.ic_menu_close_clear_cancel, localized.getString(R.string.alarm_stop), stop)
      .setOngoing(true)
      .setAutoCancel(false)
      .setOnlyAlertOnce(true)
      .setCategory(Notification.CATEGORY_ALARM)
      .build()
    posted = note
    try {
      context.getSystemService(NotificationManager::class.java)?.notify(noticeId(), note)
    } catch (_: Exception) {
    }
  }

  private fun minimalNote(context: Context): Notification {
    val localized = localeContext(context)
    val channel = channelId.ifBlank { "grok_alarm_sound" }
    ensureSilentChannel(localized, channel)
    return Notification.Builder(context, channel)
      .setSmallIcon(android.R.drawable.ic_lock_idle_alarm)
      .setContentTitle(alarmTitle.ifBlank { "Alarm" })
      .setContentText(status.ifBlank { localized.getString(R.string.alarm_file_playing) })
      .setOngoing(true)
      .setCategory(Notification.CATEGORY_ALARM)
      .build()
  }

  private fun ensureSilentChannel(context: Context, id: String) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val nm = context.getSystemService(NotificationManager::class.java) ?: return
    if (nm.getNotificationChannel(id) != null) return
    val channel = android.app.NotificationChannel(id, context.getString(R.string.alarm_channel_name), NotificationManager.IMPORTANCE_HIGH)
    channel.description = context.getString(R.string.alarm_channel_desc)
    channel.setSound(null, null)
    channel.enableVibration(false)
    nm.createNotificationChannel(channel)
  }

  private fun alarmAttributes(contentType: Int): AudioAttributes {
    return AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_ALARM)
      .setContentType(contentType)
      .build()
  }

  private fun holdWake(context: Context) {
    releaseWake()
    val pm = context.getSystemService(PowerManager::class.java) ?: return
    try {
      val lock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "GrokVoice:alarm-sound")
      lock.setReferenceCounted(false)
      lock.acquire(PLAY_LIMIT_MS + 5_000L)
      wake = lock
    } catch (_: Exception) {
    }
  }

  private fun releaseWake() {
    val lock = wake
    wake = null
    if (lock?.isHeld == true) {
      try {
        lock.release()
      } catch (_: Exception) {
      }
    }
  }

  private fun online(context: Context): Boolean {
    val cm = context.getSystemService(ConnectivityManager::class.java) ?: return false
    val network = cm.activeNetwork ?: return false
    val caps = cm.getNetworkCapabilities(network) ?: return false
    return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
  }

  private fun mediaSilent(context: Context): Boolean {
    val am = context.getSystemService(android.media.AudioManager::class.java) ?: return false
    return am.getStreamVolume(android.media.AudioManager.STREAM_MUSIC) == 0
  }

  private fun text(context: Context, id: Int): String = localeContext(context).getString(id)

  fun localeContext(context: Context): Context {
    val tag = readSettings(context).optString("locale", "").trim().replace('_', '-')
    if (tag.isEmpty()) return context
    val locale = Locale.forLanguageTag(tag)
    if (locale.language.isNullOrEmpty()) return context
    val config = Configuration(context.resources.configuration)
    config.setLocale(locale)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
      config.setLocales(LocaleList(locale))
    }
    return context.createConfigurationContext(config)
  }

  private fun readSettings(context: Context): JSONObject {
    val file = File(appRoot(context), "settings.json")
    if (!file.isFile) return JSONObject()
    return try {
      JSONObject(file.readText())
    } catch (_: Exception) {
      JSONObject()
    }
  }

  fun appRoot(context: Context): File {
    return roots(context).firstOrNull { File(it, "agenda.json").isFile || File(it, "settings.json").isFile }
      ?: roots(context).first()
  }

  private fun roots(context: Context): List<File> {
    val data = context.dataDir
    val files = context.filesDir
    return listOf(data, File(files, "ai.grokvoice.app"), files, File(data, "ai.grokvoice.app"))
      .distinctBy { it.absolutePath }
  }

  private fun resolve(context: Context, relative: String): File? {
    val cleaned = relative.trim().replace('\\', "/")
    if (!cleaned.startsWith("alarm-sounds/") || cleaned.contains("..")) return null
    val name = cleaned.removePrefix("alarm-sounds/")
    if (name.isBlank() || name.contains('/') || !name.all { it.isLetterOrDigit() || it == '.' || it == '_' || it == '-' }) return null
    return roots(context).firstNotNullOfOrNull { root ->
      val dir = File(root, "alarm-sounds")
      val file = File(dir, name)
      if (!file.isFile) return@firstNotNullOfOrNull null
      try {
        val canonDir = dir.canonicalFile
        val canon = file.canonicalFile
        if (canon.parentFile == canonDir) canon else null
      } catch (_: Exception) {
        null
      }
    }
  }

  private fun displayName(context: Context, uri: Uri): String {
    var name = ""
    try {
      context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) name = cursor.getString(0) ?: ""
      }
    } catch (_: Exception) {
    }
    return name.filterNot { it.isISOControl() }.take(120)
  }

  private fun extension(name: String, mime: String): String? {
    val fromName = name.substringAfterLast('.', "").lowercase(Locale.US)
    val mimeOk = mime.isBlank() || mime.startsWith("audio/") || mime == "application/octet-stream" || mime == "video/mp4" || mime == "video/3gpp"
    if (audioExt.contains(fromName) && mimeOk) return fromName
    return when (mime.lowercase(Locale.US)) {
      "audio/mpeg", "audio/mp3" -> "mp3"
      "audio/mp4", "audio/m4a", "audio/x-m4a" -> "m4a"
      "audio/aac" -> "aac"
      "audio/wav", "audio/x-wav", "audio/wave" -> "wav"
      "audio/ogg" -> "ogg"
      "audio/opus" -> "opus"
      "audio/flac" -> "flac"
      "audio/3gpp" -> "3gp"
      "audio/webm" -> "webm"
      "audio/amr" -> "amr"
      else -> null
    }
  }

  private fun sanitizeStem(raw: String): String {
    val base = raw.substringAfterLast('/').substringAfterLast('\\').trim()
    val dot = base.lastIndexOf('.')
    val stem = if (dot > 0 && audioExt.contains(base.substring(dot + 1).lowercase(Locale.US))) base.substring(0, dot) else base
    val out = StringBuilder()
    var dash = false
    for (ch in stem) {
      if (ch.isLetterOrDigit() && ch.code < 128) {
        out.append(ch.lowercaseChar())
        dash = false
      } else if (!dash && out.isNotEmpty()) {
        out.append('-')
        dash = true
      }
      if (out.length >= 40) break
    }
    while (out.endsWith("-")) out.deleteCharAt(out.length - 1)
    return if (out.isEmpty()) "audio" else out.toString()
  }

  data class Choice(val kind: String, val ref: String, val label: String)
}
