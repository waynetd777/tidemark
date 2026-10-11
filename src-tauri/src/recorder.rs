// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The background helper: the app's own binary run as `Tidemark --record` by a launch agent, so
//! the record goes on while the app is quit. It has no window and no Dock icon; it records
//! (monitor.rs) and keeps the disk's schedule (schedule.rs) whenever it holds the recorder lock,
//! which it does unless the app took it first, and then takes over when the app quits.
//!
//! The agent is a plain one: a plist the app writes to ~/Library/LaunchAgents, pointing at the
//! installed binary, loaded with `launchctl`. macOS lists it in System Settings › General ›
//! Login Items › Allow in the Background, where switching it off disables it (`launchctl
//! print-disabled`). It used to be registered through SMAppService from a plist inside the
//! bundle, but for an app signed with a self-signed identity macOS gave that registration no
//! build it would allow, and launchd refused to start it (OS_REASON_CODESIGNING, exit 78) and
//! retried for days. `refresh`, at each launch of the installed app, keeps the agent pointing
//! at the binary and restarts it on a new build.

use serde::Serialize;

/// The launch agent's label and plist. Not the old SMAppService one's
/// (com.wayned.tidemark.recorder): macOS keeps a background-item record under that label, and
/// launchd refuses to start a plain agent that shares it.
pub const LABEL: &str = "com.wayned.tidemark.helper";
const AGENT_PLIST: &str = "com.wayned.tidemark.helper.plist";

/// The argument the launch agent passes.
pub const ARG: &str = "--record";

/// Runs the helper until it is stopped. Never returns.
pub fn run() -> ! {
    // Before any other thread starts, so every one inherits the blocked signals.
    on_stop_signal();
    trim_log();
    // No window of its own: slow sampling, unless the app says one is on screen.
    crate::monitor::FAST.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::monitor::HEADLESS.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::monitor::start();
    crate::schedule::start_headless();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
        // A helper can run for weeks between logins: the log is kept small as it goes too.
        trim_log();
    }
}

/// SIGTERM (launchd stopping the agent, `make install-app`, logout, Activity Monitor's Quit,
/// `killall`) and SIGINT end the helper cleanly: the minute in hand is saved and open incidents
/// end now. The signals are blocked and taken by a thread of their own with sigwait, where the
/// work is safe to do. The helper then dies of the same signal rather than exiting 0: the agent
/// is restarted only after an unsuccessful exit (KeepAlive SuccessfulExit = false, see
/// `install`), so a helper quit by hand comes back, while one booted out (switched off, a
/// reinstall) stays gone, as launchd no longer has the job.
fn on_stop_signal() {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        if libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("signals".into())
            .spawn(move || {
                let mut sig: libc::c_int = 0;
                if libc::sigwait(&set, &mut sig) == 0 {
                    crate::monitor::stop();
                    libc::signal(sig, libc::SIG_DFL);
                    let mut one: libc::sigset_t = std::mem::zeroed();
                    libc::sigemptyset(&mut one);
                    libc::sigaddset(&mut one, sig);
                    libc::pthread_sigmask(libc::SIG_UNBLOCK, &one, std::ptr::null_mut());
                    libc::raise(sig);
                    // Not reached; should the signal not kill, still not a success.
                    std::process::exit(128 + sig);
                }
            });
    }
}

/// The helper's standard error, where launchd puts what it reports (see `install`).
fn log_path() -> std::path::PathBuf {
    crate::cache::dir().join("helper.log")
}

/// Keeps the log small: emptied, at start and then hourly, once it passes 1 MB. launchd opens it
/// to append (O_APPEND), so the helper's writes go on at the new end, not past a hole.
fn trim_log() {
    let path = log_path();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000) {
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = f.set_len(0);
        }
    }
}

#[derive(Serialize)]
pub struct Helper {
    /// Only the installed app can install it.
    pub available: bool,
    pub on: bool,
    /// Installed, but switched off in System Settings: it needs allowing there.
    pub needs_approval: bool,
}

/// ~/Library/LaunchAgents/com.wayned.tidemark.helper.plist.
fn agent_path() -> Option<std::path::PathBuf> {
    Some(
        std::path::PathBuf::from(std::env::var_os("HOME")?)
            .join("Library/LaunchAgents")
            .join(AGENT_PLIST),
    )
}

/// The launchd domain of this user's session: gui/<uid>.
fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn launchctl(args: &[&str]) -> bool {
    std::process::Command::new("/bin/launchctl")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Only an installed app: the agent points at a binary that stays where it is. Not one opened
/// from a disk image or a translocated copy, which go away.
fn available() -> bool {
    std::env::current_exe().is_ok_and(|p| {
        let p = p.to_string_lossy();
        let home_apps = std::env::var("HOME")
            .map(|h| format!("{h}/Applications/"))
            .unwrap_or_default();
        p.contains(".app/Contents/MacOS/")
            && (p.starts_with("/Applications/")
                || (!home_apps.is_empty() && p.starts_with(&home_apps)))
            && !p.contains("/AppTranslocation/")
    })
}

/// The binary the installed agent runs, from its plist; None when there is no agent.
fn agent_program() -> Option<String> {
    let v: plist::Value = plist::from_file(agent_path()?).ok()?;
    let args = v.as_dictionary()?.get("ProgramArguments")?.as_array()?;
    args.first()?.as_string().map(str::to_string)
}

/// Loaded into launchd now. Switching Tidemark off in System Settings' Login Items unloads it
/// (and macOS lists it under the same switch as the app's old registration).
fn loaded() -> bool {
    launchctl(&["print", &format!("{}/{LABEL}", domain())])
}

/// Switched off in System Settings' Login Items (launchd keeps it as disabled).
fn disabled() -> bool {
    std::process::Command::new("/bin/launchctl")
        .args(["print-disabled", &domain()])
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout).lines().any(|l| {
                // "=> disabled" now; "=> true" on older macOS.
                l.contains(&format!("\"{LABEL}\""))
                    && (l.contains("disabled") || l.trim_end().ends_with("true"))
            })
        })
}

