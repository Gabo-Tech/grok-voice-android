package ai.grokvoice.app

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Process

/**
 * Microphone owned by the foreground service. AAudio (cpal) capture is tied to the
 * activity and goes silent when the screen locks or the task is removed. AudioRecord
 * started after startForeground(microphone) keeps delivering samples.
 */
class MicCapture(
  private val onEvent: (String) -> Unit,
  private val onOpenChanged: (Boolean) -> Unit,
  private val onPresent: () -> Unit,
  private val onStopped: (reopen: Boolean) -> Unit,
) {
  @Volatile private var running = false
  @Volatile private var recorder: AudioRecord? = null
  private var worker: Thread? = null

  fun start() {
    if (worker?.isAlive == true) return
    running = true
    worker = Thread({
      Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
      try {
        System.loadLibrary("grok_voice_lib")
      } catch (_: UnsatisfiedLinkError) {
      }
      var reopen = false
      try {
        reopen = captureLoop()
      } catch (e: Exception) {
        onEvent("microphone capture error (${e.javaClass.simpleName})")
        reopen = running
      } finally {
        release()
        onOpenChanged(false)
        onStopped(reopen && running)
      }
    }, "grok-mic")
    worker?.start()
  }

  fun stop() {
    running = false
    try {
      recorder?.stop()
    } catch (_: Exception) {
    }
    worker?.join(1500)
    worker = null
    onOpenChanged(false)
  }

  private fun captureLoop(): Boolean {
    val rec = openRecorder()
    recorder = rec
    rec.startRecording()
    if (rec.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
      onEvent("microphone did not enter the recording state")
      return running
    }
    onOpenChanged(true)
    onEvent("microphone capture started in the foreground service")
    val shorts = ShortArray(READ_SAMPLES)
    var zeroSamples = 0
    while (running) {
      val n = rec.read(shorts, 0, shorts.size)
      if (!running) break
      if (n < 0) {
        onEvent("microphone read error $n, reopening")
        return true
      }
      if (n == 0) continue
      if (isSilence(shorts, n)) {
        zeroSamples += n
        if (zeroSamples >= RATE * 8) {
          onEvent("microphone returned silence, reopening")
          return true
        }
      } else {
        zeroSamples = 0
      }
      if (!feed(shorts, n)) continue
      if (pollReopen()) {
        onEvent("wake engine asked the foreground microphone to reopen")
        return true
      }
      if (pollPresent()) onPresent()
    }
    return false
  }

  private fun openRecorder(): AudioRecord {
    val sources = intArrayOf(MediaRecorder.AudioSource.VOICE_RECOGNITION, MediaRecorder.AudioSource.MIC)
    var last: Exception? = null
    for (source in sources) {
      try {
        val rec = build(source)
        if (rec.state == AudioRecord.STATE_INITIALIZED) {
          if (source == MediaRecorder.AudioSource.MIC) {
            onEvent("microphone opened with the mic source")
          }
          return rec
        }
        rec.release()
      } catch (e: Exception) {
        last = e
      }
    }
    throw last ?: IllegalStateException("AudioRecord")
  }

  private fun build(source: Int): AudioRecord {
    val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
    if (min <= 0) throw IllegalStateException("no microphone buffer")
    val bytes = maxOf(min * 2, RATE / 5 * 2)
    return AudioRecord.Builder()
      .setAudioSource(source)
      .setAudioFormat(
        AudioFormat.Builder()
          .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
          .setSampleRate(RATE)
          .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
          .build()
      )
      .setBufferSizeInBytes(bytes)
      .build()
  }

  private fun release() {
    val rec = recorder
    recorder = null
    if (rec == null) return
    try {
      rec.stop()
    } catch (_: Exception) {
    }
    try {
      rec.release()
    } catch (_: Exception) {
    }
  }

  private fun feed(samples: ShortArray, n: Int): Boolean {
    return try {
      RustBridge.feedMic(samples, n)
      true
    } catch (_: UnsatisfiedLinkError) {
      false
    } catch (_: Exception) {
      false
    }
  }

  private fun pollReopen(): Boolean {
    return try {
      RustBridge.pollReopenMic()
    } catch (_: UnsatisfiedLinkError) {
      false
    } catch (_: Exception) {
      false
    }
  }

  private fun pollPresent(): Boolean {
    return try {
      RustBridge.pollPresentTalk()
    } catch (_: UnsatisfiedLinkError) {
      false
    } catch (_: Exception) {
      false
    }
  }

  private fun isSilence(samples: ShortArray, n: Int): Boolean {
    for (i in 0 until n) {
      if (samples[i] != 0.toShort()) return false
    }
    return true
  }

  companion object {
    const val RATE = 16_000
    private const val READ_SAMPLES = 640
  }
}
