// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Ask: runs an AI coding CLI already installed and signed in on this Mac — Claude Code, Codex,
//! Antigravity or GitHub Copilot — non-interactively, and streams its answer back to the window
//! as events. It has no tools: the message carries the scan, and the app itself deletes nothing.
//! A follow-up resumes the CLI's session. The model id says which CLI answers.

mod antigravity;
mod claude;
mod codex;
mod copilot;

use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

#[cfg(not(windows))]
const SYSTEM: &str = "You are an adviser inside Tidemark, a macOS app that records what is using a Mac's disk and how hard the Mac is working, over time. \
The user's message carries what the app knows: the folder on screen and the biggest folders inside it (sizes on disk, item counts, loose files), what grew on the disk recently, and the performance record (CPU, memory, swap and memory pressure now, today's highs, the week's high-water marks, incidents with the apps that were busiest at each, and the apps that used the most). \
Answer the question asked. For questions about Tidemark itself (what a screen, figure or setting means, how to do something in the app), answer from the user guide below. For disk questions, find where space can be reclaimed safely: say in plain language what each big folder is for, which ones are safe to clear or shrink and how (an app's own clear-cache or empty-trash command, a Finder step, a Terminal command), roughly how much each would free, and what to leave alone: system folders, cloud placeholders that take no local space, and anything the user would have to recreate by hand; rank by space reclaimed against risk. \
For performance questions, explain what happened from the record: which resource ran short, when, and which apps were responsible, in terms of the figures given; say what would help (quitting or limiting an app, stopping a VM, closing tabs) and when something is normal and not worth acting on. \
Be honest when a folder's or process's purpose is unclear from its name, and ask before guessing. \
The app deletes and changes nothing and you have no tools and no files to read, so answer from the message alone, run no commands, and never claim to have checked or removed anything; if a Terminal command would help the user, give it exactly, and warn when it is destructive. \
The app's figures are measured facts, not faults. The disk's used figure is larger than the scan total, and that gap is expected: the guide below says why (local snapshots, purgeable space, swap, folders the scan skips or cannot read). Never suggest the app is wrong or broken. Mention the gap or skipped folders only when the user asks or it changes the advice, briefly, and when folders could not be read end with the Full Disk Access step. \
Write tightly: short paragraphs or bullets, headings only when they help, paths and app names as written in the message.";
/// The same, for Windows: its words, its tools (File Explorer, PowerShell), and why its disk's
/// used figure is larger than the scan (no Full Disk Access to suggest).
#[cfg(windows)]
const SYSTEM: &str = "You are an adviser inside Tidemark, a Windows app that records what is using a PC's disk and how hard the PC is working, over time. \
The user's message carries what the app knows: the folder on screen and the biggest folders inside it (sizes on disk, item counts, loose files), what grew on the disk recently, and the performance record (CPU, memory, swap and memory pressure now, today's highs, the week's high-water marks, incidents with the apps that were busiest at each, and the apps that used the most). \
Answer the question asked. For questions about Tidemark itself (what a screen, figure or setting means, how to do something in the app), answer from the user guide below. For disk questions, find where space can be reclaimed safely: say in plain language what each big folder is for, which ones are safe to clear or shrink and how (an app's own clear-cache or empty-trash command, a File Explorer step, Windows' Disk Cleanup or Storage Sense, a PowerShell command), roughly how much each would free, and what to leave alone: system folders, cloud placeholders that take no local space, and anything the user would have to recreate by hand; rank by space reclaimed against risk. \
For performance questions, explain what happened from the record: which resource ran short, when, and which apps were responsible, in terms of the figures given; say what would help (quitting or limiting an app, stopping a VM, closing tabs) and when something is normal and not worth acting on. \
Be honest when a folder's or process's purpose is unclear from its name, and ask before guessing. \
The app deletes and changes nothing and you have no tools and no files to read, so answer from the message alone, run no commands, and never claim to have checked or removed anything; if a PowerShell command would help the user, give it exactly, and warn when it is destructive. \
The app's figures are measured facts, not faults. The disk's used figure is larger than the scan total, and that gap is expected: the guide below says why (System Restore points and shadow copies, the file system's own records, folders the scan cannot read). Never suggest the app is wrong or broken. Mention the gap or skipped folders only when the user asks or it changes the advice, briefly, and when folders could not be read say they are mostly ones only an administrator can open. \
Write tightly: short paragraphs or bullets, headings only when they help, paths and app names as written in the message.";

