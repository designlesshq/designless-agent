//! The plugin's hooks and its live watcher, as subcommands of this binary.
//!
//! They used to be scripts that needed Node, which no host guarantees: a
//! machine with only the host app installed failed every hook. Here they need
//! nothing beyond the binary the plugin already ships.
//!
//!   designless-mcp-bridge hook <name>          the host's event hooks
//!   designless-mcp-bridge inbox-watch <id>     the watcher the agent starts
//!
//! Behaviour is the scripts' own, file for file and line for line: the same
//! stdin handling, the same text on stdout, the same files under `~` in the
//! same shapes (other copies of the plugin may be reading them), and the same
//! fail-open rule: any error is silence and exit 0.

mod events;
mod js;
mod marker;
mod probe;
mod watch;

use std::io::{Read, Write};

/// The event hooks, by the names the host configuration uses.
pub const HOOKS: [&str; 5] = [
    "session-start-inbox",
    "canvas-wake",
    "canvas-drain-check",
    "canvas-arm-watch",
    "compose-epilogue",
];

/// What every hook and the watcher read from the machine.
pub struct Env {
    /// `os.homedir()`
    pub home: String,
    /// The command that starts the watcher, as the arm line prints it.
    pub watcher: String,
}

impl Env {
    fn detect() -> Option<Env> {
        Some(Env { home: js::home_dir()?, watcher: watcher_command() })
    }
}

/// `/bin/sh '<plugin>/bin/designless' inbox-watch`, located from this
/// executable, which lives beside the launcher.
fn watcher_command() -> String {
    let launcher = std::env::current_exe()
        .ok()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .and_then(|p| p.parent().map(|d| d.join("designless")))
        .map(|p| p.to_string_lossy().into_owned());
    match launcher {
        // A single-quoted path is one argument whatever it contains; a quote
        // inside it is closed, escaped and reopened.
        Some(p) => format!("/bin/sh '{}' inbox-watch", p.replace('\'', "'\\''")),
        None => "/bin/sh \"${CLAUDE_PLUGIN_ROOT}\"/bin/designless inbox-watch".into(),
    }
}

fn read_stdin() -> String {
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).into_owned()
}

fn runtime() -> Option<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().ok()
}

/// Run one hook. Always exits 0: a hook never blocks the turn, and an unknown
/// name or any failure is silence.
pub fn run_hook(name: Option<&str>) -> i32 {
    let outcome = std::panic::catch_unwind(|| {
        let name = name?;
        if !HOOKS.contains(&name) {
            return None;
        }
        let env = Env::detect()?;
        let raw = read_stdin();
        let rt = runtime()?;
        rt.block_on(async {
            match name {
                "session-start-inbox" => events::session_start(&raw, &env).await,
                "canvas-wake" => events::canvas_wake(&raw, &env).await,
                "canvas-drain-check" => events::drain_check(&raw, &env).await,
                "canvas-arm-watch" => events::arm_watch(&raw, &env).await,
                "compose-epilogue" => events::compose_epilogue(&raw, &env).await,
                _ => None,
            }
        })
    });
    if let Ok(Some(out)) = outcome {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(out.as_bytes());
        let _ = stdout.flush();
    }
    0
}

/// Run the watcher until it stands down, is stopped, or finds one running.
pub fn run_watch(session: Option<&str>) -> i32 {
    std::panic::catch_unwind(|| {
        let env = Env::detect()?;
        let rt = runtime()?;
        Some(rt.block_on(watch::run(session, &env)))
    })
    .ok()
    .flatten()
    .unwrap_or(0)
}
