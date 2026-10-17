//! After a clear, the bridge names the conversation the person is in (2026-10-17).
//!
//! In Claude Code's terminal app, `/clear` starts a new conversation inside
//! the same host process. The plugin's MCP server, this bridge, keeps
//! running, and the session id it was started with is the old
//! conversation's. The session-start hook does hear the new id: it runs on a
//! clear, and on a resume, with the conversation now open. So the hook leaves
//! that conversation's name where this host's bridge looks for it, one small
//! file per host process under `~/.designless/host-session/`, and the bridge
//! reads it on every call and prefers it to the name it started with.
//!
//! The file is named by the host's process id and carries the moment the
//! host started. A reader whose host started at another moment ignores it,
//! so a process id the system hands out again never inherits a name. Each
//! side finds the host by walking up from its own parent past any shell: the
//! host starts the bridge, and runs each hook, perhaps through a shell. The
//! hook also asks the host to agree, through `CLAUDE_PID`, which Claude Code
//! sets for each hook it runs. The bridge cannot ask: an MCP server keeps
//! whatever `CLAUDE_PID` it inherited, which inside a nested run is the outer
//! conversation's host.
//!
//! Anything that does not line up writes nothing or reads nothing, and the
//! bridge goes on sending the name it started with. The file holds a name, a
//! start time and when it was written; never a credential.

use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A host process as the system knows it: its id and the moment it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    pub pid: i32,
    pub started: (u64, u64),
}

/// What the system says of one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub ppid: i32,
    pub name: String,
    pub started: (u64, u64),
}

/// How far up a walk goes. A host is one or two steps away.
const MAX_STEPS: usize = 8;

/// How many entries one tidy looks at, so a crowded folder cannot slow a hook.
const MAX_TIDY: usize = 256;

/// The shells a host may run a command through. A login shell's name starts with a dash.
pub fn is_shell(name: &str) -> bool {
    matches!(name.trim_start_matches('-'), "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "fish" | "tcsh" | "csh")
}

/// The nearest process at or above `pid` that is not a shell.
pub fn host_above(mut pid: i32, look: impl Fn(i32) -> Option<Proc>) -> Option<Host> {
    for _ in 0..MAX_STEPS {
        if pid <= 1 {
            return None;
        }
        let p = look(pid)?;
        if !is_shell(&p.name) {
            return Some(Host { pid, started: p.started });
        }
        pid = p.ppid;
    }
    None
}

/// The host a hook writes for: the one above it, provided the host names
/// itself the same way. No name, or another one, is no host.
pub fn agreed(found: Option<Host>, claude_pid: Option<&str>) -> Option<Host> {
    let named = claude_pid?.trim().parse::<i32>().ok().filter(|p| *p > 1)?;
    found.filter(|h| h.pid == named)
}

fn parent_pid() -> i32 {
    // SAFETY: getppid is infallible and thread-safe.
    unsafe { libc::getppid() }
}

/// The bridge's host, read once at start: a server whose host has gone is
/// handed to another parent.
pub fn bridge_host() -> Option<Host> {
    host_above(parent_pid(), proc_info)
}

/// The host this hook runs for.
pub fn hook_host() -> Option<Host> {
    agreed(host_above(parent_pid(), proc_info), std::env::var("CLAUDE_PID").ok().as_deref())
}

#[cfg(target_os = "macos")]
pub fn proc_info(pid: i32) -> Option<Proc> {
    // SAFETY: proc_bsdinfo is plain data; all zeroes is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the buffer is a proc_bsdinfo of exactly the size passed.
    let n = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size) };
    if n != size {
        return None;
    }
    // The name fills its field when it is as long as the field, with no end mark.
    let name: Vec<u8> = info.pbi_comm.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    Some(Proc {
        ppid: info.pbi_ppid as i32,
        name: String::from_utf8_lossy(&name).into_owned(),
        started: (info.pbi_start_tvsec, info.pbi_start_tvusec),
    })
}

/// Linux reads `/proc`. The plugin ships for macOS only; this is what lets the
/// tests that run real processes run on a Linux machine too.
#[cfg(target_os = "linux")]
pub fn proc_info(pid: i32) -> Option<Proc> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (open, close) = (stat.find('(')?, stat.rfind(')')?);
    let name = stat.get(open + 1..close)?.to_string();
    // After the name: state, ppid, and the start time is the twentieth field.
    let rest: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
    Some(Proc { ppid: rest.get(1)?.parse().ok()?, name, started: (rest.get(19)?.parse().ok()?, 0) })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn proc_info(_pid: i32) -> Option<Proc> {
    None
}

