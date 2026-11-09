package ai.grokvoice.app

import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.io.File
import java.util.Calendar
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * Speaks a notification that was already posted. Normal mode returns immediately.
 * Spoken mode uses Piper when that language pack is on the phone, then Android
 * text-to-speech. It does not start a Talk session and does not touch the
 * voice-minute budget.
 *
 * Playback uses the alarm stream from a short foreground service. A background
 * receiver that plays on the media or notification stream is silenced on
 * Android 17 (this app targets SDK 37), which leaves only the notification
 * sound. A failure is written to logcat under GrokVoice.
 */
object SpokenAlerts {
  private const val TAG = "GrokVoice"
  private val worker = Executors.newSingleThreadExecutor { runnable ->
    Thread(runnable, "spoken-notice")
  }

  fun enqueue(context: Context, id: Int, kind: String, title: String, body: String, channelId: String, category: String) {
    val app = context.applicationContext
    val file = File(appRoot(app), "settings.json")
    if (!file.isFile) {
      val tried = roots(app).joinToString { it.absolutePath }
      Log.w(TAG, "spoken notice settings.json not found; tried $tried")
    }
    val delivery = readSettings(app).optString("notificationDelivery", "")
    if (delivery != "spoken") {
      Log.i(TAG, "spoken notice quiet: notificationDelivery=${delivery.ifEmpty { "missing" }} kind=$kind settings=${file.path} exists=${file.isFile}")
      return
    }
    Log.i(TAG, "spoken notice delivery=spoken kind=$kind settings=${file.path}")
    val intent = SpokenNoticeService.intent(app, id, kind, title, body, channelId, category)
    try {
      ContextCompat.startForegroundService(app, intent)
    } catch (t: Throwable) {
      Log.w(TAG, "spoken notice foreground service was blocked; speaking on the alarm stream from the receiver", t)
      speakQueued(app, intent) {}
    }
  }

  /** Runs after notices already queued, so a receiver can stay awake until speech ends. */
  fun afterQueue(done: () -> Unit) {
    worker.execute {
      try {
        done()
      } catch (t: Throwable) {
        Log.w(TAG, "spoken notice finish failed", t)
      }
    }
  }

  fun speakQueued(context: Context, intent: Intent, done: () -> Unit) {
    val app = context.applicationContext
    val id = intent.getIntExtra("id", 0)
    val kind = intent.getStringExtra("kind") ?: "reminder"
    val title = intent.getStringExtra("title") ?: ""
    val body = intent.getStringExtra("body") ?: ""
    val channelId = intent.getStringExtra("channelId") ?: ""
    val category = intent.getStringExtra("category") ?: "reminder"
    worker.execute {
      val wake = wakeLock(app)
      try {
        speak(app, id, kind, title, body, channelId, category)
      } catch (t: Throwable) {
        Log.w(TAG, "spoken notice failed", t)
      } finally {
        if (wake?.isHeld == true) wake.release()
        done()
      }
    }
  }

  private fun speak(context: Context, id: Int, kind: String, title: String, body: String, channelId: String, category: String) {
    if (!RustBridge.loadCore()) {
      Log.w(TAG, "spoken notice skipped: Rust core failed to load")
      return
    }
    val specJson = spec(context, id, kind, channelId, category)
    val line = RustBridge.planSpokenNotice(specJson, title, body).orEmpty()
    if (line.isEmpty()) {
      Log.w(TAG, "spoken notice produced no line for kind=$kind; spec=$specJson")
      return
    }
    Log.i(TAG, "spoken notice saying: $line")
    // The notification tone starts as the banner is posted. Give it a moment so
    // the sentence is not covered by that sound.
    try {
      Thread.sleep(600)
    } catch (_: InterruptedException) {
    }
    waitForTalk()
    if (RustBridge.talkOutputBusy()) {
      Log.w(TAG, "spoken notice skipped: Talk is still speaking")
      return
    }
    val am = context.getSystemService(AudioManager::class.java)
    if (am == null) {
      Log.w(TAG, "spoken notice skipped: no AudioManager")
      return
    }
    if (inCall(am)) {
      Log.w(TAG, "spoken notice skipped: phone call in progress")
      return
    }
    if (am.getStreamVolume(AudioManager.STREAM_ALARM) == 0) {
      Log.w(TAG, "spoken notice alarm stream volume is 0, so the sentence will be silent")
    }
    val focus = requestFocus(am)
    if (focus == null) {
      Log.w(TAG, "spoken notice audio focus denied; speaking on the alarm stream anyway")
    }
    RustBridge.setNoticeSpeaking(true)
    try {
      if (!playPiper(context, line) && !playAndroid(context, line)) {
        Log.w(TAG, "could not speak the notice")
      } else {
        Log.i(TAG, "spoken notice finished")
      }
    } finally {
      RustBridge.setNoticeSpeaking(false)
      if (focus != null) abandonFocus(am, focus)
    }
  }

