package app.cavecad.desktop

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.content.Context
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.graphics.Color
import android.location.LocationManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MainActivity : TauriActivity() {
  private var page: WebView? = null

  // Sonim's PTT, SOS and Yellow keys (SonimKeyReceiver) go to the page as `cave-hwkey` events while this screen is
  // showing (apps/web/src/lib/hardware-keys.ts maps them to survey actions). The phone sends each key's broadcast only to
  // the app chosen under Settings > Programmable Keys.
  private fun sendKey(key: String, phase: String) {
    // key and phase are fixed words from SonimKeyReceiver, never broadcast data, so they are safe to put in script
    page?.post {
      page?.evaluateJavascript("window.dispatchEvent(new CustomEvent('cave-hwkey',{detail:{key:'$key',phase:'$phase'}}))", null)
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    page = webView
  }

  override fun onResume() {
    super.onResume()
    SonimKeyReceiver.sink = { key, phase -> sendKey(key, phase) }
  }

  override fun onPause() {
    SonimKeyReceiver.sink = null
    super.onPause()
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // The page draws under the status and navigation bars and keeps clear of them with env(safe-area-inset-*).
    // Light bar icons: the app's default look is dark (a light theme gets light icons too; see docs/ANDROID-TAURI.md).
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    // Phones portrait only, tablets (smallest width 600 dp and up) turn freely, as on the iPhone and iPad.
    requestedOrientation =
      if (resources.getBoolean(R.bool.is_tablet)) ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
      else ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
    current = this
    // Bluetooth (btleplug) is set up from here: its Java classes are only found from a call that starts in Java.
    val problem = initBluetooth()
    if (problem.isEmpty()) Log.i(TAG, "Bluetooth bridge ready") else Log.e(TAG, "Bluetooth bridge: $problem")
  }

  override fun onDestroy() {
    if (current === this) current = null
    super.onDestroy()
  }

  // The page has no history (its tabs and sheets are not pages), so Back with nothing to go back to would close the
  // app and drop what is open. It goes to the background instead, like Home: coming back finds the app as it was.
  @Deprecated("Called by Tauri's back handlers when the web view cannot go back")
  override fun onBackPressed() {
    moveTaskToBack(true)
  }

  /** Sets up btleplug (src-tauri/src/ble_android.rs). Gives back "" or what went wrong. */
  private external fun initBluetooth(): String

  companion object {
    private const val TAG = "CaveCAD"
    @Volatile private var current: MainActivity? = null

    /**
     * Called from Rust (ble_scan, on a background thread) before looking for instruments: asks for the Bluetooth
     * permissions if they are missing (and waits for the answer), then checks that Bluetooth, and on Android 11 and
     * older Location, are on. Gives back "" when a scan can start, or what the person has to do, in plain words.
     */
    @JvmStatic
    fun bluetoothProblem(): String {
      val activity = current ?: return "CaveCAD is not open."
      val needed =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S)
          arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
        else arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
      fun granted() = needed.all {
        ContextCompat.checkSelfPermission(activity, it) == PackageManager.PERMISSION_GRANTED
      }
      if (!granted()) {
        val answered = CountDownLatch(1)
        activity.runOnUiThread { activity.requestPermissions(needed) { answered.countDown() } }
        answered.await(120, TimeUnit.SECONDS)
        if (!granted()) {
          return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S)
            "CaveCAD needs the Nearby devices permission to find your instrument. Allow it (Android Settings > Apps > CaveCAD > Permissions), then tap Connect again."
          else
            "CaveCAD needs the Location permission to find Bluetooth instruments (Android 11 and older ask for it for any Bluetooth search; CaveCAD does not use your location). Allow it (Android Settings > Apps > CaveCAD > Permissions), then tap Connect again."
        }
      }
      @Suppress("DEPRECATION")
      val adapter = BluetoothAdapter.getDefaultAdapter() ?: return "This device has no Bluetooth."
      if (!adapter.isEnabled) return "Bluetooth is off. Turn it on, then tap Connect again."
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
        val location = activity.getSystemService(Context.LOCATION_SERVICE) as LocationManager
        if (!location.isLocationEnabled)
          return "Location is off. Android 11 and older only find Bluetooth instruments with Location on: turn it on (quick settings), then tap Connect again."
      }
      return ""
    }
  }
}