/// Writes the agent's plist for this binary and loads it (it starts at once and at each login).
fn install() -> bool {
    let (Some(path), Ok(exe)) = (agent_path(), std::env::current_exe()) else {
        return false;
    };
    let mut d = plist::Dictionary::new();
    d.insert("Label".into(), LABEL.into());
    d.insert(
        "ProgramArguments".into(),
        plist::Value::Array(vec![exe.to_string_lossy().into_owned().into(), ARG.into()]),
    );
    d.insert(
        "AssociatedBundleIdentifiers".into(),
        plist::Value::Array(vec!["com.wayned.tidemark".into()]),
    );
    d.insert("RunAtLoad".into(), true.into());
    // Restarted only after a failure, at most every 5 minutes, so a helper that can't run
    // retries rarely rather than all day.
    let mut keep = plist::Dictionary::new();
    keep.insert("SuccessfulExit".into(), false.into());
    d.insert("KeepAlive".into(), keep.into());
    d.insert("ThrottleInterval".into(), 300.into());
    d.insert("ProcessType".into(), "Background".into());
    // What the helper reports (eprintln) lands in the data folder, not nowhere.
    let _ = std::fs::create_dir_all(crate::cache::dir());
    d.insert(
        "StandardErrorPath".into(),
        log_path().to_string_lossy().into_owned().into(),
    );
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if plist::to_file_xml(&path, &plist::Value::Dictionary(d)).is_err() {
        return false;
    }
    // A loaded copy of an older plist goes first, or bootstrap refuses ("already loaded"); while
    // a running helper is still going, launchd refuses again for a moment, so it is retried.
    // Not when switched off in Login Items: launchd refuses every time, and waiting out the
    // retries would only hold up Settings.
    let _ = launchctl(&["bootout", &format!("{}/{LABEL}", domain())]);
    let tries = if disabled() { 1 } else { 10 };
    for _ in 0..tries {
        if launchctl(&["bootstrap", &domain(), &path.to_string_lossy()]) || loaded() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

fn uninstall() {
    let _ = launchctl(&["bootout", &format!("{}/{LABEL}", domain())]);
    if let Some(path) = agent_path() {
        let _ = std::fs::remove_file(path);
    }
}

fn helper() -> Helper {
    if crate::scene::active() {
        return Helper {
            available: true,
            on: true,
            needs_approval: false,
        };
    }
    // On: installed and running under launchd. Switched off in Login Items is the one case only
    // the user can undo there; installed but not loaded otherwise just reads as off.
    let installed = agent_program().is_some();
    let off_there = installed && disabled();
    Helper {
        available: available(),
        on: installed && loaded() && !off_there,
        needs_approval: off_there,
    }
}

/// This build of the app: its version, and the binary's size and time, which every rebuild
/// changes.
fn build_id() -> Option<String> {
    let m = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    let t = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{} {} {t}", env!("CARGO_PKG_VERSION"), m.len()))
}

/// Where the build the helper last started on is kept.
fn build_file() -> std::path::PathBuf {
    crate::cache::dir().join("recorder.build")
}

fn remember_build() {
    if let Some(b) = build_id() {
        let _ = std::fs::create_dir_all(crate::cache::dir());
        if let Err(e) = std::fs::write(build_file(), b) {
            eprintln!("recorder: keeping the build: {e}");
        }
    }
}

/// At the installed app's launch: points the agent at this binary if the app has moved, and
/// restarts the helper on a new build so it runs the new code.
pub fn refresh() {
    if crate::scene::active() || !available() {
        return;
    }
    let Some(program) = agent_program() else {
        return;
    };
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned());
    // Moved, or a new build in the same place (the running helper is still the old binary):
    // loaded afresh, which starts it at once. (A `kickstart -k` would be held back by the
    // ThrottleInterval when the helper had just started.)
    let moved = exe.as_deref().ok() != Some(program.as_str());
    let rebuilt = std::fs::read_to_string(build_file()).ok() != build_id();
    // Reloaded only while it runs (not when switched off in Login Items); the build is kept
    // once the helper is running it, so a reload that didn't take is tried at the next launch.
    if !(moved || rebuilt) || (loaded() && install()) {
        remember_build();
    }
}

/// Off the main thread (async): it runs launchctl, which can take a moment.
#[tauri::command(async)]
pub fn background_helper() -> Helper {
    helper()
}

/// Turns the helper on or off. Switched off in System Settings, turning it on opens Login Items,
/// where only the user can allow it. Off the main thread (async): loading the agent can take
/// seconds.
#[tauri::command(async)]
pub fn set_background_helper(on: bool) -> Helper {
    if !available() {
        return helper();
    }
    if on {
        if install() {
            remember_build();
        }
        // Switched off in Login Items, which only the user can switch back on.
        if disabled() {
            #[cfg(target_os = "macos")]
            crate::login_item::open_settings();
        }
    } else {
        uninstall();
    }
    helper()
}
