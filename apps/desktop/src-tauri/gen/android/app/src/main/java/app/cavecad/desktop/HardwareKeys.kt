package app.cavecad.desktop

import android.os.Build
import android.view.KeyEvent
import android.webkit.JavascriptInterface
import org.json.JSONObject

/**
 * What rugged phones send for their action keys, turned into the page's vendor-neutral key names (ptt, sos, prog1..3,
 * camera, focus; see apps/web/src/lib/hardware-keys.ts). Two ways a phone reports a key:
 *  - a broadcast (Sonim, and the Zello convention many rugged phones follow): [KNOWN_ACTIONS], plus any the person added;
 *  - a key event with a key code (camera and programmable keys of most others): [KNOWN_KEYCODES], plus any added.
 * To support another phone: add its broadcast actions to [KNOWN_ACTIONS] here and in the manifest's receiver (docs/ANDROID-TAURI.md).
 */
object HardwareKeys {
  /** A key going down or coming back up (`phase`: "down" or "up"). */
  data class Press(val key: String, val phase: String)

  /** Broadcasts known phones send. Sonim sends them only to the app chosen under Settings > Programmable Keys. */
  val KNOWN_ACTIONS: Map<String, Press> = mapOf(
    "com.sonim.intent.action.PTT_KEY_DOWN" to Press("ptt", "down"),
    "com.sonim.intent.action.PTT_KEY_UP" to Press("ptt", "up"),
    "com.sonim.intent.action.SOS_KEY_DOWN" to Press("sos", "down"),
    "com.sonim.intent.action.SOS_KEY_UP" to Press("sos", "up"),
    "com.sonim.intent.action.YELLOW_KEY_DOWN" to Press("prog1", "down"),
    "com.sonim.intent.action.YELLOW_KEY_UP" to Press("prog1", "up"),
    // the broadcasts Zello documents for hardware PTT keys; phones that follow it send these
    "com.zello.ptt.down" to Press("ptt", "down"),
    "com.zello.ptt.up" to Press("ptt", "up"),
  )

  /** Key codes of the camera keys and the programmable keys most phones report as F1 to F3. */
  val KNOWN_KEYCODES: Map<Int, String> = mapOf(
    KeyEvent.KEYCODE_CAMERA to "camera",
    KeyEvent.KEYCODE_FOCUS to "focus",
    KeyEvent.KEYCODE_F1 to "prog1",
    KeyEvent.KEYCODE_F2 to "prog2",
    KeyEvent.KEYCODE_F3 to "prog3",
  )

  /** Keys that learn mode never takes (a person can still add one by its number). */
  private val ESSENTIAL = setOf(
    KeyEvent.KEYCODE_BACK, KeyEvent.KEYCODE_HOME, KeyEvent.KEYCODE_APP_SWITCH, KeyEvent.KEYCODE_POWER,
    KeyEvent.KEYCODE_VOLUME_UP, KeyEvent.KEYCODE_VOLUME_DOWN, KeyEvent.KEYCODE_VOLUME_MUTE,
  )

  private val KEY_NAMES = setOf("ptt", "sos", "prog1", "prog2", "prog3", "camera", "focus")
  private val ACTION_NAME = Regex("^[A-Za-z0-9._]{3,120}$")
  private const val LIMIT = 24

  /** What the person added (apps/web sends it with [CaveKeysBridge.configure]). */
  @Volatile private var customActions: Map<String, Press> = emptyMap()
  @Volatile private var customKeycodes: Map<Int, String> = emptyMap()

  /** The survey is live: the page wants the keys, so key events for them are taken (kept from doing their usual job). */
  @Volatile var active = false

  /** Learn mode: a key the app does not know is reported to the page by its code, and taken. */
  @Volatile var learning = false

  /** True on a Sonim phone, which sends its camera key as the Yellow broadcast as well as a key event. */
  private val sonim = Build.MANUFACTURER.equals("sonim", ignoreCase = true) || Build.MANUFACTURER.equals("sonimtech", ignoreCase = true)

  /** The key a broadcast action means: a known one, or one the person added. */
  fun fromAction(action: String?): Press? = if (action == null) null else customActions[action] ?: KNOWN_ACTIONS[action]

  /** The key name for a key code, or null when it is not one this app knows. */
  fun fromKeyCode(code: Int): String? {
    customKeycodes[code]?.let { return it }
    if (sonim && (code == KeyEvent.KEYCODE_CAMERA || code == KeyEvent.KEYCODE_FOCUS)) return null // Sonim sends these as the Yellow broadcast
    return KNOWN_KEYCODES[code]
  }

  /** True for a key learn mode leaves alone. */
  fun isEssential(code: Int) = code in ESSENTIAL

  /** The broadcast actions the person added: the dynamic receiver listens for these while the screen shows. */
  fun customActionNames(): Set<String> = customActions.keys

  /** Reads what the page keeps: `{"actions":[{"action","key","phase"}],"keycodes":[{"code","key"}]}`. Anything invalid is dropped. */
  fun configure(json: String) {
    val actions = LinkedHashMap<String, Press>()
    val codes = LinkedHashMap<Int, String>()
    try {
      val o = JSONObject(json)
      val a = o.optJSONArray("actions")
      if (a != null) for (i in 0 until a.length()) {
        if (actions.size >= LIMIT) break
        val e = a.optJSONObject(i) ?: continue
        val name = e.optString("action")
        val key = e.optString("key")
        val phase = e.optString("phase")
        if (ACTION_NAME.matches(name) && key in KEY_NAMES && (phase == "down" || phase == "up") && name !in actions) actions[name] = Press(key, phase)
      }
      val c = o.optJSONArray("keycodes")
      if (c != null) for (i in 0 until c.length()) {
        if (codes.size >= LIMIT) break
        val e = c.optJSONObject(i) ?: continue
        val code = e.optInt("code", -1)
        val key = e.optString("key")
        if (code > 0 && code < 100000 && key in KEY_NAMES && code !in codes) codes[code] = key
      }
    } catch (e: Exception) {
      // unreadable: nothing custom
    }
    customActions = actions
    customKeycodes = codes
  }
}

/**
 * The page's door to the phone's keys (`window.CaveKeys`): who the phone is, what the person added, whether the survey
 * is live (so the keys are taken) and learn mode. Only the page of this app is loaded in the web view.
 */
class CaveKeysBridge(private val owner: MainActivity) {
  @JavascriptInterface
  fun info(): String = JSONObject().put("manufacturer", Build.MANUFACTURER).put("model", Build.MODEL).toString()

  @JavascriptInterface
  fun configure(json: String) {
    HardwareKeys.configure(json)
    owner.runOnUiThread { owner.refreshCustomReceiver() }
  }

  @JavascriptInterface
  fun setActive(on: Boolean) {
    HardwareKeys.active = on
  }

  @JavascriptInterface
  fun learn(on: Boolean) {
    HardwareKeys.learning = on
  }
}
