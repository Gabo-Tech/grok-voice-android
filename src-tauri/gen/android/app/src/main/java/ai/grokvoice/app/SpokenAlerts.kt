package ai.grokvoice.app

import android.app.NotificationManager
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.os.SystemClock
import java.util.Calendar
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * Reads a notification that was already posted. Normal mode returns immediately.
 * Spoken mode uses Piper when that language pack is on the phone, then Android
 * text-to-speech. A failure leaves the notification on screen.
 */
object SpokenAlerts {
  private const val TAG = "GrokVoice"
  private val worker = Executors.newSingleThreadExecutor { runnable ->
    Thread(runnable, "spoken-notice")
  }
  private val ttsThread = HandlerThread("spoken-tts").apply { start() }
  private val ttsHandler = Handler(ttsThread.looper)

  fun enqueue(context: Context, id: Int, kind: String, title: String, body: String, channelId: String, category: String) {
    val app = context.applicationContext
    worker.execute {
      val wake = wakeLock(app)
      try {
        speak(app, id, kind, title, body, channelId, category)
      } catch (t: Throwable) {
        Log.w(TAG, "spoken notice failed", t)
      } finally {
        if (wake?.isHeld == true) wake.release()
      }
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

  private fun speak(context: Context, id: Int, kind: String, title: String, body: String, channelId: String, category: String) {
    if (!RustBridge.loadCore()) return
    val line = RustBridge.planSpokenNotice(spec(context, id, kind, channelId, category), title, body).orEmpty()
    if (line.isEmpty()) return
    waitForTalk()
    if (RustBridge.talkOutputBusy()) return
    val am = context.getSystemService(AudioManager::class.java) ?: return
    if (inCall(am)) return
    val focus = requestFocus(am, category) ?: return
    RustBridge.setNoticeSpeaking(true)
    try {
      if (!playPiper(context, line, category) && !playAndroid(context, line)) {
        Log.w(TAG, "could not speak the notice")
      }
    } finally {
      RustBridge.setNoticeSpeaking(false)
      abandonFocus(am, focus)
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

  /** Tauri stores settings and packs in `dataDir`, which is the parent of `filesDir`. */
  private fun appRoot(context: Context): File {
    val data = context.dataDir
    if (File(data, "settings.json").isFile || File(data, "local-models").isDirectory) return data
    val files = context.filesDir
    if (File(files, "settings.json").isFile || File(files, "local-models").isDirectory) return files
    return data
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
    } catch (_: Exception) {
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

  private fun requestFocus(am: AudioManager, category: String): AudioFocusRequest? {
    val usage = if (category == "alarm") AudioAttributes.USAGE_ALARM else AudioAttributes.USAGE_NOTIFICATION
    val attrs = AudioAttributes.Builder()
      .setUsage(usage)
      .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
      .build()
    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
      .setAudioAttributes(attrs)
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

  private fun playPiper(context: Context, line: String, category: String): Boolean {
    val settings = readSettings(context)
    val pack = RustBridge.noticePiperPack(settings.optString("locale", ""), Locale.getDefault().language ?: "").orEmpty()
    val files = piperFiles(context, pack) ?: return false
    val pcm = RustBridge.synthesizeNotice(files.model, files.tokens, files.data, line) ?: return false
    if (pcm.isEmpty()) return false
    val rate = RustBridge.noticeSampleRate().takeIf { it > 0 } ?: 22050
    val usage = if (category == "alarm") AudioAttributes.USAGE_ALARM else AudioAttributes.USAGE_NOTIFICATION
    return playPcm(pcm, rate, usage)
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

  private fun playPcm(pcm: ShortArray, rate: Int, usage: Int): Boolean {
    val min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
    if (min <= 0) return false
    val track = try {
      AudioTrack.Builder()
        .setAudioAttributes(
          AudioAttributes.Builder()
            .setUsage(usage)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
        )
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
      if (track.state != AudioTrack.STATE_INITIALIZED) return false
      track.play()
      var offset = 0
      while (offset < pcm.size) {
        val wrote = track.write(pcm, offset, pcm.size - offset)
        if (wrote <= 0) break
        offset += wrote
      }
      val ms = pcm.size * 1000L / rate.coerceAtLeast(1) + 300L
      Thread.sleep(ms.coerceAtMost(35_000L))
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
    ttsHandler.post {
      var engine: TextToSpeech? = null
      engine = TextToSpeech(context.applicationContext) { status ->
        val tts = engine
        if (tts == null || status != TextToSpeech.SUCCESS) {
          latch.countDown()
          return@TextToSpeech
        }
        val lang = tts.setLanguage(locale)
        if (lang == TextToSpeech.LANG_MISSING_DATA || lang == TextToSpeech.LANG_NOT_SUPPORTED) {
          tts.language = Locale.getDefault()
        }
        tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
          override fun onStart(utteranceId: String?) {}
          override fun onDone(utteranceId: String?) {
            spoke[0] = true
            tts.shutdown()
            latch.countDown()
          }
          override fun onError(utteranceId: String?) {
            tts.shutdown()
            latch.countDown()
          }
        })
        val queued = tts.speak(line, TextToSpeech.QUEUE_FLUSH, Bundle(), "gv-notice")
        if (queued == TextToSpeech.ERROR) {
          tts.shutdown()
          latch.countDown()
        }
      }
    }
    try {
      latch.await(30, TimeUnit.SECONDS)
    } catch (_: InterruptedException) {
    }
    return spoke[0]
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