  private fun spec(context: Context, id: Int, kind: String, channelId: String, category: String): String {
    val settings = readSettings(context)
    val due = dueOf(context, id)
    val nm = context.getSystemService(NotificationManager::class.java)
    val enabled = nm?.areNotificationsEnabled() ?: true
    var blocked = false
    var bypass = false
    if (nm != null && channelId.isNotEmpty()) {
      val channel = nm.getNotificationChannel(channelId)
      if (channel != null) {
        blocked = channel.importance == NotificationManager.IMPORTANCE_NONE
        bypass = channel.canBypassDnd()
      }
    }
    val filter = when (nm?.currentInterruptionFilter) {
      NotificationManager.INTERRUPTION_FILTER_ALL -> "all"
      NotificationManager.INTERRUPTION_FILTER_PRIORITY -> "priority"
      NotificationManager.INTERRUPTION_FILTER_ALARMS -> "alarms"
      NotificationManager.INTERRUPTION_FILTER_NONE -> "none"
      else -> "unknown"
    }
    val policy = nm?.notificationPolicy
    val cats = policy?.priorityCategories ?: 0
    val allowAlarms = if (policy == null) {
      false
    } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      (cats and NotificationManager.Policy.PRIORITY_CATEGORY_ALARMS) != 0
    } else {
      true
    }
    return JSONObject()
      .put("delivery", settings.optString("notificationDelivery", ""))
      .put("locale", settings.optString("locale", ""))
      .put("phoneLang", Locale.getDefault().toLanguageTag())
      .put("dueInMinutes", due.first)
      .put("dueHour", due.second)
      .put("kind", kind)
      .put("category", category)
      .put("enabled", enabled)
      .put("channelBlocked", blocked)
      .put("bypass", bypass)
      .put("filter", filter)
      .put("policyKnown", policy != null && filter == "priority")
      .put("allowAlarms", allowAlarms)
      .put("allowReminders", (cats and NotificationManager.Policy.PRIORITY_CATEGORY_REMINDERS) != 0)
      .put("allowEvents", (cats and NotificationManager.Policy.PRIORITY_CATEGORY_EVENTS) != 0)
      .toString()
  }

  /**
   * Tauri 2 `app_data_dir` is `Context.dataDir`. Older builds and the home
   * widgets also look under `filesDir/ai.grokvoice.app`.
   */
  private fun roots(context: Context): List<File> {
    val data = context.dataDir
    val files = context.filesDir
    return listOf(
      data,
      File(files, "ai.grokvoice.app"),
      files,
      File(data, "ai.grokvoice.app"),
    ).distinctBy { it.absolutePath }
  }

  private fun appRoot(context: Context): File {
    return roots(context).firstOrNull { File(it, "settings.json").isFile } ?: roots(context).first()
  }

  /** Minutes until the real event, and its local hour. Lead days stay on the saved item. */
  private fun dueOf(context: Context, id: Int): Pair<Int, Int> {
    if (id <= 0) return 0 to -1
    val file = File(appRoot(context), "agenda.json")
    if (!file.isFile) return 0 to -1
    return try {
      val items = JSONObject(file.readText()).optJSONArray("items") ?: return 0 to -1
      var found: JSONObject? = null
      for (i in 0 until items.length()) {
        val row = items.optJSONObject(i) ?: continue
        if (row.optInt("id") == id) {
          found = row
          break
        }
      }
      val item = found ?: return 0 to -1
      val lead = item.optInt("leadDays", 0).coerceIn(0, 30)
      val at = item.optLong("atMs", 0L)
      val event = if (lead > 0 && at > 0L) at + lead * 86_400_000L else at
      val now = System.currentTimeMillis()
      val minutes = if (event > now + 60_000L) ((event - now) / 60_000L).toInt().coerceAtLeast(0) else 0
      val hour = if (event > 0L) {
        Calendar.getInstance().apply { timeInMillis = event }.get(Calendar.HOUR_OF_DAY)
      } else {
        -1
      }
      minutes to hour
    } catch (_: Exception) {
      0 to -1
    }
  }

  private fun readSettings(context: Context): JSONObject {
    val file = File(appRoot(context), "settings.json")
    if (!file.isFile) return JSONObject()
    return try {
      JSONObject(file.readText())
    } catch (t: Exception) {
      Log.w(TAG, "spoken notice could not read ${file.path}", t)
      JSONObject()
    }
  }

  private fun waitForTalk() {
    val deadline = SystemClock.elapsedRealtime() + 30_000L
    while (SystemClock.elapsedRealtime() < deadline) {
      if (!RustBridge.talkOutputBusy()) return
      try {
        Thread.sleep(200)
      } catch (_: InterruptedException) {
        return
      }
    }
  }

  private fun inCall(am: AudioManager): Boolean {
    return when (am.mode) {
      AudioManager.MODE_IN_CALL,
      AudioManager.MODE_IN_COMMUNICATION,
      AudioManager.MODE_CALL_SCREENING,
      AudioManager.MODE_RINGTONE -> true
      else -> false
    }
  }

  /** Alarm-stream attributes. Android 17 keeps this audible from an exact alarm. */
  private fun alarmAttributes(): AudioAttributes {
    return AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_ALARM)
      .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
      .build()
  }

  private fun requestFocus(am: AudioManager): AudioFocusRequest? {
    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
      .setAudioAttributes(alarmAttributes())
      .setOnAudioFocusChangeListener { }
      .build()
    return if (am.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) request else null
  }

  private fun abandonFocus(am: AudioManager, request: AudioFocusRequest) {
    try {
      am.abandonAudioFocusRequest(request)
    } catch (_: Exception) {
    }
  }

  private fun playPiper(context: Context, line: String): Boolean {
    val settings = readSettings(context)
    val pack = RustBridge.noticePiperPack(settings.optString("locale", ""), Locale.getDefault().language ?: "").orEmpty()
    val files = piperFiles(context, pack)
    if (files == null) {
      Log.i(TAG, "spoken notice piper pack not on the phone; using Android TTS")
      return false
    }
    val pcm = RustBridge.synthesizeNotice(files.model, files.tokens, files.data, line)
    if (pcm == null || pcm.isEmpty()) {
      Log.w(TAG, "spoken notice piper synthesis failed; using Android TTS")
      return false
    }
    val rate = RustBridge.noticeSampleRate().takeIf { it > 0 } ?: 22050
    return playPcm(pcm, rate)
  }

  private data class PiperFiles(val model: String, val tokens: String, val data: String)

  private fun piperFiles(context: Context, pack: String): PiperFiles? {
    if (pack.isEmpty()) return null
    val root = File(appRoot(context), "local-models")
    val dir = when (pack) {
      "tts" -> File(root, "tts/vits-piper-en_US-lessac-low")
      "tts-de" -> File(root, "tts-de/vits-piper-de_DE-thorsten-low")
      else -> return null
    }
    val model = when (pack) {
      "tts" -> File(dir, "en_US-lessac-low.onnx")
      "tts-de" -> File(dir, "de_DE-thorsten-low.onnx")
      else -> return null
    }
    val tokens = File(dir, "tokens.txt")
    val data = File(dir, "espeak-ng-data")
    if (!model.isFile || !tokens.isFile || !data.isDirectory) return null
    return PiperFiles(model.absolutePath, tokens.absolutePath, data.absolutePath)
  }

  private fun playPcm(pcm: ShortArray, rate: Int): Boolean {
    val min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
    if (min <= 0) {
      Log.w(TAG, "spoken notice audio track buffer failed code=$min")
      return false
    }
    val track = try {
      AudioTrack.Builder()
        .setAudioAttributes(alarmAttributes())
        .setAudioFormat(
          AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(rate)
            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .build()
        )
        .setTransferMode(AudioTrack.MODE_STREAM)
        .setBufferSizeInBytes(maxOf(min, pcm.size * 2))
        .build()
    } catch (t: Throwable) {
      Log.w(TAG, "piper playback failed", t)
      return false
    }
    return try {
      if (track.state != AudioTrack.STATE_INITIALIZED) {
        Log.w(TAG, "spoken notice audio track was not initialized")
        return false
      }
      track.play()
      if (track.playState != AudioTrack.PLAYSTATE_PLAYING) {
        Log.w(TAG, "spoken notice playback did not start state=${track.playState}")
        return false
      }
      var offset = 0
      while (offset < pcm.size) {
        val wrote = track.write(pcm, offset, pcm.size - offset)
        if (wrote < 0) {
          Log.w(TAG, "spoken notice audio write failed code=$wrote")
          return false
        }
        if (wrote == 0) break
        offset += wrote
      }
      val ms = pcm.size * 1000L / rate.coerceAtLeast(1) + 300L
      Thread.sleep(ms.coerceAtMost(35_000L))
      if (offset <= 0) {
        Log.w(TAG, "spoken notice playback wrote no audio")
      }
      offset > 0
    } catch (t: Throwable) {
      Log.w(TAG, "piper playback failed", t)
      false
    } finally {
      try {
        track.stop()
      } catch (_: Throwable) {
      }
      track.release()
    }
  }

  private fun playAndroid(context: Context, line: String): Boolean {
    val locale = speechLocale(readSettings(context).optString("locale", ""))
    val latch = CountDownLatch(1)
    val spoke = booleanArrayOf(false)
    Handler(Looper.getMainLooper()).post {
      val holder = arrayOfNulls<TextToSpeech>(1)
      val listener = TextToSpeech.OnInitListener { status ->
        val ready = holder[0]
        if (ready == null) {
          Handler(Looper.getMainLooper()).post {
            val tts = holder[0]
            if (tts == null) {
              Log.w(TAG, "spoken notice TTS engine was not created")
              latch.countDown()
            } else {
              onTtsReady(tts, status, locale, line, spoke, latch)
            }
          }
        } else {
          onTtsReady(ready, status, locale, line, spoke, latch)
        }
      }
      holder[0] = TextToSpeech(context.applicationContext, listener)
    }
    try {
      if (!latch.await(30, TimeUnit.SECONDS)) {
        Log.w(TAG, "spoken notice TTS timed out before it finished")
      }
    } catch (_: InterruptedException) {
      Log.w(TAG, "spoken notice TTS wait was interrupted")
    }
    if (!spoke[0]) {
      Log.w(TAG, "spoken notice Android TTS did not speak")
    }
    return spoke[0]
  }

  private fun onTtsReady(
    tts: TextToSpeech,
    status: Int,
    locale: Locale,
    line: String,
    spoke: BooleanArray,
    latch: CountDownLatch,
  ) {
    if (status != TextToSpeech.SUCCESS) {
      Log.w(TAG, "spoken notice TTS init failed status=$status")
      shutdownTts(tts)
      latch.countDown()
      return
    }
    val attrs = tts.setAudioAttributes(alarmAttributes())
    if (attrs == TextToSpeech.ERROR) {
      Log.w(TAG, "spoken notice could not put TTS on the alarm stream")
    }
    val lang = tts.setLanguage(locale)
    if (lang == TextToSpeech.LANG_MISSING_DATA || lang == TextToSpeech.LANG_NOT_SUPPORTED) {
      Log.w(TAG, "spoken notice TTS missing $locale; falling back to the phone language")
      tts.language = Locale.getDefault()
    }
    tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
      override fun onStart(utteranceId: String?) {}
      override fun onDone(utteranceId: String?) {
        spoke[0] = true
        shutdownTts(tts)
        latch.countDown()
      }
      override fun onError(utteranceId: String?) {
        Log.w(TAG, "spoken notice TTS error id=$utteranceId")
        shutdownTts(tts)
        latch.countDown()
      }
      override fun onError(utteranceId: String?, errorCode: Int) {
        Log.w(TAG, "spoken notice TTS error code=$errorCode id=$utteranceId")
        shutdownTts(tts)
        latch.countDown()
      }
    })
    val queued = tts.speak(line, TextToSpeech.QUEUE_FLUSH, Bundle(), "gv-notice")
    if (queued == TextToSpeech.ERROR) {
      Log.w(TAG, "spoken notice TTS rejected the sentence")
      shutdownTts(tts)
      latch.countDown()
    }
  }

  private fun shutdownTts(tts: TextToSpeech) {
    try {
      tts.shutdown()
    } catch (_: Exception) {
    }
  }

  private fun speechLocale(tag: String): Locale {
    val cleaned = tag.trim().replace('_', '-')
    if (cleaned.isEmpty()) return Locale.getDefault()
    if (cleaned.equals("ca-ES-valencia", ignoreCase = true)) return Locale.forLanguageTag("ca-ES")
    val locale = Locale.forLanguageTag(cleaned)
    return if (locale.language.isNullOrEmpty()) Locale.getDefault() else locale
  }

  private fun wakeLock(context: Context): PowerManager.WakeLock? {
    val pm = context.getSystemService(PowerManager::class.java) ?: return null
    return try {
      pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "GrokVoice:spoken-notice").apply {
        setReferenceCounted(false)
        acquire(90_000L)
      }
    } catch (_: Exception) {
      null
    }
  }
}
