//! The plugin runs with nothing but /bin/sh and the binary it ships.
//!
//! Two kinds of check live here. WIRING: every hook the binary implements is
//! registered with every host, inside `hooks`, with a matcher that matches the
//! names hosts actually deliver, and no shipped file asks for Node. And END TO
//! END: the built binary run as the host runs it, stdin in and stdout out,
//! against a stand-in desktop socket, so the probe's wire protocol is tested
//! rather than assumed.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HOOKS: [&str; 5] = ["session-start-inbox", "canvas-wake", "canvas-drain-check", "canvas-arm-watch", "compose-epilogue"];
const BIN: &str = env!("CARGO_BIN_EXE_designless-mcp-bridge");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}
fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap()
}

/// A minimal regex matcher for the host matchers: literals, `.`, `*`, `^`, `$`.
/// Enough to run the configured matcher against real names under both of the
/// semantics hosts use (substring search and full-string match).
fn re_here(p: &[char], t: &[char], full: bool) -> bool {
    if p.is_empty() {
        return !full || t.is_empty();
    }
    if p == ['$'] {
        return t.is_empty();
    }
    if p.len() >= 2 && p[1] == '*' {
        let mut i = 0;
        loop {
            if re_here(&p[2..], &t[i..], full) {
                return true;
            }
            if i < t.len() && (p[0] == '.' || p[0] == t[i]) {
                i += 1;
            } else {
                return false;
            }
        }
    }
    !t.is_empty() && (p[0] == '.' || p[0] == t[0]) && re_here(&p[1..], &t[1..], full)
}
fn re_search(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    if p.first() == Some(&'^') {
        return re_here(&p[1..], &t, false);
    }
    (0..=t.len()).any(|i| re_here(&p, &t[i..], false))
}
fn re_full(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.trim_start_matches('^').chars().collect();
    let t: Vec<char> = text.chars().collect();
    re_here(&p, &t, true)
}
fn matches_either_way(pat: &str, name: &str) -> bool {
    re_search(pat, name) && re_full(pat, name)
}
fn qualified(tool: &str) -> String {
    format!("mcp__plugin_designless_less-mcp__{tool}")
}

// ── wiring ──────────────────────────────────────────────────────────────────

#[test]
fn every_event_is_registered_inside_hooks_never_beside_it() {
    let cfg = json("hooks/hooks.json");
    let keys: Vec<&String> = cfg.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["hooks"], "a key outside `hooks` registers nothing");
    for (event, entries) in cfg["hooks"].as_object().unwrap() {
        assert!(entries.as_array().is_some_and(|a| !a.is_empty()), "{event} has no entries");
    }
}