/// The in-app help's pages (src/help/), in the order the drawer shows them (src/guides.ts): the
/// whole guide, about 20 KB, so Ask can explain the app's figures as the guide does and answer
/// how anything in Tidemark works (Help's own button sends that question here).
/// Each page's name (its file in src/help/ without `.md`) and its text.
macro_rules! help_pages {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_str!(concat!("../../../src/help/", $name, ".md")))),*]
    };
}
const HELP: [(&str, &str); 10] = help_pages![
    "intro",
    "now",
    "charts",
    "performance",
    "disk-map",
    "trends",
    "settings",
    "ask",
    "menu-bar",
    "keyboard",
];

/// A guide page as the help shows it on this platform (src/guides.ts forPlatform): the other
/// platform's parts and the words joining the two for GitHub gone, the markers too.
fn for_platform(md: &str) -> String {
    let other = if cfg!(windows) { "mac" } else { "windows" };
    let cut = |s: String, name: &str| {
        let (open, close) = (format!("<!-- {name} -->"), format!("<!-- /{name} -->"));
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(a) = rest.find(&open) {
            out.push_str(&rest[..a]);
            rest = rest[a..]
                .find(&close)
                .map_or("", |b| &rest[a + b + close.len()..]);
        }
        out.push_str(rest);
        out
    };
    let mut s = cut(cut(md.to_string(), "github"), other);
    for m in [
        "<!-- mac -->",
        "<!-- /mac -->",
        "<!-- windows -->",
        "<!-- /windows -->",
    ] {
        s = s.replace(m, "");
    }
    while s.contains("\n\n\n") {
        s = s.replace("\n\n\n", "\n\n");
    }
    s
}

/// The system prompt: SYSTEM, then the whole guide as this platform's help has it, built once.
fn system() -> &'static str {
    static PROMPT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let guide: Vec<String> = HELP.iter().map(|(_, p)| for_platform(p.trim())).collect();
        format!(
            "{SYSTEM}\n\nThe app's user guide:\n\n{}",
            guide.join("\n\n")
        )
    });
    &PROMPT
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Chunk {
    chat_id: String,
    text: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    chat_id: String,
    session_id: Option<String>,
    text: String,
    error: Option<String>,
}

/// The CLI processes under way, by chat, so a chat can be stopped and nothing outlives the app.
#[derive(Default)]
pub struct Running {
    children: Mutex<Vec<(String, u32)>>,
}

impl Running {
    fn remove(&self, pid: u32) {
        if let Ok(mut ch) = self.children.lock() {
            ch.retain(|(_, p)| *p != pid);
        }
    }
    /// A CLI is answering: an update's restart would stop it (updates.rs waits).
    pub fn busy(&self) -> bool {
        self.children.lock().is_ok_and(|ch| !ch.is_empty())
    }
    pub fn kill_all(&self) {
        if let Ok(ch) = self.children.lock() {
            for (_, pid) in ch.iter() {
                kill_group(*pid);
            }
        }
    }
}

/// The CLIs start helpers of their own, so the whole process group goes.
fn kill_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
    }
    // Windows has no process groups: taskkill ends the CLI and everything it started (/T).
    #[cfg(windows)]
    {
        let mut kill = Command::new("taskkill");
        kill.args(["/T", "/F", "/PID", &pid.to_string()]);
        let _ = crate::process::no_window(&mut kill).status();
    }
}

/// Where a CLI is installed: the usual places first, then the login shell's PATH.
#[cfg(not(windows))]
fn find(name: &str, extra: &[&str]) -> Option<PathBuf> {
    let home = crate::cache::home();
    let mut places = vec![home.join(".local/bin").join(name)];
    places.extend(extra.iter().map(|p| home.join(p)));
    places.extend([
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ]);
    if let Some(p) = places.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    let out = Command::new("/bin/zsh")
        .args(["-lc", &format!("command -v {name}")])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty() && PathBuf::from(&s).is_file()).then(|| PathBuf::from(s))
}

