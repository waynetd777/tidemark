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
//! retried for days. `refresh`, at each launch of the installed app, moves an old registration
//! over, keeps the agent pointing at the binary, and restarts it on a new build.

use serde::Serialize;

/// The launch agent's label and plist. Not the old SMAppService one's (`PLIST`): macOS keeps a
/// background-item record under that label, and launchd refuses to start a plain agent that
/// shares it.
pub const LABEL: &str = "com.wayned.tidemark.helper";
const AGENT_PLIST: &str = "com.wayned.tidemark.helper.plist";
/// The bundle's legacy plist (Contents/Library/LaunchAgents), kept only so an old SMAppService
/// registration can be removed; and the label a plain agent briefly had too.
pub const PLIST: &str = "com.wayned.tidemark.recorder.plist";
const OLD_LABEL: &str = "com.wayned.tidemark.recorder";

/// The argument the launch agent passes.
pub const ARG: &str = "--record";

/// Runs the helper until it is stopped. Never returns.
pub fn run() -> ! {
    // No window of its own: slow sampling, unless the app says one is on screen.
    crate::monitor::FAST.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::monitor::start();
    crate::schedule::start_headless();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
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

/// ~/Library/LaunchAgents/<name>.
fn agents_dir() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var_os("HOME")?).join("Library/LaunchAgents"))
}

/// ~/Library/LaunchAgents/com.wayned.tidemark.helper.plist.
fn agent_path() -> Option<std::path::PathBuf> {
    Some(agents_dir()?.join(AGENT_PLIST))
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

/// Only an installed app: the agent points at a binary that stays where it is.
fn available() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// The binary the installed agent runs, from its plist; None when there is no agent.
fn agent_program() -> Option<String> {
    let v: plist::Value = plist::from_file(agent_path()?).ok()?;
    let args = v.as_dictionary()?.get("ProgramArguments")?.as_array()?;
    args.first()?.as_string().map(str::to_string)
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
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if plist::to_file_xml(&path, &plist::Value::Dictionary(d)).is_err() {
        return false;
    }
    // A loaded copy of an older plist goes first, or bootstrap refuses ("already loaded").
    let _ = launchctl(&["bootout", &format!("{}/{LABEL}", domain())]);
    launchctl(&["bootstrap", &domain(), &path.to_string_lossy()])
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
    let installed = agent_program().is_some();
    let off = installed && disabled();
    Helper {
        available: available(),
        on: installed && !off,
        needs_approval: off,
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

/// At the installed app's launch: moves an old SMAppService registration to the plain agent
/// (keeping whether it was on), points the agent at this binary if the app has moved, and
/// restarts the helper on a new build so it runs the new code.
pub fn refresh() {
    if crate::scene::active() || !available() {
        return;
    }
    // Installed just now: already running this binary, so not loaded again below.
    let mut fresh = false;
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {
        use crate::login_item::{self, Status};
        // SMAppService hands back autoreleased objects; this thread has no pool of its own.
        let old = login_item::agent_status();
        if matches!(old, Status::Enabled | Status::RequiresApproval) {
            login_item::agent_set(false);
            if old == Status::Enabled && agent_program().is_none() {
                fresh = install();
            }
        }
    });
    // A plain agent under the old label (one build wrote it): it can't start; this one replaces
    // it.
    if let Some(old) = agents_dir().map(|d| d.join(PLIST)).filter(|p| p.exists()) {
        let _ = launchctl(&["bootout", &format!("{}/{OLD_LABEL}", domain())]);
        let _ = std::fs::remove_file(old);
        if agent_program().is_none() {
            fresh = install();
        }
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
    if !fresh && (moved || rebuilt) {
        install();
    }
    remember_build();
}

#[tauri::command]
pub fn background_helper() -> Helper {
    helper()
}

/// Turns the helper on or off. Switched off in System Settings, turning it on opens Login Items,
/// where only the user can allow it.
#[tauri::command]
pub fn set_background_helper(on: bool) -> Helper {
    if !available() {
        return helper();
    }
    if on {
        if install() {
            remember_build();
        }
        if disabled() {
            #[cfg(target_os = "macos")]
            crate::login_item::open_settings();
        }
    } else {
        uninstall();
    }
    helper()
}
