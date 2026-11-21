package ai.grokvoice.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.telecom.TelecomManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import org.json.JSONObject

/** Places, answers, and ends calls only after Talk has already confirmed it. */
object PhoneCalls {
  fun state(context: Context): String {
    val note = Inbox.ringingCall()
    val phone = callState(context)
    val ringing = note?.answer != null || note?.decline != null || phone == TelephonyManager.CALL_STATE_RINGING
    val offhook = phone == TelephonyManager.CALL_STATE_OFFHOOK
    return JSONObject()
      .put("ringing", ringing)
      .put("offhook", offhook)
      .put("name", note?.name ?: "")
      .put("number", "")
      .put("answerPermission", granted(context, Manifest.permission.ANSWER_PHONE_CALLS))
      .put("answerAction", note?.answer != null)
      .put("declineAction", note?.decline != null)
      .put("hangupAction", note?.hangup != null)
      .toString()
  }

  fun place(context: Context, number: String): String {
    if (!granted(context, Manifest.permission.CALL_PHONE)) return fail("inbox-call-denied")
    val clean = number.filter { it.isDigit() || it == '+' || it == '*' || it == '#' }
    if (clean.count { it.isDigit() } < 3) return fail("inbox-call-number")
    val intent = Intent(Intent.ACTION_CALL, Uri.fromParts("tel", clean, null)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return try {
      context.startActivity(intent)
      JSONObject().put("ok", true).toString()
    } catch (_: SecurityException) {
      fail("inbox-call-denied")
    } catch (_: Exception) {
      fail("inbox-call-denied")
    }
  }

  fun answer(context: Context): String {
    val note = Inbox.ringingCall()
    if (note?.answer != null && fire(context, note.answer)) return ok()
    if (granted(context, Manifest.permission.ANSWER_PHONE_CALLS) && accept(context)) return ok()
    return fail("inbox-answer-denied")
  }

  fun decline(context: Context): String {
    val note = Inbox.ringingCall()
    if (note?.decline != null && fire(context, note.decline)) return ok()
    if (granted(context, Manifest.permission.ANSWER_PHONE_CALLS) && endTelecom(context)) return ok()
    return fail("inbox-answer-denied")
  }

  fun end(context: Context): String {
    val note = Inbox.ringingCall()
    val action = note?.hangup ?: note?.decline
    if (action != null && fire(context, action)) return ok()
    if (granted(context, Manifest.permission.ANSWER_PHONE_CALLS) && endTelecom(context)) return ok()
    return fail("inbox-answer-denied")
  }

  @Suppress("DEPRECATION")
  private fun accept(context: Context): Boolean {
    val telecom = context.getSystemService(TelecomManager::class.java) ?: return false
    return try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) telecom.acceptRingingCall()
      true
    } catch (_: SecurityException) {
      false
    } catch (_: Exception) {
      false
    }
  }

  @Suppress("DEPRECATION")
  private fun endTelecom(context: Context): Boolean {
    val telecom = context.getSystemService(TelecomManager::class.java) ?: return false
    return try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) telecom.endCall() else false
    } catch (_: SecurityException) {
      false
    } catch (_: Exception) {
      false
    }
  }

  private fun fire(context: Context, action: android.app.Notification.Action): Boolean {
    return try {
      action.actionIntent?.send(context, 0, Intent())
      true
    } catch (_: Exception) {
      false
    }
  }

  private fun callState(context: Context): Int {
    if (!granted(context, Manifest.permission.READ_PHONE_STATE)) return TelephonyManager.CALL_STATE_IDLE
    val tm = context.getSystemService(TelephonyManager::class.java) ?: return TelephonyManager.CALL_STATE_IDLE
    return try {
      @Suppress("DEPRECATION")
      tm.callState
    } catch (_: SecurityException) {
      TelephonyManager.CALL_STATE_IDLE
    }
  }

  private fun granted(context: Context, permission: String): Boolean {
    return ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
  }

  private fun ok() = JSONObject().put("ok", true).toString()
  private fun fail(key: String) = JSONObject().put("ok", false).put("error", key).toString()
}
