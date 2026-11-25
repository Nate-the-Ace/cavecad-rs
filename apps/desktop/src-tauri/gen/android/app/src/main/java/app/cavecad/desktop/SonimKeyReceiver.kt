package app.cavecad.desktop

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Sonim's PTT, SOS and Yellow keys arrive as broadcasts, sent only to the app chosen under Settings > Programmable
 * Keys. Declared in the manifest (not only registered while the screen shows) because the phone's PTT handling looks
 * for a receiver the app declares; the Torch app, which the PTT key opens by default, declares one the same way.
 * Each key goes to [sink] (set by MainActivity while it is showing), which passes it to the page.
 */
class SonimKeyReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val (key, phase) = when (intent.action) {
      "com.sonim.intent.action.PTT_KEY_DOWN" -> "ptt" to "down"
      "com.sonim.intent.action.PTT_KEY_UP" -> "ptt" to "up"
      "com.sonim.intent.action.SOS_KEY_DOWN" -> "sos" to "down"
      "com.sonim.intent.action.SOS_KEY_UP" -> "sos" to "up"
      "com.sonim.intent.action.YELLOW_KEY_DOWN" -> "yellow" to "down"
      "com.sonim.intent.action.YELLOW_KEY_UP" -> "yellow" to "up"
      else -> return
    }
    sink?.invoke(key, phase)
  }

  companion object {
    /** Where keys go: the open screen's page. Null when the app is not showing, so keys then do nothing. */
    @Volatile var sink: ((String, String) -> Unit)? = null
  }
}