/// Where a CLI is installed on Windows: the native installers' places (Claude Code's,
/// Antigravity's), npm's own folder, then whatever `where` finds on the PATH, which an app
/// started before the install doesn't have yet. An npm .cmd shim comes last: a prompt's newlines
/// can't be passed through one, so the program inside the npm package is run instead.
#[cfg(windows)]
fn find(name: &str, _extra: &[&str]) -> Option<PathBuf> {
    let home = crate::cache::home();
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"));
    let exe = format!("{name}.exe");
    if let Some(p) = [
        home.join(".local/bin").join(&exe),
        local.join(name).join("bin").join(&exe),
    ]
    .into_iter()
    .find(|p| p.is_file())
    {
        return Some(p);
    }
    // npm's own folder, which isn't on the PATH when Node was installed by another account.
    if let Some(npm) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("npm")) {
        if npm.join(format!("{name}.cmd")).is_file() {
            if let Some(p) = in_npm_package(&npm.join("node_modules"), name) {
                return Some(p);
            }
        }
    }
    let out = crate::process::no_window(Command::new("where").arg(name))
        .output()
        .ok()?;
    let found: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .filter(|p| p.is_file())
        .collect();
    let ext = |p: &PathBuf, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    if let Some(p) = found.iter().find(|p| ext(p, "exe")) {
        return Some(p.clone());
    }
    let shim = found.iter().find(|p| ext(p, "cmd"))?;
    shim.parent()
        .and_then(|d| in_npm_package(&d.join("node_modules"), name))
        .or_else(|| Some(shim.clone()))
}

/// The program itself inside an npm install of `name` (Codex's and Copilot's packages carry it,
/// as codex-win32-x64\…\codex.exe), to run in place of npm's .cmd shim.
#[cfg(windows)]
fn in_npm_package(modules: &std::path::Path, name: &str) -> Option<PathBuf> {
    fn walk(d: &std::path::Path, exe: &str, depth: u8) -> Option<PathBuf> {
        let mut dirs = Vec::new();
        for e in std::fs::read_dir(d).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(exe)) {
                return Some(p);
            }
        }
        if depth == 0 {
            return None;
        }
        dirs.into_iter().find_map(|p| walk(&p, exe, depth - 1))
    }
    let exe = format!("{name}.exe");
    // The package is `name` or `@scope/name`.
    let mut roots = vec![modules.join(name)];
    roots.extend(
        std::fs::read_dir(modules)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|f| f.to_string_lossy().starts_with('@'))
            })
            .map(|p| p.join(name)),
    );
    roots
        .into_iter()
        .filter(|r| r.is_dir())
        .find_map(|r| walk(&r, &exe, 6))
}

fn version(bin: &PathBuf) -> Option<String> {
    let out = crate::process::no_window(Command::new(bin).arg("--version"))
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_end_matches('.').to_string())
}

#[derive(Serialize)]
pub struct Model {
    id: String,
    name: String,
}

#[derive(Serialize)]
pub struct Cli {
    path: Option<String>,
    version: Option<String>,
    models: Vec<Model>,
}

#[derive(Serialize)]
pub struct Status {
    claude: Cli,
    codex: Cli,
    antigravity: Cli,
    copilot: Cli,
}

/// Which CLIs are installed, and the models each offers. With no models anywhere, the window
/// disables Ask and says why.
#[tauri::command]
pub async fn assistant_status() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let cli = |bin: Option<PathBuf>, models: Vec<Model>| Cli {
            version: bin.as_ref().and_then(version),
            models: if bin.is_some() { models } else { Vec::new() },
            path: bin.map(|p| p.to_string_lossy().to_string()),
        };
        Status {
            claude: cli(claude::find(), claude::models()),
            codex: cli(codex::find(), codex::models()),
            antigravity: cli(antigravity::find(), antigravity::models()),
            copilot: cli(copilot::find(), copilot::models()),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Windows holds a command line to 32,767 characters, which the guide in the instructions and a
/// long question together pass: there the question goes in on stdin, which Claude Code (`-p`
/// with no prompt), Codex (`-`) and Copilot (no `-p`) read when it is piped.
pub(crate) const PROMPT_ON_STDIN: bool = cfg!(windows);

/// Starts the CLI, with `input` written to its stdin when there is any.
fn spawn(
    cmd: &mut Command,
    running: &Running,
    chat_id: &str,
    name: &str,
    input: Option<String>,
) -> Result<(Child, ChildStdout, std::thread::JoinHandle<String>), String> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd, 0);
    crate::process::no_window(cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Couldn't start {name}: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    // Written on a thread of its own and then closed, so the CLI sees the end of it.
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let stderr = child.stderr.take();
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    if let Ok(mut ch) = running.children.lock() {
        ch.push((chat_id.to_string(), child.id()));
    }
    Ok((child, stdout, err))
}