/// `$HOME`, else the password database: the home the hooks use.
pub fn home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    // SAFETY: getpwuid returns a pointer into static storage or null; it is
    // read once, before any other call could overwrite it.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        Some(PathBuf::from(std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned()))
    }
}

pub fn dir(home: &Path) -> PathBuf {
    home.join(".designless").join("host-session")
}

fn file(home: &Path, pid: i32) -> PathBuf {
    dir(home).join(format!("{pid}.json"))
}

fn started_text(started: (u64, u64)) -> String {
    format!("{}.{:06}", started.0, started.1)
}

/// A name the bridge may send for a Claude Code conversation: `claude:` and
/// eight lowercase hex, the shape `identity::session_label` makes.
pub fn is_claude_name(s: &str) -> bool {
    s.strip_prefix("claude:")
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

/// Leave `name` for `host`'s bridge. Written whole or not at all: a reader
/// sees the old file or the new one, never part of one.
pub fn write(home: &Path, host: Host, name: &str, now_ms: u128) -> std::io::Result<()> {
    if !is_claude_name(name) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a conversation name"));
    }
    let d = dir(home);
    std::fs::create_dir_all(&d)?;
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700))?;
    let body = json!({ "version": 1, "started": started_text(host.started), "session": name, "at": now_ms as u64 }).to_string();
    let part = d.join(format!(".{}.{}.part", host.pid, std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&part)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .and_then(|_| std::fs::rename(&part, file(home, host.pid)));
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written
}

/// The name `host`'s hook left, when the file is that host's.
pub fn read(home: &Path, host: Host) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file(home, host.pid)).ok()?).ok()?;
    if v.get("version")?.as_u64()? != 1 || v.get("started")?.as_str()? != started_text(host.started) {
        return None;
    }
    let name = v.get("session")?.as_str()?;
    is_claude_name(name).then(|| name.to_string())
}

/// Remove what no live host can read: a file whose process has gone or
/// started at another moment, and a half-written file older than a minute.
/// Returns how many were removed.
pub fn tidy(home: &Path, look: impl Fn(i32) -> Option<Proc>, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir(home)) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten().take(MAX_TIDY) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = match name.strip_suffix(".json").and_then(|p| p.parse::<i32>().ok()) {
            Some(pid) => {
                let started = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|b| serde_json::from_str::<Value>(&b).ok())
                    .and_then(|v| v.get("started").and_then(Value::as_str).map(str::to_string));
                match look(pid) {
                    Some(p) => started.as_deref() != Some(started_text(p.started).as_str()),
                    None => true,
                }
            }
            None => entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age.as_secs() >= 60),
        };
        if stale && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Whether a session start moves the conversation on inside its host: a
/// clear or a resume. A first start and a compaction keep the id the host
/// started its servers with, so they leave nothing.
pub fn moves_on(source: Option<&str>) -> bool {
    matches!(source, Some("clear" | "resume"))
}

/// The session-start hook's part, given what it found: after a clear or a
/// resume, leave the conversation's name for the host; at every start, tidy.
/// Returns whether a name was left.
pub fn follow(home: &Path, source: Option<&str>, name: Option<String>, host: Option<Host>, now: SystemTime) -> bool {
    let left = match (moves_on(source), name.filter(|n| is_claude_name(n)), host) {
        (true, Some(name), Some(host)) => {
            let ms = now.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            write(home, host, &name, ms).is_ok()
        }
        _ => false,
    };
    tidy(home, proc_info, now);
    left
}