#[test]
fn every_hook_the_binary_implements_is_reachable_from_an_event() {
    let cmds = serde_json::to_string(&json("hooks/hooks.json")["hooks"]).unwrap();
    for h in HOOKS {
        let cmd = format!(r#"/bin/sh \"${{CLAUDE_PLUGIN_ROOT}}\"/bin/designless hook {h}""#);
        assert!(cmds.contains(&cmd), "{h} ships but no event runs it as {cmd}");
    }
}

#[test]
fn the_cursor_hooks_run_from_the_plugin_root() {
    let cfg = json("hooks/cursor.hooks.json");
    let mut seen = Vec::new();
    for entries in cfg["hooks"].as_object().unwrap().values() {
        for e in entries.as_array().unwrap() {
            let c = e["command"].as_str().unwrap();
            let name = c.strip_prefix("/bin/sh ./bin/designless hook ").unwrap_or_else(|| panic!("{c}"));
            assert!(HOOKS.contains(&name), "{name}");
            seen.push(name.to_string());
        }
    }
    assert_eq!(cfg["hooks"]["stop"][0]["loop_limit"], 3);
    assert!(seen.len() >= 4);
}

fn post_entry(fragment: &str) -> Value {
    json("hooks/hooks.json")["hooks"]["PostToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["hooks"].to_string().contains(fragment))
        .cloned()
        .unwrap_or_else(|| panic!("{fragment} must be registered"))
}

#[test]
fn the_compose_epilogue_runs_after_a_compose_and_nothing_else() {
    let m = post_entry("hook compose-epilogue")["matcher"].as_str().unwrap().to_string();
    assert!(matches_either_way(&m, &qualified("less_canvas_compose")));
    assert!(!re_search(&m, &qualified("less_canvas_status")), "compose only");
    assert!(m.ends_with('$'), "anchored to compose on purpose");
}

#[test]
fn the_watcher_ask_runs_after_every_designless_tool() {
    let m = post_entry("hook canvas-arm-watch")["matcher"].as_str().unwrap().to_string();
    for t in ["less_canvas_compose", "less_canvas_status", "less_artefact_open", "less_list_templates", "less_resolve_brand"] {
        assert!(matches_either_way(&m, &qualified(t)), "{t}");
    }
    for other in ["Bash", "Read", "Edit"] {
        assert!(!re_full(&m, other), "must not fire on {other}");
    }
    assert!(!m.ends_with('$'));
}

#[test]
fn every_host_starts_the_server_with_sh() {
    let claude = json(".mcp.json");
    assert_eq!(claude["mcpServers"]["less-mcp"]["command"], "/bin/sh");
    assert_eq!(claude["mcpServers"]["less-mcp"]["args"], serde_json::json!(["${CLAUDE_PLUGIN_ROOT}/bin/designless"]));
    for f in [".mcp.codex.json", ".mcp.cursor.json"] {
        let c = json(f);
        assert_eq!(c["mcpServers"]["less-mcp"]["command"], "/bin/sh", "{f}");
        assert_eq!(c["mcpServers"]["less-mcp"]["args"], serde_json::json!(["./bin/designless"]), "{f}");
        assert_eq!(c["mcpServers"]["less-mcp"]["cwd"], ".", "{f}");
    }
}

#[test]
fn the_command_starts_the_watcher_through_the_launcher() {
    let md = read("commands/agent.md");
    assert!(md.contains("run `/bin/sh \"${CLAUDE_PLUGIN_ROOT}\"/bin/designless inbox-watch --once <this session's id>` as a background command described exactly \"Designless Agent\""));
    assert!(md.contains("Bash with `run_in_background`, not the Monitor tool"));
}

#[test]
fn no_shipped_file_asks_for_node() {
    let mut files = vec![".mcp.json".to_string(), ".mcp.codex.json".into(), ".mcp.cursor.json".into()];
    for dir in ["hooks", "commands", "skills", "agents"] {
        for e in walk(&root().join(dir)) {
            files.push(e.strip_prefix(root()).unwrap().to_string_lossy().into_owned());
        }
    }
    files.push("bin/designless".into());
    for f in files {
        let t = read(&f);
        for bad in ["\"node\"", "node \"", "node ./", "node ${", ".mjs"] {
            assert!(!t.contains(bad), "{f} still invokes Node ({bad})");
        }
    }
    for e in walk(&root().join("hooks")) {
        assert!(!e.to_string_lossy().ends_with(".mjs"), "{} ships but nothing runs it", e.display());
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn the_launcher_is_a_posix_sh_script() {
    let p = root().join("bin/designless");
    let mode = std::fs::metadata(&p).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111, "the launcher must be executable");
    assert!(read("bin/designless").starts_with("#!/bin/sh\n"));
    assert!(Command::new("/bin/sh").arg("-n").arg(&p).status().unwrap().success());
}

// ── end to end ──────────────────────────────────────────────────────────────

struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Scratch {
        // Under /tmp, not $TMPDIR: a socket path must stay under 104 bytes.
        let d = PathBuf::from(format!("/tmp/dlw-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        Scratch(d)
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A stand-in desktop: answers each connection's first line with `reply`.
fn desktop(sock: &str, reply: &'static str) {
    let listener = std::os::unix::net::UnixListener::bind(sock).unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut r = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            assert_eq!(line, "{\"op\":\"list_inbox\"}\n", "the probe's request frame");
            if !reply.is_empty() {
                let mut w = conn;
                let _ = w.write_all(reply.as_bytes());
            } else {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    });
}

fn run(args: &[&str], stdin: &str, home: &str, sock: &str) -> (i32, String, String) {
    let mut child = Command::new(BIN)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DESIGNLESS_IPC_SOCKET", sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

fn context(out: &str) -> String {
    let v: Value = serde_json::from_str(out).unwrap_or_else(|_| panic!("not JSON: {out}"));
    v["hookSpecificOutput"]["additionalContext"].as_str().unwrap().to_string()
}

#[test]
fn the_wake_reads_the_desktop_over_its_socket() {
    let s = Scratch::new("wake");
    let sock = s.path("ipc.sock");
    desktop(&sock, "{\"op\":\"inbox\",\"sessions\":[{\"session_id\":\"a\",\"n_artefact\":2,\"title\":\"Deck\"}],\"attn_dark\":null}\n");
    let input = format!(r#"{{"cwd":"{}","session_id":"sess-x"}}"#, s.path(""));
    let (code, out, err) = run(&["hook", "canvas-wake"], &input, &s.path("home"), &sock);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.starts_with(r#"{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"Designless canvas: 2 edit(s) are waiting on the Designless app for \"Deck\"."#), "{out}");
    // The prompt was stamped as presence, in the file every watcher reads.
    let stamp = std::fs::read_to_string(s.path("home/.designless/watch/host-activity.json")).unwrap();
    assert!(stamp.starts_with("{\"at\":") && stamp.ends_with('}'));
}

#[test]
fn a_refusal_is_reported_as_unknown_never_as_empty() {
    let s = Scratch::new("refuse");
    let sock = s.path("ipc.sock");
    desktop(&sock, "{\"op\":\"no_session_stale\"}\n");
    let (code, out, _) = run(&["hook", "canvas-wake"], r#"{"cwd":"/tmp","session_id":"z"}"#, &s.path("home"), &sock);
    assert_eq!(code, 0);
    assert!(context(&out).starts_with("Designless canvas: the quick inbox check did not answer (desktop replied no_session_stale). Not an all-clear"));
}

#[test]
fn a_slow_desktop_is_a_timeout_not_an_all_clear() {
    let s = Scratch::new("slow");
    let sock = s.path("ipc.sock");
    desktop(&sock, "");
    let t = std::time::Instant::now();
    let (code, out, _) = run(&["hook", "session-start-inbox"], r#"{"cwd":"/tmp","session_id":"z"}"#, &s.path("home"), &sock);
    assert_eq!(code, 0);
    assert!(context(&out).contains("did not answer (timeout after 700ms)"));
    assert!(t.elapsed() < std::time::Duration::from_secs(2), "the budget bounds the hook");
}

#[test]
fn no_desktop_is_silence_or_the_register_and_always_exit_zero() {
    let s = Scratch::new("none");
    let sock = s.path("absent.sock");
    let home = s.path("home");
    let (code, out, _) = run(&["hook", "session-start-inbox"], "", &home, &sock);
    assert_eq!(code, 0);
    assert!(context(&out).starts_with("Designless register, scoped to Designless work only"));
    for (hook, input) in [
        ("canvas-wake", r#"{"cwd":"/tmp"}"#),
        ("canvas-drain-check", r#"{"cwd":"/tmp","stop_hook_active":true}"#),
        ("canvas-drain-check", "not json"),
        ("compose-epilogue", r#"{"tool_name":"x"}"#),
        ("canvas-arm-watch", "null"),
        ("no-such-hook", "{}"),
    ] {
        assert_eq!(run(&["hook", hook], input, &home, &sock), (0, String::new(), String::new()), "{hook}");
    }
}

#[test]
fn the_arm_line_names_the_launcher_beside_the_binary() {
    let s = Scratch::new("arm");
    let input = r#"{"tool_name":"mcp__plugin_designless_less-mcp__less_canvas_status","session_id":"abc-123","tool_response":"ok"}"#;
    let (code, out, _) = run(&["hook", "canvas-arm-watch"], input, &s.path("home"), &s.path("absent.sock"));
    assert_eq!(code, 0);
    let dir = Path::new(BIN).canonicalize().unwrap().parent().unwrap().join("designless");
    assert!(context(&out).contains(&format!("): /bin/sh '{}' inbox-watch --once abc-123.", dir.display())), "{out}");
    // Once a turn: the same session's next Designless call is not asked again
    // until the user types.
    let (_, again, _) = run(&["hook", "canvas-arm-watch"], input, &s.path("home"), &s.path("absent.sock"));
    assert_eq!(again, "");
    run(&["hook", "canvas-wake"], r#"{"cwd":"/tmp","session_id":"abc-123"}"#, &s.path("home"), &s.path("absent.sock"));
    let (_, next_turn, _) = run(&["hook", "canvas-arm-watch"], input, &s.path("home"), &s.path("absent.sock"));
    assert!(context(&next_turn).contains("inbox-watch --once abc-123."), "{next_turn}");
}

#[test]
fn the_watcher_refuses_without_an_id_and_refuses_a_second_copy() {
    let s = Scratch::new("watch");
    let home = s.path("home");
    let none = run(&["inbox-watch"], "", &home, &s.path("absent.sock"));
    assert_eq!(none, (0, "Designless watcher: no session id was passed, so it did not start.\n".into(), String::new()));
    // A live marker for the session: a second watcher finds it and exits.
    std::fs::create_dir_all(format!("{home}/.designless/watch")).unwrap();
    // A beat from the future reads as fresh, so the marker holds.
    let iso = "2999-01-01T00:00:00.000Z";
    std::fs::write(format!("{home}/.designless/watch/held.json"), format!(r#"{{"pid":1,"started_at":"{iso}","beat_at":"{iso}"}}"#)).unwrap();
    let held = run(&["inbox-watch", "held"], "", &home, &s.path("absent.sock"));
    assert_eq!(held, (0, "Designless watcher: one is already running for this session.\n".into(), String::new()));
}

#[test]
fn the_watcher_speaks_on_news_and_frees_the_session_when_stopped() {
    let s = Scratch::new("live");
    let sock = s.path("ipc.sock");
    desktop(&sock, "{\"op\":\"inbox\",\"sessions\":[{\"session_id\":\"a\",\"n_annotation\":1}]}\n");
    let home = s.path("home");
    let mut child = Command::new(BIN)
        .args(["inbox-watch", "live-1"])
        .env_clear()
        .env("HOME", &home)
        .env("DESIGNLESS_IPC_SOCKET", &sock)
        .current_dir("/tmp")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    assert!(line.starts_with("Designless canvas: 1 annotation(s) are waiting."), "{line}");
    let marker = format!("{home}/.designless/watch/live-1.json");
    assert!(Path::new(&marker).exists(), "the watcher holds the session");
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(!Path::new(&marker).exists(), "a stopped watcher frees the session");
}

/// A watcher process, its stdout and stderr gathered in the background.
struct Watcher {
    child: std::process::Child,
    out: std::sync::mpsc::Receiver<String>,
}
impl Watcher {
    fn start(args: &[&str], home: &str, sock: &str) -> Watcher {
        let mut child = Command::new(BIN)
            .args(args)
            .env_clear()
            .env("HOME", home)
            .env("DESIGNLESS_IPC_SOCKET", sock)
            .current_dir("/tmp")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, out) = std::sync::mpsc::channel();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        Watcher { child, out }
    }
    /// The first line within `secs`, if any.
    fn line_within(&self, secs: u64) -> Option<String> {
        self.out.recv_timeout(std::time::Duration::from_secs(secs)).ok()
    }
    /// Stop it and hand back what it wrote to stderr.
    fn stop(mut self) -> (Option<i32>, String) {
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let status = self.child.wait().unwrap();
        let mut err = String::new();
        let _ = self.child.stderr.take().unwrap().read_to_string(&mut err);
        (status.code(), err)
    }
}

#[test]
fn the_one_shot_prints_once_exits_and_frees_the_session() {
    let s = Scratch::new("once");
    let sock = s.path("ipc.sock");
    desktop(&sock, "{\"op\":\"inbox\",\"sessions\":[{\"session_id\":\"a\",\"n_artefact\":1,\"n_undelivered\":1,\"title\":\"Deck\"}]}\n");
    let home = s.path("home");
    let mut w = Watcher::start(&["inbox-watch", "--once", "once-1"], &home, &sock);
    let line = w.line_within(5).expect("the news");
    assert!(line.starts_with("Designless canvas: 1 edit(s) are waiting on the Designless app for \"Deck\"."), "{line}");
    assert!(line.ends_with("inbox-watch --once once-1"), "{line}");
    assert!(!line.contains("less_stream") && !line.contains("wait_seconds"), "{line}");
    let status = w.child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "the exit is the news");
    assert!(w.line_within(1).is_none(), "one line, then nothing");
    assert!(!Path::new(&format!("{home}/.designless/watch/once-1.json")).exists(), "an exited watcher frees the session");
    let seen = std::fs::read_to_string(format!("{home}/.designless/watch/once-1.seen.json")).unwrap();
    assert!(seen.contains(r#""sessions":[["a",1,0]]"#), "{seen}");
}

#[test]
fn the_one_shot_does_not_wake_for_what_it_was_already_told() {
    let s = Scratch::new("told");
    let sock = s.path("ipc.sock");
    desktop(&sock, "{\"op\":\"inbox\",\"sessions\":[{\"session_id\":\"a\",\"n_artefact\":1,\"n_undelivered\":1,\"title\":\"Deck\"}]}\n");
    let home = s.path("home");
    // The turn-boundary hook told the agent about this edit...
    let (_, out, _) = run(&["hook", "canvas-wake"], r#"{"cwd":"/tmp","session_id":"told-1"}"#, &home, &sock);
    assert!(context(&out).contains("\"Deck\""));
    // ...so a watcher started after it waits for more.
    let w = Watcher::start(&["inbox-watch", "--once", "told-1"], &home, &sock);
    assert_eq!(w.line_within(3), None);
    assert_eq!(w.stop().0, Some(0));
}

#[test]
fn edits_in_an_agents_hands_and_a_blind_desktop_wake_nobody() {
    let s = Scratch::new("quiet");
    let home = s.path("home");
    // An expired canvas whose edits an agent took and never finished.
    let held = s.path("held.sock");
    desktop(&held, "{\"op\":\"inbox\",\"sessions\":[{\"session_id\":\"old\",\"recoverable\":true,\"n_artefact\":5,\"n_undelivered\":0,\"title\":\"Old badge\"}]}\n");
    let w = Watcher::start(&["inbox-watch", "--once", "quiet-1"], &home, &held);
    assert_eq!(w.line_within(3), None, "nobody can take those edits, so they are not news");
    assert_eq!(w.stop().0, Some(0));
    // A desktop that holds the request past the budget.
    let slow = s.path("slow.sock");
    desktop(&slow, "");
    let w = Watcher::start(&["inbox-watch", "quiet-2"], &home, &slow);
    assert_eq!(w.line_within(3), None, "a watcher that cannot see says nothing to the agent");
    let (code, err) = w.stop();
    assert_eq!(code, Some(0));
    assert!(err.contains("the quick check did not answer (timeout after 700ms); still watching"), "{err}");
}

/// The launcher with this build beside it, in a scratch plugin tree.
fn scratch_plugin(s: &Scratch, sysctl: Option<&str>) -> String {
    let bin = s.path("plugin/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let mut launcher = read("bin/designless");
    if let Some(mock) = sysctl {
        let p = s.path("sysctl");
        std::fs::write(&p, format!("#!/bin/sh\necho {mock}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        launcher = launcher.replace("/usr/sbin/sysctl", &p);
    }
    std::fs::write(format!("{bin}/designless"), launcher).unwrap();
    std::fs::copy(BIN, format!("{bin}/designless-mcp-bridge-darwin-arm64")).unwrap();
    format!("{bin}/designless")
}

fn sh(launcher: &str, args: &[&str], home: &str) -> (i32, String, String) {
    let out = Command::new("/bin/sh")
        .arg(launcher)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DESIGNLESS_IPC_SOCKET", "/tmp/dlw-no-such-dir/ipc.sock")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

#[test]
fn an_unsupported_machine_is_told_so_and_its_hooks_stay_silent() {
    let s = Scratch::new("unsup");
    let l = scratch_plugin(&s, Some("0"));
    let (code, out, err) = sh(&l, &[], &s.path("home"));
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("Designless MCP bridge: this platform ("), "{err}");
    assert!(err.contains(") is not supported.\nThe Designless plugin and the Designless app run on Apple Silicon Macs only.\n"));
    assert_eq!(sh(&l, &["hook", "canvas-wake"], &s.path("home")), (0, String::new(), String::new()));
}

#[test]
fn a_missing_binary_is_named() {
    let s = Scratch::new("missing");
    let l = scratch_plugin(&s, Some("1"));
    std::fs::remove_file(s.path("plugin/bin/designless-mcp-bridge-darwin-arm64")).unwrap();
    let (code, _, err) = sh(&l, &[], &s.path("home"));
    if cfg!(target_os = "macos") {
        assert_eq!(code, 1);
        assert!(err.starts_with(&format!("Designless MCP bridge: binary not found at {}/plugin/bin/designless-mcp-bridge-darwin-arm64.\n", std::fs::canonicalize(&s.0).unwrap().display())), "{err}");
    } else {
        assert_eq!(code, 1, "elsewhere the platform check answers first");
    }
}

#[test]
fn the_launcher_runs_a_hook_on_a_supported_mac() {
    if !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")) {
        return;
    }
    let s = Scratch::new("ok");
    let l = scratch_plugin(&s, None);
    let (code, out, _) = sh(&l, &["hook", "session-start-inbox"], &s.path("home"));
    assert_eq!(code, 0);
    assert!(context(&out).starts_with("Designless register"));
}
