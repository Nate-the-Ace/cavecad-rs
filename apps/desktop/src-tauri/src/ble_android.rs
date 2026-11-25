//! Bluetooth on Android: btleplug's Android half is Java (vendored in gen/android/app/src/vendor/java) reached
//! through JNI. Three things the other platforms do not need:
//!  - setting btleplug up from a call that starts in Java (`MainActivity.initBluetooth`), because only there does JNI
//!    find the app's own classes;
//!  - every thread that runs a Bluetooth future attached to the Java VM (`on_jvm`): btleplug calls Java while it is
//!    polled, on whichever runtime thread that happens to be;
//!  - asking for the Bluetooth (and on Android 11 and older Location) permission before a scan (`ready`).

use jni::objects::{JClass, JObject, JString};
use jni::sys::jstring;
use jni::{JNIEnv, JavaVM};
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

static VM: OnceLock<JavaVM> = OnceLock::new();
/// Set once btleplug is set up: until then its adapter must not be asked for (it would panic).
static SET_UP: AtomicBool = AtomicBool::new(false);
const ACTIVITY: &str = "app/cavecad/desktop/MainActivity";

/// `MainActivity.initBluetooth()`: sets up jni-utils and btleplug. Gives back "" or what went wrong.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_cavecad_desktop_MainActivity_initBluetooth(env: JNIEnv, _this: JObject) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
        if VM.get().is_none() {
            let vm = env.get_java_vm().map_err(|e| e.to_string())?;
            let _ = VM.set(vm);
        }
        jni_utils::init(&env).map_err(|e| format!("jni-utils: {e}"))?;
        jni_utils::classcache::find_add_class(&env, ACTIVITY).map_err(|e| e.to_string())?;
        btleplug::platform::init(&env).map_err(|e| format!("btleplug: {e}"))?;
        SET_UP.store(true, Ordering::SeqCst);
        Ok(())
    }));
    let message = match result {
        Ok(Ok(())) => String::new(),
        Ok(Err(e)) => e,
        Err(_) => "the Bluetooth setup panicked (a Java class missing from the app?)".into(),
    };
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    env.new_string(message)
        .map(|s| s.into_inner())
        .unwrap_or(std::ptr::null_mut())
}

/// Attaches the current thread to the Java VM for good (nothing to do when it already is).
fn attach() {
    if let Some(vm) = VM.get() {
        if vm.get_env().is_err() {
            let _ = vm.attach_current_thread_permanently();
        }
    }
}

/// A future whose every poll runs on a thread attached to the Java VM.
pub struct OnJvm<F>(Pin<Box<F>>);

impl<F: Future> Future for OnJvm<F> {
    type Output = F::Output;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        attach();
        self.0.as_mut().poll(cx)
    }
}

pub fn on_jvm<F: Future>(f: F) -> OnJvm<F> {
    OnJvm(Box::pin(f))
}

/// Before a scan: Bluetooth set up, the permissions granted (asked for if not: this waits for the person's answer,
/// so it runs off the async threads), Bluetooth on and, on Android 11 and older, Location on.
pub async fn ready() -> Result<(), String> {
    if !SET_UP.load(Ordering::SeqCst) {
        return Err("Bluetooth could not be set up in CaveCAD on this device.".into());
    }
    tauri::async_runtime::spawn_blocking(|| {
        attach();
        let vm = VM.get().ok_or("no Java VM")?;
        let env = vm.get_env().map_err(|e| e.to_string())?;
        let class = jni_utils::classcache::get_class(ACTIVITY).ok_or("MainActivity is not known")?;
        let answer = env
            .call_static_method(JClass::from(class.as_obj()), "bluetoothProblem", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|e| {
                let _ = env.exception_clear();
                format!("Could not check Bluetooth: {e}")
            })?;
        let text: String = env
            .get_string(JString::from(answer))
            .map_err(|e| e.to_string())?
            .into();
        if text.is_empty() { Ok(()) } else { Err(text) }
    })
    .await
    .map_err(|e| e.to_string())?
}