/// The session-start hook's part, reading this process's own facts. The name
/// is the one this process's conversation goes by in Claude Code; under
/// another host (Codex, Cursor) there is none and nothing is left.
pub fn on_session_start(home: &Path, source: Option<&str>, session_id: Option<&str>) -> bool {
    let name = if moves_on(source) { session_id.and_then(crate::identity::own_session_name) } else { None };
    let host = if name.is_some() { hook_host() } else { None };
    follow(home, source, name, host, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("host-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn table(rows: &[(i32, i32, &str, u64)]) -> HashMap<i32, Proc> {
        rows.iter().map(|&(pid, ppid, name, s)| (pid, Proc { ppid, name: name.into(), started: (s, 7) })).collect()
    }

    const NAME: &str = "claude:0a1b2c3d";
    const OTHER: &str = "claude:99887766";

    #[test]
    fn the_host_is_the_nearest_process_above_that_is_not_a_shell() {
        // The chain a nested run showed: a hook's shell, the nested host, the
        // terminal driver, a shell, the outer host.
        let t = table(&[(59745, 59724, "sh", 1), (59724, 59723, "claude", 2), (59723, 59249, "expect", 3), (59249, 28514, "zsh", 4), (28514, 28513, "claude", 5)]);
        let look = |p: i32| t.get(&p).cloned();
        assert_eq!(host_above(59745, look), Some(Host { pid: 59724, started: (2, 7) }), "the nested host, not the outer one");
        assert_eq!(host_above(59724, look), Some(Host { pid: 59724, started: (2, 7) }), "a server started by its host directly");
        assert_eq!(host_above(59249, look), Some(Host { pid: 28514, started: (5, 7) }));
        // Login shells, chains of shells, and walks that end nowhere.
        let t = table(&[(10, 9, "-zsh", 1), (9, 8, "bash", 1), (8, 7, "dash", 1), (7, 6, "node", 9)]);
        assert_eq!(host_above(10, |p| t.get(&p).cloned()), Some(Host { pid: 7, started: (9, 7) }));
        let shells = table(&[(5, 4, "sh", 1), (4, 1, "sh", 1)]);
        assert_eq!(host_above(5, |p| shells.get(&p).cloned()), None, "only shells up to launchd");
        assert_eq!(host_above(5, |_| None), None, "a process the system will not describe");
        assert_eq!(host_above(1, |p| t.get(&p).cloned()), None);
        let endless: HashMap<i32, Proc> = (2..100).map(|p| (p, Proc { ppid: p + 1, name: "sh".into(), started: (0, 0) })).collect();
        assert_eq!(host_above(2, |p| endless.get(&p).cloned()), None, "the walk stops");
    }

    #[test]
    fn a_hook_writes_only_for_the_host_that_names_itself() {
        let h = Some(Host { pid: 59724, started: (2, 7) });
        assert_eq!(agreed(h, Some("59724")), h);
        assert_eq!(agreed(h, Some(" 59724\n")), h);
        // Inside a nested run a server inherits the outer host's CLAUDE_PID.
        assert_eq!(agreed(h, Some("28514")), None);
        for bad in [None, Some(""), Some("1"), Some("0"), Some("-4"), Some("abc")] {
            assert_eq!(agreed(h, bad), None, "{bad:?}");
        }
        assert_eq!(agreed(None, Some("59724")), None);
    }

    #[test]
    fn a_name_left_for_a_host_is_read_by_that_host_and_no_other() {
        let home = scratch("rw");
        let host = Host { pid: 4242, started: (1_700_000_000, 123) };
        assert_eq!(read(&home, host), None, "nothing left yet");
        write(&home, host, NAME, 1).unwrap();
        assert_eq!(read(&home, host), Some(NAME.to_string()));
        write(&home, host, OTHER, 2).unwrap();
        assert_eq!(read(&home, host), Some(OTHER.to_string()), "the newest name wins");
        // The same id handed to a process that started at another moment.
        assert_eq!(read(&home, Host { pid: 4242, started: (1_700_000_000, 124) }), None);
        assert_eq!(read(&home, Host { pid: 4243, started: host.started }), None);
        // Owner only, and no half-written file left behind.
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir(&home)), 0o700);
        assert_eq!(mode(&dir(&home).join("4242.json")), 0o600);
        let names: Vec<String> = std::fs::read_dir(dir(&home)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["4242.json".to_string()]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn only_a_conversation_name_is_written_or_read() {
        let home = scratch("names");
        let host = Host { pid: 77, started: (5, 0) };
        for bad in ["", "claude:", "claude:0A1B2C3D", "claude:0a1b2c3", "claude:0a1b2c3d4", "codex:0a1b2c3d", "claude:0a1b2c3g", "x claude:0a1b2c3d"] {
            assert!(!is_claude_name(bad), "{bad:?}");
            assert!(write(&home, host, bad, 0).is_err(), "{bad:?}");
        }
        assert!(is_claude_name(NAME));
        let p = dir(&home).join("77.json");
        std::fs::create_dir_all(dir(&home)).unwrap();
        for body in [
            "".to_string(),
            "{broken".to_string(),
            r#"{"version":2,"started":"5.000000","session":"claude:0a1b2c3d"}"#.to_string(),
            r#"{"version":1,"started":"5.000001","session":"claude:0a1b2c3d"}"#.to_string(),
            r#"{"version":1,"started":"5.000000","session":"codex:0a1b2c3d"}"#.to_string(),
            r#"{"version":1,"started":"5.000000"}"#.to_string(),
        ] {
            std::fs::write(&p, &body).unwrap();
            assert_eq!(read(&home, host), None, "{body}");
        }
        std::fs::write(&p, r#"{"version":1,"started":"5.000000","session":"claude:0a1b2c3d","at":1}"#).unwrap();
        assert_eq!(read(&home, host), Some(NAME.to_string()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_clear_or_a_resume_leaves_a_name_and_nothing_else_does() {
        for (source, moves) in [(Some("clear"), true), (Some("resume"), true), (Some("startup"), false), (Some("compact"), false), (Some(""), false), (None, false)] {
            assert_eq!(moves_on(source), moves, "{source:?}");
        }
        let home = scratch("follow");
        // The host is this test process, so the tidy that follows keeps it.
        let me = proc_info(std::process::id() as i32).map(|p| Host { pid: std::process::id() as i32, started: p.started });
        let now = SystemTime::now();
        assert!(!follow(&home, Some("startup"), Some(NAME.into()), me, now));
        assert!(!follow(&home, Some("clear"), None, me, now), "no name: a hook under another host");
        assert!(!follow(&home, Some("clear"), Some(NAME.into()), None, now), "no agreed host");
        assert!(!follow(&home, Some("clear"), Some("codex:0a1b2c3d".into()), me, now));
        if let Some(host) = me {
            assert_eq!(read(&home, host), None);
            assert!(follow(&home, Some("clear"), Some(NAME.into()), me, now));
            assert_eq!(read(&home, host), Some(NAME.to_string()));
            assert!(follow(&home, Some("resume"), Some(OTHER.into()), me, now));
            assert_eq!(read(&home, host), Some(OTHER.to_string()));
            assert!(!follow(&home, Some("startup"), Some(NAME.into()), me, now));
            assert_eq!(read(&home, host), Some(OTHER.to_string()), "a later start of another kind leaves it alone");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_tidy_removes_what_no_live_host_can_read() {
        let home = scratch("tidy");
        let live = Host { pid: 100, started: (10, 0) };
        let restarted = Host { pid: 200, started: (20, 0) };
        let gone = Host { pid: 300, started: (30, 0) };
        for h in [live, restarted, gone] {
            write(&home, h, NAME, 0).unwrap();
        }
        let d = dir(&home);
        std::fs::write(d.join(".400.1.part"), "{").unwrap();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        // Pid 200 now belongs to a process that started later.
        let t = table(&[(100, 1, "claude", 10), (200, 1, "claude", 21)]);
        let look = |p: i32| t.get(&p).map(|x| Proc { started: (x.started.0, 0), ..x.clone() });
        assert_eq!(tidy(&home, look, SystemTime::now()), 2, "the restarted id and the gone host");
        assert_eq!(read(&home, live), Some(NAME.to_string()));
        assert!(!d.join("200.json").exists() && !d.join("300.json").exists());
        assert!(d.join(".400.1.part").exists(), "a fresh half-written file may still be in flight");
        let later = SystemTime::now() + std::time::Duration::from_secs(61);
        assert_eq!(tidy(&home, look, later), 2, "a minute on, the leftovers go");
        assert_eq!(read(&home, live), Some(NAME.to_string()));
        let nowhere = std::env::temp_dir().join(format!("host-session-nowhere-{}", std::process::id()));
        assert_eq!(tidy(&nowhere, look, later), 0, "no folder, nothing to do");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn this_process_is_described_by_the_system() {
        let me = proc_info(std::process::id() as i32).expect("the system describes a live process");
        // SAFETY: getppid is infallible.
        assert_eq!(me.ppid, unsafe { libc::getppid() });
        assert!(!me.name.is_empty());
        assert_eq!(proc_info(std::process::id() as i32).map(|p| p.started), Some(me.started), "the start time is stable");
    }
}
