//! One gimbal at a time (an Osmo Mobile 7 through `cave-osmo`), driven from a tokio runtime on its own thread.
//! The C doors only flip flags and queue commands; the phone polls [`status`]. The `Osmo` never leaves the one
//! runtime task that drives it.

use cave_osmo::Osmo;
use cave_osmo::targets::{LIST_MAX, Target};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// What the runner is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// Nothing running (connected or not).
    #[default]
    Idle,
    /// Looking for the gimbal and connecting.
    Connecting,
    /// Running a plan.
    Moving,
    /// Paused on a target (target mode kept alive).
    Holding,
    /// Stopped: going home.
    Homing,
    /// The plan ran to its end (or was stopped and is home).
    Done,
    /// Something failed; see `error`.
    Error,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Connecting => "connecting",
            State::Moving => "moving",
            State::Holding => "holding",
            State::Homing => "homing",
            State::Done => "done",
            State::Error => "error",
        }
    }
}

/// What the phone reads back.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// A gimbal is connected.
    pub connected: bool,
    /// What the runner is doing.
    pub state: State,
    /// Targets reached so far in the running plan.
    pub step: u32,
    /// Targets in the running plan.
    pub steps: u32,
    /// The running plan has entered target mode and the gimbal is home (set after the post-enter wait, before
    /// the first move); false from the moment a plan is queued.
    pub entered: bool,
    /// The gimbal's joints (pan, tilt, roll; degrees) from its last attitude report.
    pub joints: Option<[f64; 3]>,
    /// The last failure, readable.
    pub error: Option<String>,
}

impl Status {
    /// `{"connected","state","step","steps","mount_ok","joints":[3]|null,"error":string|null}`.
    pub fn json(&self) -> String {
        serde_json::json!({
            "connected": self.connected, "state": self.state.name(), "step": self.step, "steps": self.steps,
            "entered": self.entered,
            "mount_ok": mount_ok(self.joints), "joints": self.joints, "error": self.error,
        })
        .to_string()
    }
}

/// Portrait upright: the roll joint (third of [`cave_osmo::Attitude::joints`]) sits within 80…112 (the Tauri app's
/// homeFrom rule).
pub fn mount_ok(joints: Option<[f64; 3]>) -> bool {
    joints.is_some_and(|j| (80.0..=112.0).contains(&j[2]))
}

#[derive(Deserialize)]
struct TargetIn {
    yaw: f64,
    roll: f64,
    pitch: f64,
    seconds: f64,
}

/// A plan: a JSON array of lists of `{yaw,roll,pitch,seconds}` target-mode targets, each list 1..=15 long.
pub fn parse_plan(json: &str) -> Result<Vec<Vec<Target>>, String> {
    let lists: Vec<Vec<TargetIn>> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if lists.is_empty() || lists.iter().any(|l| l.is_empty() || l.len() > LIST_MAX) {
        return Err(format!("a plan holds lists of 1 to {LIST_MAX} targets"));
    }
    if lists.iter().flatten().any(|t| {
        ![t.yaw, t.roll, t.pitch, t.seconds]
            .iter()
            .all(|v| v.is_finite())
            || t.seconds <= 0.0
    }) {
        return Err("a target needs finite angles and a positive move time".into());
    }
    Ok(lists
        .into_iter()
        .map(|l| {
            l.into_iter()
                .map(|t| Target {
                    yaw: t.yaw,
                    roll: t.roll,
                    pitch: t.pitch,
                    seconds: t.seconds,
                })
                .collect()
        })
        .collect())
}

enum Cmd {
    Connect(Duration),
    Run(Vec<Vec<Target>>),
    StopHome,
    Disconnect,
}

