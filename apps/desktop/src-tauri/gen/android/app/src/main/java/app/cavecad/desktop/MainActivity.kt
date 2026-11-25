package app.cavecad.desktop

import android.content.pm.ActivityInfo
import android.graphics.Color
import android.os.Bundle
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
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
  }

  // The page has no history (its tabs and sheets are not pages), so Back with nothing to go back to would close the
  // app and drop what is open. It goes to the background instead, like Home: coming back finds the app as it was.
  @Deprecated("Called by Tauri's back handlers when the web view cannot go back")
  override fun onBackPressed() {
    moveTaskToBack(true)
  }
}
