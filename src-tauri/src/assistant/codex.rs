// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Codex (`codex exec --json`): messages arrive whole, not as deltas, so the answer is sent once,
//! at the end. The user's own Codex config (MCP servers, hooks, rules) is left out; sign-in still
//! comes from ~/.codex. Runs in the read-only sandbox, in an empty folder, with whatever reaches
//! past the message switched off: the shell (its only way to read a file), web search, the
//! ChatGPT apps and plugins (connectors to the user's other accounts), images, the browser and
//! computer use, sub-agents and goals. What is left can't read or change anything: a clock, and
//! apply_patch, which the read-only sandbox refuses.

use super::{finish, silent, spawn, system, Done, Model, Running, PROMPT_ON_STDIN};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Codex's features that reach past the message: switched off for every run.
const NO_FEATURES: [&str; 10] = [
    "shell_tool",
    "unified_exec",
    "apps",
    "plugins",
    "view_image",
    "image_generation",
    "browser_use",
    "computer_use",
    "multi_agent",
    "goals",
];

pub fn find() -> Option<PathBuf> {
    super::find("codex", &[])
}

/// The models the Codex CLI would offer in its picker, from the list it caches for the
/// signed-in account. Empty if Codex has never run, so the app shows nothing to choose.
pub fn models() -> Vec<Model> {
    let home = crate::cache::home();
    let Ok(s) = std::fs::read_to_string(home.join(".codex/models_cache.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return Vec::new();
    };
    let mut ms: Vec<(i64, Model)> = v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            let id = m["slug"].as_str()?.to_string();
            let name = m["display_name"].as_str().unwrap_or(&id).to_string();
            Some((
                m["priority"].as_i64().unwrap_or(i64::MAX),
                Model { id, name },
            ))
        })
        .collect();
    ms.sort_by_key(|(p, _)| *p);
    ms.into_iter().map(|(_, m)| m).collect()
}

pub fn ask(
    app: AppHandle,
    running: Arc<Running>,
    cwd: PathBuf,
    chat_id: String,
    prompt: String,
    model: String,
    session: Option<String>,
) -> Result<(), String> {
    let bin = find().ok_or(
        "Codex isn't installed, or couldn't be found. Install it and sign in, then try again.",
    )?;
    // -c values are TOML; a JSON string is a valid TOML basic string.
    let instructions = format!(
        "developer_instructions={}",
        serde_json::to_string(system()).map_err(|e| e.to_string())?
    );
    let mut cmd = Command::new(bin);
    cmd.current_dir(&cwd).arg("exec");
    if session.is_some() {
        cmd.arg("resume");
    }
    cmd.args([
        "--json",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
        "-m",
        &model,
    ])
    .args(["-c", "sandbox_mode=\"read-only\"", "-c", &instructions])
    .args(["-c", "web_search=\"disabled\""]);
    for f in NO_FEATURES {
        cmd.args(["-c", &format!("features.{f}=false")]);
    }
    cmd.arg("--");
    if let Some(s) = &session {
        cmd.arg(s);
    }
    // "-" reads the prompt from stdin.
    cmd.arg(if PROMPT_ON_STDIN {
        "-"
    } else {
        prompt.as_str()
    });
    let (child, stdout, stderr) = spawn(
        &mut cmd,
        &running,
        &chat_id,
        "Codex",
        PROMPT_ON_STDIN.then(|| prompt.clone()),
    )?;

    std::thread::spawn(move || {
        let mut text = String::new();
        let mut session_id = None;
        let mut error = None;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            match v["type"].as_str() {
                Some("thread.started") => {
                    if let Some(s) = v["thread_id"].as_str() {
                        session_id = Some(s.to_string());
                    }
                }
                // Earlier messages can be progress notes; the answer is the last one.
                Some("item.completed") if v["item"]["type"] == "agent_message" => {
                    if let Some(t) = v["item"]["text"].as_str() {
                        text = t.to_string();
                    }
                }
                Some("turn.failed") => {
                    error = Some(
                        v["error"]["message"]
                            .as_str()
                            .unwrap_or("Codex returned an error")
                            .to_string(),
                    )
                }
                Some("error") => {
                    error = Some(
                        v["message"]
                            .as_str()
                            .unwrap_or("Codex returned an error")
                            .to_string(),
                    )
                }
                _ => {}
            }
        }
        // Codex also reports transient trouble (a dropped stream it then reconnects) as errors,
        // so one only counts when no answer came.
        if !text.is_empty() {
            error = None;
        }
        let ok = finish(child, &running);
        if error.is_none() && !ok && text.is_empty() {
            error = Some(silent("Codex", ok, stderr));
        }
        let _ = app.emit(
            "ask-done",
            Done {
                chat_id,
                session_id,
                text,
                error,
            },
        );
    });
    Ok(())
}