struct Runner {
    _rt: tokio::runtime::Runtime,
    tx: mpsc::UnboundedSender<Cmd>,
    status: Arc<Mutex<Status>>,
    hold: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

fn runner() -> &'static Runner {
    static R: OnceLock<Runner> = OnceLock::new();
    R.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("gimbal")
            .enable_all()
            .build()
            .expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let hold = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        rt.spawn(drive(rx, status.clone(), hold.clone(), stop.clone()));
        Runner {
            _rt: rt,
            tx,
            status,
            hold,
            stop,
        }
    })
}

fn set(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) {
    f(&mut status.lock().unwrap_or_else(|p| p.into_inner()))
}

fn joints(o: &Osmo) -> Option<[f64; 3]> {
    o.attitude().map(|(_, a)| a.joints())
}

/// Extra time past the scan for connecting (cave-osmo allows 15 s) before a connect is given up.
const CONNECT_SLACK: Duration = Duration::from_secs(20);

async fn drive(
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    status: Arc<Mutex<Status>>,
    hold: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    let mut osmo: Option<Osmo> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        let cmd = tokio::select! {
            c = rx.recv() => match c { Some(c) => c, None => break },
            _ = tick.tick() => {
                if let Some(o) = osmo.as_ref() {
                    let j = joints(o);
                    set(&status, |s| s.joints = j.or(s.joints));
                }
                continue;
            }
        };
        match cmd {
            Cmd::Connect(wait) => {
                if let Some(o) = osmo.take() {
                    o.disconnect().await;
                }
                set(&status, |s| {
                    *s = Status {
                        state: State::Connecting,
                        ..Status::default()
                    };
                });
                // its own task, so a panic inside Bluetooth is an error here and the runner lives on
                let found =
                    tokio::time::timeout(wait + CONNECT_SLACK, tokio::spawn(Osmo::find(wait)))
                        .await;
                match found {
                    Ok(Ok(Ok(o))) => {
                        let j = joints(&o);
                        osmo = Some(o);
                        set(&status, |s| {
                            s.connected = true;
                            s.state = State::Idle;
                            s.joints = j;
                        });
                    }
                    other => {
                        let e = match other {
                            Ok(Ok(Err(e))) => e.to_string(),
                            Ok(Err(_)) => "the Bluetooth library failed".to_string(),
                            _ => "timed out connecting to the gimbal".to_string(),
                        };
                        set(&status, |s| {
                            s.connected = false;
                            s.state = State::Error;
                            s.error = Some(e);
                        });
                    }
                }
            }
            Cmd::Run(lists) => {
                let Some(o) = osmo.as_mut() else {
                    set(&status, |s| {
                        s.state = State::Error;
                        s.error = Some("no gimbal connected".into());
                    });
                    continue;
                };
                set(&status, |s| s.error = None);
                let r = run(o, &lists, &status, &hold, &stop).await;
                let mut lost = false;
                set(&status, |s| {
                    match r {
                        Ok(()) => s.state = State::Done,
                        Err(e) => {
                            s.state = State::Error;
                            s.error = Some(e);
                        }
                    }
                    lost = !s.connected;
                });
                // a failed send means the link is gone: the next connect starts afresh
                if lost {
                    osmo = None;
                }
            }
            // a stop only means something inside `run` (the flag); between runs there is nothing to stop
            Cmd::StopHome => {}
            Cmd::Disconnect => {
                if let Some(o) = osmo.take() {
                    o.disconnect().await;
                }
                set(&status, |s| *s = Status::default());
            }
        }
    }
}

/// A failed send: the link is taken as lost (`connected` false), and the error ends the run.
fn lost(status: &Mutex<Status>) -> impl Fn(cave_osmo::Error) -> String + '_ {
    move |e| {
        set(status, |s| s.connected = false);
        format!("gimbal link lost: {e}")
    }
}

