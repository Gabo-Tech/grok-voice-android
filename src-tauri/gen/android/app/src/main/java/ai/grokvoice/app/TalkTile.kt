package ai.grokvoice.app

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * Quick Settings tile. Android does not offer a third-party lock-screen widget.
 * The tile can open Talk over the keyguard. Some phones still block the microphone until unlock.
 */
class TalkTile : TileService() {
  override fun onStartListening() {
    val tile = qsTile ?: return
    tile.label = getString(R.string.tile_talk)
    if (Build.VERSION.SDK_INT >= 29) {
      tile.subtitle = HomeWidgets.nextReminder(this).ifBlank { getString(R.string.tile_talk_idle) }
    }
    tile.state = Tile.STATE_INACTIVE
    tile.updateTile()
  }

  override fun onClick() {
    WidgetGate.allow(this, "lock")
    val launch = Intent(this, MainActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      .putExtra(MainActivity.EXTRA_WIDGET, "lock")
    if (Build.VERSION.SDK_INT >= 34) {
      val pending = PendingIntent.getActivity(
        this,
        7,
        launch,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
      )
      startActivityAndCollapse(pending)
    } else {
      @Suppress("DEPRECATION")
      startActivityAndCollapse(launch)
    }
  }
}
