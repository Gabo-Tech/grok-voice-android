package ai.grokvoice.app

import android.content.Context
import android.content.res.Configuration
import android.graphics.Color
import android.widget.RemoteViews

/**
 * Magazine palettes for home-screen widgets. Hex values match src/lib/appearance.ts.
 * Widgets never read API keys. They only read the theme choice and custom colors.
 */
object WidgetTheme {
  data class Palette(
    val canvas: Int,
    val panel: Int,
    val ink: Int,
    val muted: Int,
    val accent: Int,
    val border: Int,
    val income: Int,
    val expense: Int,
    val onAccent: Int,
  )

  fun read(context: Context): Palette {
    val settings = AppData.settingsRoot(context)
    val appearance = settings?.optString("appearance").orEmpty().ifBlank { "graphite" }.lowercase()
    val custom = settings?.optString("customTheme").orEmpty()
    val night = (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    val id = if (appearance == "system") if (night) "graphite" else "stone" else appearance
    val preset = PRESETS[id] ?: GRAPHITE
    return if (id == "custom") fromCustom(custom, preset) else preset
  }

  fun paint(views: RemoteViews, palette: Palette) {
    views.setInt(R.id.widget_root, "setBackgroundColor", palette.border)
    views.setInt(R.id.widget_body, "setBackgroundColor", palette.canvas)
  }

  fun canvas(context: Context): Int = read(context).canvas

  fun lightBars(color: Int): Boolean = luminance(color) > 0.45

  private fun fromCustom(raw: String, fallback: Palette): Palette {
    val parts = raw.split(",").map { it.trim() }
    if (parts.size != 7 || parts.any { !it.matches(HEX) }) return fallback
    val canvas = Color.parseColor(parts[0])
    val panel = Color.parseColor(parts[1])
    val border = Color.parseColor(parts[2])
    val ink = Color.parseColor(parts[3])
    val accent = Color.parseColor(parts[4])
    val income = Color.parseColor(parts[5])
    val expense = Color.parseColor(parts[6])
    val onAccent = if (luminance(accent) > 0.55) Color.parseColor("#1A1B1E") else Color.parseColor("#F7F4EE")
    return Palette(canvas, panel, ink, mix(ink, canvas, 0.45f), accent, border, income, expense, onAccent)
  }

  private fun luminance(color: Int): Double {
    fun channel(value: Int): Double {
      val s = value / 255.0
      return if (s <= 0.03928) s / 12.92 else Math.pow((s + 0.055) / 1.055, 2.4)
    }
    return 0.2126 * channel(Color.red(color)) + 0.7152 * channel(Color.green(color)) + 0.0722 * channel(Color.blue(color))
  }

  private fun mix(from: Int, to: Int, toward: Float): Int {
    fun channel(a: Int, b: Int) = (a + (b - a) * toward).toInt().coerceIn(0, 255)
    return Color.rgb(channel(Color.red(from), Color.red(to)), channel(Color.green(from), Color.green(to)), channel(Color.blue(from), Color.blue(to)))
  }

  private val HEX = Regex("^#[0-9A-Fa-f]{6}$")

  private fun p(canvas: String, panel: String, ink: String, muted: String, accent: String, border: String, income: String, expense: String, onAccent: String) =
    Palette(Color.parseColor(canvas), Color.parseColor(panel), Color.parseColor(ink), Color.parseColor(muted), Color.parseColor(accent), Color.parseColor(border), Color.parseColor(income), Color.parseColor(expense), Color.parseColor(onAccent))

  private val GRAPHITE = p("#0E0F11", "#17181B", "#EEECE8", "#8B8A86", "#C4B5A0", "#9A9690", "#7D9A86", "#C48984", "#1A1B1E")

  private val PRESETS = mapOf(
    "graphite" to GRAPHITE,
    "ink" to p("#070708", "#101114", "#F7F4EE", "#B7B3AA", "#C4B5A0", "#E8E4DC", "#8EAE98", "#D09A96", "#070708"),
    "stone" to p("#F4F2EE", "#FBFAF7", "#1A1B1E", "#5E5A54", "#6F5A42", "#2A2826", "#2F6A48", "#8E3F3B", "#F4F2EE"),
    "midnight" to p("#0B1220", "#121A2B", "#E7E4DC", "#8E97A8", "#C6B59A", "#8E97A8", "#7F9E8C", "#C48B8A", "#0B1220"),
    "carbon" to p("#000000", "#0A0A0A", "#F4F1EA", "#9A958C", "#C4B5A0", "#E8E4DC", "#86A892", "#C98984", "#000000"),
    "ivory" to p("#FBF7F0", "#FFFFFF", "#1C1916", "#6B645C", "#8A6E49", "#2A241E", "#2E6B46", "#8C3E3A", "#FBF7F0"),
    "sand" to p("#E7DCC8", "#F3E9D6", "#3A2E22", "#7A6A58", "#8C6239", "#4A3B2C", "#3E6B4F", "#8E4A42", "#F3E9D6"),
    "forest" to p("#101612", "#18201B", "#E8E4D8", "#8E9A8E", "#C6B48A", "#9AA898", "#8FB59A", "#C4928C", "#101612"),
    "plum" to p("#161218", "#211C24", "#F0EBE4", "#A398A4", "#C4B09A", "#B4A4B0", "#86A494", "#C48A96", "#161218"),
  )
}
