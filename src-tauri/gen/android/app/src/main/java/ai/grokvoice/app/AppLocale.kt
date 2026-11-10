package ai.grokvoice.app

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import org.json.JSONObject
import java.io.File
import java.util.Locale

/** Applies the in-app language to Android resources. An empty setting follows the phone. */
object AppLocale {
  fun wrap(base: Context): Context {
    val tag = read(base).trim().replace('_', '-')
    if (tag.isEmpty()) return base
    val locale = Locale.forLanguageTag(tag)
    if (locale.language.isNullOrEmpty()) return base
    val config = Configuration(base.resources.configuration)
    config.setLocale(locale)
    config.setLocales(LocaleList(locale))
    return base.createConfigurationContext(config)
  }

  private fun read(context: Context): String {
    val dirs = listOf(File(context.filesDir, "ai.grokvoice.app"), context.filesDir)
    val file = dirs.map { File(it, "settings.json") }.firstOrNull { it.isFile } ?: return ""
    return try {
      JSONObject(file.readText()).optString("locale")
    } catch (_: Exception) {
      ""
    }
  }
}