fn finish(mut child: Child, running: &Running) -> bool {
    let ok = child.wait().is_ok_and(|s| s.success());
    running.remove(child.id());
    ok
}

/// The error for a CLI that answered nothing: its stderr if it failed with any, else a plain line.
fn silent(name: &str, ok: bool, stderr: std::thread::JoinHandle<String>) -> String {
    let msg = stderr.join().unwrap_or_default();
    if msg.trim().is_empty() || ok {
        format!("{name} stopped without answering")
    } else {
        msg.trim().to_string()
    }
}

/// Puts a question to the CLI the model id names; the answer arrives as `ask-chunk` events while
/// it is written and `ask-done` closes it, with the session to resume for a follow-up.
#[tauri::command]
pub async fn ask(
    app: AppHandle,
    running: tauri::State<'_, Arc<Running>>,
    chat_id: String,
    prompt: String,
    model: String,
    session: Option<String>,
) -> Result<(), String> {
    let cli = if model.starts_with(antigravity::PREFIX) {
        "agy"
    } else if model.starts_with(copilot::PREFIX) {
        "copilot"
    } else if model.starts_with("claude") {
        "claude"
    } else {
        "codex"
    };
    // Each CLI gets an empty folder of its own to run in, so there is nothing to read.
    let cwd = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(cli);
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    let running = Arc::clone(&running);
    match cli {
        "agy" => antigravity::ask(app, running, cwd, chat_id, prompt, model, session),
        "copilot" => copilot::ask(app, running, cwd, chat_id, prompt, model, session),
        "claude" => claude::ask(app, running, cwd, chat_id, prompt, model, session),
        _ => codex::ask(app, running, cwd, chat_id, prompt, model, session),
    }
}

/// Stops a chat's answer; the window hears `ask-done` as the process ends.
#[tauri::command]
pub fn ask_cancel(running: tauri::State<'_, Arc<Running>>, chat_id: String) {
    if let Ok(ch) = running.children.lock() {
        for (_, pid) in ch.iter().filter(|(id, _)| id == &chat_id) {
            kill_group(*pid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole guide, every page of it, follows the instructions.
    #[test]
    fn prompt_carries_the_whole_guide() {
        let p = system();
        assert!(p.starts_with(SYSTEM));
        for (_, page) in HELP {
            assert!(p.contains(&for_platform(page.trim())));
        }
        // Help's sections on the app's own figures among them.
        let unread = if cfg!(windows) {
            "Folders the scan can't read"
        } else {
            "Full Disk Access"
        };
        for title in ["Free space", unread, "Trends"] {
            assert!(p.contains(&format!("## {title}\n")), "missing {title}");
        }
    }

    /// Every platform part of the guide is closed, so the help and Ask drop only what they should.
    #[test]
    fn platform_markers_are_balanced() {
        for (name, page) in HELP {
            for m in ["mac", "windows", "github"] {
                assert_eq!(
                    page.matches(&format!("<!-- {m} -->")).count(),
                    page.matches(&format!("<!-- /{m} -->")).count(),
                    "{name}.md has an unclosed <!-- {m} --> part"
                );
            }
        }
    }

    #[test]
    fn keeps_this_platforms_parts() {
        let md = "Show in <!-- mac -->Finder<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->File Explorer<!-- /windows -->.\n\n<!-- windows -->\nOnly Windows.\n<!-- /windows -->\n\nEnd.";
        let s = for_platform(md);
        if cfg!(windows) {
            assert_eq!(s, "Show in File Explorer.\n\nOnly Windows.\n\nEnd.");
        } else {
            assert_eq!(s, "Show in Finder.\n\nEnd.");
        }
    }

    /// Every page in src/help/ is in HELP, and nothing else: a page added to the drawer is
    /// added here too, or Ask wouldn't know it.
    #[test]
    fn help_lists_every_page() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/help");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("src/help is readable")
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "md").then(|| p.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        files.sort();
        let mut listed: Vec<String> = HELP.iter().map(|(n, _)| n.to_string()).collect();
        listed.sort();
        assert_eq!(listed, files);
    }
}