/// Runs each list: enter (sends home), upload, step through. Hold waits on a reached target, re-sending its "go"
/// at once and then every 5 s so target mode (which ends ~9 s after the last command) stays alive; stop goes
/// home and ends. A failed send marks the link lost.
async fn run(
    o: &mut Osmo,
    lists: &[Vec<Target>],
    status: &Mutex<Status>,
    hold: &AtomicBool,
    stop: &AtomicBool,
) -> Result<(), String> {
    let link = lost(status);
    let steps: u32 = lists.iter().map(|l| l.len() as u32).sum();
    set(status, |s| {
        s.steps = steps;
        s.step = 0;
        s.entered = false;
        s.state = State::Moving;
    });
    o.targets_enter(false).await.map_err(&link)?;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    set(status, |s| s.entered = true);
    let mut done = 0u32;
    for (n, list) in lists.iter().enumerate() {
        let id = (n % 200) as u8 + 1;
        if n > 0 {
            o.targets_leave().await.map_err(&link)?;
            o.targets_enter(true).await.map_err(&link)?;
            tokio::time::sleep(Duration::from_millis(2500)).await;
        }
        o.targets_upload(list, id).await.map_err(&link)?;
        for (i, t) in list.iter().enumerate() {
            let i = i as u8;
            if i > 0 {
                o.targets_go(i).await.map_err(&link)?;
            }
            let arrived = o
                .targets_arrived(i, id, Duration::from_secs_f64(t.seconds + 5.0))
                .await
                .map_err(&link)?;
            if !arrived {
                // a dropped list sends the gimbal home by itself
                let _ = o.targets_exit().await;
                let at = joints(o)
                    .map(|j| format!("pan {:.0} tilt {:.0} roll {:.0}", j[0], j[1], j[2]))
                    .unwrap_or_else(|| "no attitude".into());
                return Err(format!(
                    "target {} of list {} not reached (asked yaw {:.0} pitch {:.0}; joints {at})",
                    i + 1,
                    n + 1,
                    t.yaw,
                    t.pitch
                ));
            }
            done += 1;
            let j = joints(o);
            set(status, |s| {
                s.step = done;
                s.joints = j.or(s.joints);
            });
            // the keep-alive goes at once when a hold begins: the move to this target may have taken up to 8 s
            let mut last: Option<Instant> = None;
            while hold.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                set(status, |s| s.state = State::Holding);
                if last.is_none_or(|l| l.elapsed() > Duration::from_secs(5)) {
                    o.targets_go(i).await.map_err(&link)?;
                    last = Some(Instant::now());
                    let j = joints(o);
                    set(status, |s| s.joints = j.or(s.joints));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            set(status, |s| s.state = State::Moving);
            if stop.load(Ordering::SeqCst) {
                set(status, |s| s.state = State::Homing);
                let _ = o.targets_leave().await;
                let _ = o.targets_enter(true).await; // entering sends it home
                tokio::time::sleep(Duration::from_millis(2500)).await;
                let _ = o.targets_exit().await;
                return Ok(());
            }
        }
    }
    let _ = o.targets_exit().await;
    Ok(())
}

/// Looks for the gimbal for `ms` milliseconds and connects (status `connecting`, then `idle` or `error`).
pub fn connect(ms: u32) {
    let _ = runner()
        .tx
        .send(Cmd::Connect(Duration::from_millis(ms as u64)));
}

/// Queues a plan (see [`parse_plan`]); a bad plan is refused here and nothing is sent. The status reads
/// `moving` at once, so a poll right after never sees the last run's `done`.
pub fn run_plan(json: &str) -> Result<(), String> {
    let p = parse_plan(json)?;
    let r = runner();
    r.hold.store(false, Ordering::SeqCst);
    r.stop.store(false, Ordering::SeqCst);
    let steps: u32 = p.iter().map(|l| l.len() as u32).sum();
    set(&r.status, |s| {
        s.state = State::Moving;
        s.step = 0;
        s.steps = steps;
        s.entered = false;
        s.error = None;
    });
    r.tx.send(Cmd::Run(p)).map_err(|e| e.to_string())
}

/// Pauses on the next reached target until [`go_on`].
pub fn hold() {
    runner().hold.store(true, Ordering::SeqCst);
}

/// Ends a [`hold`].
pub fn go_on() {
    runner().hold.store(false, Ordering::SeqCst);
}

/// Stops the running plan at its next reached target and sends the gimbal home.
pub fn stop_home() {
    let r = runner();
    r.stop.store(true, Ordering::SeqCst);
    r.hold.store(false, Ordering::SeqCst);
    let _ = r.tx.send(Cmd::StopHome);
}

/// Disconnects (after any running plan ends) and resets the status.
pub fn disconnect() {
    let _ = runner().tx.send(Cmd::Disconnect);
}

/// The status JSON (see [`Status::json`]).
pub fn status() -> String {
    runner()
        .status
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_json_parses_and_refuses_bad_lists() {
        let ok = parse_plan(r#"[[{"yaw":10,"roll":0,"pitch":0,"seconds":1},{"yaw":0,"roll":0,"pitch":0,"seconds":1}]]"#).unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].len(), 2);
        assert_eq!(
            ok[0][0],
            Target {
                yaw: 10.0,
                roll: 0.0,
                pitch: 0.0,
                seconds: 1.0
            }
        );
        assert!(parse_plan("[]").is_err());
        assert!(parse_plan("[[]]").is_err());
        assert!(parse_plan("not json").is_err());
        assert!(parse_plan(r#"[[{"yaw":0,"roll":0,"pitch":0,"seconds":0}]]"#).is_err());
        let full: Vec<_> = (0..15)
            .map(|_| serde_json::json!({"yaw":0,"roll":0,"pitch":0,"seconds":1}))
            .collect();
        assert!(parse_plan(&serde_json::json!([full]).to_string()).is_ok());
        let big: Vec<_> = (0..16)
            .map(|_| serde_json::json!({"yaw":0,"roll":0,"pitch":0,"seconds":1}))
            .collect();
        assert!(parse_plan(&serde_json::json!([big]).to_string()).is_err());
    }

    #[test]
    fn status_json_and_mount_rule() {
        let mut s = Status::default();
        let j: serde_json::Value = serde_json::from_str(&s.json()).unwrap();
        assert_eq!(j["state"], "idle");
        assert_eq!(j["connected"], false);
        assert_eq!(j["mount_ok"], false);
        assert_eq!(j["entered"], false);
        assert!(j["joints"].is_null());
        assert!(j["error"].is_null());
        assert!(!mount_ok(None));
        assert!(mount_ok(Some([0.0, 0.0, 95.0])));
        assert!(mount_ok(Some([0.0, 0.0, 80.0])) && mount_ok(Some([0.0, 0.0, 112.0])));
        assert!(!mount_ok(Some([0.0, 0.0, 150.0])));
        assert!(!mount_ok(Some([95.0, 0.0, 0.0])), "roll is the third joint");
        s.state = State::Moving;
        s.step = 3;
        s.steps = 11;
        s.joints = Some([1.0, 2.0, 90.0]);
        s.error = Some("x".into());
        let j: serde_json::Value = serde_json::from_str(&s.json()).unwrap();
        assert_eq!(
            (j["state"].as_str(), j["step"].as_u64(), j["steps"].as_u64()),
            (Some("moving"), Some(3), Some(11))
        );
        assert_eq!(j["joints"], serde_json::json!([1.0, 2.0, 90.0]));
        assert_eq!(j["mount_ok"], true);
        assert_eq!(j["error"], "x");
        let names: Vec<_> = [
            State::Idle,
            State::Connecting,
            State::Moving,
            State::Holding,
            State::Homing,
            State::Done,
            State::Error,
        ]
        .iter()
        .map(|s| s.name())
        .collect();
        assert_eq!(
            names,
            [
                "idle",
                "connecting",
                "moving",
                "holding",
                "homing",
                "done",
                "error"
            ]
        );
    }

    #[test]
    fn a_bad_plan_is_refused_before_the_runner() {
        assert!(run_plan("[]").is_err());
    }
}
