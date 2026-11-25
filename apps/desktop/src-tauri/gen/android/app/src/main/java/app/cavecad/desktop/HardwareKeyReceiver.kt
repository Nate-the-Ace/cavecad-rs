package app.cavecad.desktop

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Takes the broadcasts rugged phones send for their action keys (see [HardwareKeys.KNOWN_ACTIONS]) and gives each to
 * [sink], which MainActivity points at the page while its screen shows. Declared in the manifest, not only registered
 * while the screen shows: a Sonim phone sends PTT only to an app that declares a receiver (its Torch app, the default
 * PTT app, does). Broadcasts the person added are heard by a receiver MainActivity registers while the screen shows.
 */
class HardwareKeyReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val press = HardwareKeys.fromAction(intent.action) ?: return
    sink?.invoke(press.key, press.phase)
  }

  companion object {
    /** Where keys go: the open screen's page. Null when the app is not showing, so keys then do nothing. */
    @Volatile var sink: ((String, String) -> Unit)? = null
  }
}
