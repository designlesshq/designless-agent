//! The live canvas watcher: the stretch between turns that hooks cannot reach.
//!
//! Every line it prints on stdout wakes the agent, and a wake re-reads the
//! agent's whole conversation, so it prints ONLY when the user has given the
//! agent more to do than it was last told about. A request the server holds
//! for another session is not this agent's to do until its window passes.
//! Nothing else reaches stdout:
//! a desktop that cannot answer is written to stderr, where it is kept for
//! anyone looking and wakes nobody. One per session, enforced by the marker it
//! claims on start.
//!
//! Two ways to run it:
//!
//!   `inbox-watch --once <host-session-id>`  waits, prints once, exits. For a
//!       background command that reports when it exits: an idle canvas costs
//!       nothing however long it stays idle, and the exit IS the news.
//!   `inbox-watch <host-session-id>`          prints each time, never exits on
//!       its own. For a monitor that streams lines; the host caps such a watch
//!       and its expiry notice is a wake of its own.

use super::js::{self, J};
use super::marker::{self, BEAT_MS};
use super::probe::{self, Me, Probe, MISSED_HINT};
use super::Env;
use std::io::Write;
use std::sync::OnceLock;

/// The idle ladder, in beats: every beat under five minutes quiet, every 4th
/// under thirty, every 20th beyond. Any news resets it to the top.
const QUIET_LADDER: [(f64, u64); 2] = [(30.0 * 60_000.0, 20), (5.0 * 60_000.0, 4)];

/// The slowest the ladder may go while a person is at the machine: the middle
/// rung, once a minute.
const ACTIVE_CAP_BEATS: u64 = 4;

/// Polls in a row that must answer before blindness counts as news again, so
/// an accelerator that flaps is logged once, not once per cycle.
const HEALTHY_STREAK: u64 = 3;

/// How long one check may wait for the desktop. The hooks' 700 ms budget is
/// theirs: they run on every prompt and must not hold up the turn. The watcher
/// blocks nothing, so it waits as long as the desktop itself can take, which
/// bounds an inbox read at about 8 s (4 s to get the sign-in, 4 s for the
/// server) and then answers with the inbox or a refusal. A watcher held to
/// 700 ms read a desktop answering in 0.9 s as blind on every check and heard
/// nothing (measured 2026-10-09, on the release that shipped the one-shot).
/// It stays under one beat, so a slow answer never doubles a check.
pub const WATCH_BUDGET_MS: u64 = 10_000;
// Past the longest the desktop takes to answer, and inside one beat.
const _: () = assert!(WATCH_BUDGET_MS >= 8_000 && WATCH_BUDGET_MS < BEAT_MS);

/// How long the desktop may stay unanswering before a streaming watcher stands
/// down. Half an hour means the app is closed or the machine asleep; nobody is
/// editing, and the turn-boundary hook covers the person's return. A one-shot
/// watcher never stands down: its exit would wake the agent with nothing to do.
const STAND_DOWN_MS: f64 = 30.0 * 60_000.0;

pub fn poll_every_beats(quiet_ms: f64, active: bool) -> u64 {
    let every = QUIET_LADDER.iter().find(|(after, _)| quiet_ms >= *after).map(|(_, e)| *e).unwrap_or(1);
    // Presence caps the backoff; it never accelerates past what quiet earned.
    if active {
        every.min(ACTIVE_CAP_BEATS)
    } else {
        every
    }
}

/// Ask this beat? `active` is a parameter, never a file read from in here, so
/// this stays pure and its tests do not depend on who is using the machine.
pub fn should_poll(quiet_beats: u64, beat_ms: u64, active: bool) -> bool {
    quiet_beats % poll_every_beats((quiet_beats * beat_ms) as f64, active) == 0
}

/// What one canvas holds for this agent: edits nobody has taken, and notes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Work {
    /// Edits waiting for an agent to take them. An edit an agent has already
    /// taken is in its hands, often this agent's: counting it woke an agent
    /// for its own progress, and for edits an expired session's agent took and
    /// never finished. This is the count the server's own wait wakes on, so
    /// the watcher and the inbox agree on what is waiting. A request for a
    /// judgment that the server holds for another session counts once its
    /// window has passed, and not before.
    pub untaken: f64,
    /// Notes left on the canvas. Reading one does not take it, so a note in
    /// hand looks like a new one; only MORE notes than were seen are news.
    pub notes: f64,
    /// The canvas's newest edit number. Every new edit advances it and
    /// applying one does not, so it tells a new edit from an old one when the
    /// counts alone cannot: the agent applies what it was told while no
    /// watcher runs, and the next edit brings the count back to where it was.
    /// `None` from a server that does not say.
    pub seq: Option<f64>,
}

/// A number the row may or may not say.
fn said(v: Option<&J>) -> Option<f64> {
    if js::nullish(v) {
        None
    } else {
        Some(js::to_number(v)).filter(|v| v.is_finite())
    }
}

/// Per canvas, what it holds for this session, as a value that changes only
/// when that work does. Canvases holding nothing for it are absent: items only
/// the user can act on, edits already in an agent's hands, and requests held
/// for another session.
pub fn work_of(sessions: &[J], me: &Me) -> Vec<(String, Work)> {
    let mut rows: Vec<(String, Work)> = sessions
        .iter()
        .filter_map(|s| {
            let n = |k: &str| js::num_or_zero(js::get(Some(s), k));
            let (untaken, seq) = match probe::pending_requests(s) {
                // A newer server counts value edits apart from requests. Only
                // the requests this session may take are its work, and only
                // theirs can make the newest number newer: one held for
                // another session is neither work nor news here until its
                // window passes, and then it is both.
                Some(requests) => {
                    let mine: Vec<&J> = requests.iter().filter(|r| probe::open_to(r, me)).collect();
                    let seq = mine
                        .iter()
                        .map(|r| said(js::get(Some(r), "seq")))
                        .fold(said(js::get(Some(s), "latest_value_seq")), |a, b| match (a, b) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            (a, b) => a.or(b),
                        });
                    (n("n_value_undelivered") + mine.len() as f64, seq)
                }
                None => {
                    let undelivered = js::get(Some(s), "n_undelivered");
                    // A row from an older server does not say, and counts every edit
                    // as untaken, as the server's wait does for the same row.
                    let untaken = if js::nullish(undelivered) { n("n_page") + n("n_artefact") } else { js::num_or_zero(undelivered) };
                    (untaken, said(js::get(Some(s), "latest_seq")))
                }
            };
            let w = Work { untaken, notes: n("n_annotation"), seq };
            // NaN counts as nothing.
            if (w.untaken + w.notes).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return None;
            }
            let id = js::get(Some(s), "session_id");
            Some((if js::nullish(id) { String::new() } else { js::to_string(id) }, w))
        })
        .collect();
    rows.sort_by(|a, b| js::utf16_cmp(&a.0, &b.0));
    rows
}

/// The canvases holding work the agent has not been told about: more of it
/// than `before` said, or, at the same count, a newer edit. Less is never
/// news: it is the agent's own work landing.
pub fn news(before: &[(String, Work)], now: &[(String, Work)]) -> Vec<String> {
    now.iter()
        .filter(|(id, w)| {
            let was = before.iter().find(|(b, _)| b == id).map(|(_, w)| w.clone()).unwrap_or_default();
            let newer = matches!((w.seq, was.seq), (Some(s), Some(b)) if s > b);
            w.untaken > was.untaken || w.notes > was.notes || newer
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// `work_of` as the shape `marker::write_seen` keeps on disk.
pub fn seen_json(work: &[(String, Work)]) -> J {
    J::Arr(
        work.iter()
            .map(|(id, w)| J::Arr(vec![J::Str(id.clone()), J::Num(w.untaken), J::Num(w.notes), w.seq.map(J::Num).unwrap_or(J::Null)]))
            .collect(),
    )
}

/// The shape on disk read back. Anything unreadable is nothing seen.
pub fn seen_from_json(v: Option<&J>) -> Vec<(String, Work)> {
    let Some(J::Arr(rows)) = v else { return Vec::new() };
    rows.iter()
        .filter_map(|r| match r {
            J::Arr(f) if f.len() == 3 || f.len() == 4 => Some((
                js::to_string(f.first()),
                Work {
                    untaken: js::num_or_zero(f.get(1)),
                    notes: js::num_or_zero(f.get(2)),
                    seq: match f.get(3) {
                        Some(J::Num(n)) if n.is_finite() => Some(*n),
                        _ => None,
                    },
                },
            )),
            _ => None,
        })
        .collect()
}

/// The state carried from one poll to the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The last answer the desktop gave, or what the agent was last told.
    pub seen: Vec<(String, Work)>,
    pub healthy: u64,
    pub blind: bool,
    pub blind_since: Option<f64>,
    /// This session's own name, when its host says it: whose requests are
    /// this watcher's work. Fixed for the watcher's life.
    pub name: Option<String>,
}

/// What one poll says, and to whom.
#[derive(Clone, Debug, PartialEq)]
pub enum Voice {
    Silent,
    /// For the log only (stderr): wakes nobody.
    Log(String),
    /// For the agent (stdout): a wake.
    Wake(String),
}

/// One poll's decision, pure: what to say and the next state.
pub fn step(probe: &Probe, prev: &State, cwd: &str, now: f64) -> (Voice, State) {
    if let Some(unknown) = &probe.unknown {
        let blind_since = prev.blind_since.or(Some(now));
        // Latched on the FACT, not the reason: an unreachable desktop does not
        // fail the same way twice, so a reason-keyed latch spoke every poll.
        // What was seen is kept: nothing new was learned.
        if prev.blind {
            return (Voice::Silent, State { blind_since, healthy: 0, ..prev.clone() });
        }
        // Not a wake. The watcher is the one that could not see, and it keeps
        // looking; the hooks still read the inbox the moment the user types.
        return (
            Voice::Log(format!("Designless watcher: the quick check did not answer ({unknown}); still watching. {MISSED_HINT}")),
            State { blind: true, blind_since, healthy: 0, ..prev.clone() },
        );
    }
    let healthy = prev.healthy + 1;
    let recovered = healthy >= HEALTHY_STREAK;
    let me = Me { name: prev.name.clone(), now };
    let work = work_of(&probe.sessions, &me);
    let fresh = news(&prev.seen, &work);
    let next = State {
        seen: work,
        healthy,
        blind: if recovered { false } else { prev.blind },
        blind_since: if recovered { None } else { prev.blind_since },
        name: prev.name.clone(),
    };
    if fresh.is_empty() {
        return (Voice::Silent, next);
    }
    // The slice: only the canvases with news, so a line never repeats what the
    // agent was told before or counts what is already in an agent's hands.
    let slice: Vec<J> = probe
        .sessions
        .iter()
        .filter(|s| {
            let id = js::get(Some(s), "session_id");
            fresh.contains(&if js::nullish(id) { String::new() } else { js::to_string(id) })
        })
        .cloned()
        .collect();
    let opts = probe::Opts { include_attention: false, attn_dark: None, watcher_live: true, me };
    let text = probe::summarize_inbox(&slice, cwd, opts);
    if text.is_empty() {
        return (Voice::Silent, next);
    }
    (Voice::Wake(format!("Designless canvas: {text}")), next)
}

/// Has the desktop been unanswering long enough that watching is pointless?
/// Asks whether it is blind NOW, never whether it ever was.
pub fn should_stand_down(state: &State, now: f64, after_ms: f64) -> bool {
    match state.blind_since {
        Some(t) if state.blind && t != 0.0 => now - t >= after_ms,
        _ => false,
    }
}

/// The one-shot's last words: the news, and how to keep watching after it.
pub fn once_line(wake: &str, watcher: &str, session: &str) -> String {
    format!(
        "{wake} Once they are applied, start the watcher again the same way, as a background command labelled \
\"Designless Agent\": {watcher} --once {session}"
    )
}

/// The command line after the subcommand: `--once` anywhere, and the first
/// other word is the session.
pub fn parse_args(args: &[String]) -> (Option<&str>, bool) {
    let once = args.iter().any(|a| a == "--once");
    (args.iter().map(String::as_str).find(|a| !a.starts_with("--")), once)
}

/// Has the process that started the watcher gone? A watcher started already
/// orphaned (parent 1) has no such signal and keeps going.
pub fn left_behind(started_under: i32, now_under: i32) -> bool {
    started_under != 1 && now_under != started_under
}

/// The host the watcher belongs to, when the host names itself: Claude Code
/// sets `CLAUDE_PID` for everything it starts. The shell that started a
/// watcher can outlive the host (re-parented, not killed), so the parent
/// check alone misses a host that is gone. Other hosts say nothing here.
pub fn host_pid(value: Option<&str>) -> Option<i32> {
    value.and_then(|v| v.trim().parse::<i32>().ok()).filter(|pid| *pid > 1)
}

/// Is that host gone? Only a definite "no such process" counts: a process we
/// may not signal is still alive, and no host named is never gone.
pub fn host_gone(pid: Option<i32>) -> bool {
    let Some(pid) = pid else { return false };
    // SAFETY: signal 0 checks existence and delivers nothing.
    let rc = unsafe { libc::kill(pid, 0) };
    rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

static MARKER: OnceLock<std::ffi::CString> = OnceLock::new();

extern "C" fn on_signal(_: libc::c_int) {
    // Only async-signal-safe calls in here: unlink and _exit.
    if let Some(p) = MARKER.get() {
        unsafe {
            libc::unlink(p.as_ptr());
        }
    }
    unsafe { libc::_exit(0) }
}

fn say(line: &str) -> bool {
    let mut out = std::io::stdout().lock();
    out.write_all(line.as_bytes()).and_then(|_| out.flush()).is_ok()
}

/// stderr, best effort: a log line that cannot be written is not a reason to
/// stop watching.
fn log(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(format!("{} {line}\n", js::iso_string(js::now_ms())).as_bytes()).and_then(|_| err.flush());
}

pub async fn run(args: &[String], env: &Env) -> i32 {
    let (session, once) = parse_args(args);
    let Some(sid_text) = session.filter(|s| !s.is_empty()) else {
        say("Designless watcher: no session id was passed, so it did not start.\n");
        return 0;
    };
    let sid = J::Str(sid_text.to_string());
    if !marker::arm(&env.home, &sid, js::now_ms()) {
        // Already covered. A second watcher would double every wake.
        say("Designless watcher: one is already running for this session.\n");
        return 0;
    }
    let cwd = js::process_cwd().unwrap_or_default();
    if let Ok(c) = std::ffi::CString::new(marker::marker_path(&env.home, &sid)) {
        let _ = MARKER.set(c);
    }
    for sig in [libc::SIGINT, libc::SIGTERM] {
        // SAFETY: the handler only calls async-signal-safe functions.
        unsafe {
            libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }
    // The process that started this one. When it is gone the session that
    // wanted the watch is gone too, and nobody is left to hear it.
    // SAFETY: getppid is infallible and thread-safe.
    let parent = unsafe { libc::getppid() };
    let orphaned = || left_behind(parent, unsafe { libc::getppid() });
    // The host itself, when it names itself: checked before each look, so a
    // watcher whose host is gone stops without another read of the desktop.
    let host = host_pid(std::env::var("CLAUDE_PID").ok().as_deref());
    let stop = || {
        marker::disarm(&env.home, &sid);
        0
    };

    // Start from what this session was last told, by a hook or by the watcher
    // before this one, so the same work never wakes the agent twice. Its own
    // name says which held requests are its work; with none, only open ones are.
    let mut state = State {
        seen: seen_from_json(marker::read_seen(&env.home, &sid).as_ref()),
        name: crate::identity::own_session_name(sid_text),
        ..Default::default()
    };
    // Beats since anything was news. Drives the ladder, and nothing else.
    let mut quiet_beats: u64 = 0;
    loop {
        let beat_started = std::time::Instant::now();
        // Nobody is left to wake: leave quietly and free the session.
        if host_gone(host) {
            return stop();
        }
        // Presence is read on every beat, so a return to the machine is
        // answered on the next one.
        let active = marker::host_is_active(&env.home, js::now_ms(), marker::ACTIVE_WINDOW_MS);
        if should_poll(quiet_beats, BEAT_MS, active) {
            let probe = probe::probe_inbox_within(WATCH_BUDGET_MS).await;
            let (voice, next) = step(&probe, &state, &cwd, js::now_ms());
            if probe.unknown.is_none() && next.seen != state.seen {
                marker::write_seen(&env.home, &sid, seen_json(&next.seen), js::now_ms());
            }
            state = next;
            match voice {
                Voice::Wake(line) => {
                    let line = if once { once_line(&line, &env.watcher, sid_text) } else { line };
                    if !say(&format!("{line}\n")) || once {
                        return stop();
                    }
                    quiet_beats = 0;
                }
                Voice::Log(line) => {
                    log(&line);
                    quiet_beats += 1;
                }
                Voice::Silent => quiet_beats += 1,
            }
            // In silence. A one-shot never stands down: its exit is a wake.
            if !once && should_stand_down(&state, js::now_ms(), STAND_DOWN_MS) {
                return stop();
            }
        } else {
            quiet_beats += 1;
        }
        if orphaned() {
            return stop();
        }
        // The beat never slows: a stale marker frees the session to a second
        // watcher, and a crash that stops beating frees it instead of locking it.
        marker::beat(&env.home, &sid, js::now_ms());
        // A slow check spends part of the beat; the rest is slept, so the
        // cadence holds whatever the desktop takes.
        let beat = std::time::Duration::from_millis(BEAT_MS);
        tokio::time::sleep(beat.saturating_sub(beat_started.elapsed())).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: f64 = 60_000.0;
    fn cwd() -> String {
        js::process_cwd().unwrap()
    }
    fn sessions(v: &str) -> Probe {
        Probe { sessions: js::parse(v).map(|j| if let J::Arr(a) = j { a } else { vec![] }).unwrap(), ..Default::default() }
    }
    fn blind(r: &str) -> Probe {
        Probe { unknown: Some(r.into()), ..Default::default() }
    }
    /// The work as a session with no name sees it, now. Every row pinned here
    /// without requests is from a server that holds none, so anyone reads it
    /// the same way.
    fn work_of(s: &[J]) -> Vec<(String, Work)> {
        super::work_of(s, &Me::default())
    }

    // ── requests held for a session ──
    const ME: &str = "claude:0a1b2c3d";
    const OTHER: &str = "codex:9f8e7d6c";
    /// A fixed now, and a request window that ends five minutes after it.
    const T: f64 = 1_800_000_000_000.0;
    const SOON: Option<f64> = Some(T + 5.0 * MIN);

    /// A row from a server that holds requests: `value` value edits waiting,
    /// the newest numbered `value_seq`, and the requests as given. The older
    /// fields count everything, as that server sends them, so a reader that
    /// fell back to them would count the held requests too.
    fn held_row(value: u32, value_seq: Option<u32>, requests: &[(u32, Option<&str>, Option<f64>)]) -> Probe {
        let quoted = |v: Option<String>| v.map(|v| format!("\"{v}\"")).unwrap_or_else(|| "null".into());
        let list: Vec<String> = requests
            .iter()
            .map(|(seq, to, until)| {
                format!(
                    r#"{{"seq":{seq},"kind":"fit_slide","offered_to":{},"offered_until":{}}}"#,
                    quoted(to.map(str::to_string)),
                    quoted(until.map(js::iso_string))
                )
            })
            .collect();
        let all = value + requests.len() as u32;
        let latest = requests.iter().map(|r| r.0).chain(value_seq).max();
        sessions(&format!(
            r#"[{{"session_id":"a","title":"Deck","n_artefact":{all},"n_undelivered":{all},"latest_seq":{},"n_value_artefact":{value},"n_value_undelivered":{value},"latest_value_seq":{},"requests":[{}]}}]"#,
            latest.map_or("null".into(), |v| v.to_string()),
            value_seq.map_or("null".into(), |v| v.to_string()),
            list.join(",")
        ))
    }
    fn as_session(name: Option<&str>) -> State {
        State { name: name.map(str::to_string), ..Default::default() }
    }
    const CLAIM_LINE: &str = "1 request(s) on \"Deck\" are for this session: take them now with less_canvas_ops action 'claim' (the claim shows the pages and how each is settled), then ack each applied.";

    #[test]
    fn a_request_held_for_this_session_is_its_work_and_wakes_it() {
        let p = held_row(0, None, &[(7, Some(ME), SOON)]);
        assert_eq!(super::work_of(&p.sessions, &Me { name: Some(ME.into()), now: T }), vec![("a".into(), Work { untaken: 1.0, notes: 0.0, seq: Some(7.0) })]);
        let v = step(&p, &as_session(Some(ME)), &cwd(), T).0;
        let line = wake(&v).unwrap();
        assert!(line.starts_with(&format!("Designless canvas: {CLAIM_LINE}")), "{line}");
        assert!(!line.contains("edit(s) are waiting"), "a request is not a value edit: {line}");
        assert!(!line.contains('\u{2014}'));
    }

    #[test]
    fn a_request_held_for_another_session_wakes_nobody_until_its_window_passes() {
        let p = held_row(0, None, &[(7, Some(OTHER), SOON)]);
        for name in [Some(ME), None] {
            let (v, st) = step(&p, &as_session(name), &cwd(), T);
            assert_eq!(v, Voice::Silent, "{name:?}");
            assert_eq!(st.seen, vec![], "neither work nor news here: {name:?}");
            // The same request once its window has passed: anyone's, and news.
            let (v, st) = step(&p, &st, &cwd(), T + 5.0 * MIN);
            let line = wake(&v).unwrap_or_else(|| panic!("{name:?}"));
            assert!(line.contains(CLAIM_LINE), "{line}");
            assert_eq!(st.seen, vec![("a".into(), Work { untaken: 1.0, notes: 0.0, seq: Some(7.0) })]);
        }
    }

    #[test]
    fn a_request_offered_to_nobody_is_like_one_offered_to_this_session() {
        let p = held_row(0, None, &[(7, None, None)]);
        for name in [Some(ME), None] {
            let line = wake(&step(&p, &as_session(name), &cwd(), T).0).map(str::to_string);
            assert!(line.as_deref().is_some_and(|l| l.contains(CLAIM_LINE)), "{name:?}: {line:?}");
        }
    }

    #[test]
    fn a_session_with_no_name_waits_out_every_window() {
        // Its host does not say who it is, so it cannot be the session a request is held for.
        let p = held_row(0, None, &[(7, Some(ME), SOON)]);
        assert_eq!(step(&p, &as_session(None), &cwd(), T).0, Voice::Silent);
        assert!(wake(&step(&p, &as_session(None), &cwd(), T + 6.0 * MIN).0).is_some());
    }

    #[test]
    fn a_value_edit_beside_a_held_request_says_only_the_edit() {
        let p = held_row(1, Some(5), &[(6, Some(OTHER), SOON)]);
        let (v, st) = step(&p, &as_session(Some(ME)), &cwd(), T);
        let line = wake(&v).unwrap();
        assert!(line.starts_with("Designless canvas: 1 edit(s) are waiting on the Designless app for \"Deck\"."), "{line}");
        assert!(!line.contains("request(s)"), "{line}");
        assert_eq!(st.seen, vec![("a".into(), Work { untaken: 1.0, notes: 0.0, seq: Some(5.0) })], "the held request is not counted, nor its number");
    }

    #[test]
    fn a_newer_request_open_to_this_session_is_news_at_the_same_count_and_one_held_elsewhere_is_not() {
        // Told about one piece of work numbered 5, and that work since done.
        let told = State { seen: vec![("a".into(), Work { untaken: 1.0, notes: 0.0, seq: Some(5.0) })], name: Some(ME.into()), ..Default::default() };
        let mine = held_row(0, Some(5), &[(6, Some(ME), SOON)]);
        assert!(wake(&step(&mine, &told, &cwd(), T).0).is_some_and(|l| l.contains(CLAIM_LINE)));
        // One value edit still waiting, numbered 5, and a newer request held elsewhere.
        let elsewhere = held_row(1, Some(5), &[(6, Some(OTHER), SOON)]);
        assert_eq!(step(&elsewhere, &told, &cwd(), T).0, Voice::Silent);
        // Its window passes: now it is this session's too.
        assert!(wake(&step(&elsewhere, &told, &cwd(), T + 5.0 * MIN).0).is_some_and(|l| l.contains(CLAIM_LINE)));
    }

    #[test]
    fn a_row_that_says_only_half_of_it_is_read_as_before() {
        let me = Me { name: Some(ME.into()), now: T };
        let held = r#"{"seq":9,"kind":"look_slides","offered_to":"codex:9f8e7d6c","offered_until":"2999-01-01T00:00:00.000Z"}"#;
        for row in [
            format!(r#"[{{"session_id":"a","n_artefact":2,"n_undelivered":2,"latest_seq":9,"requests":[{held}]}}]"#),
            format!(r#"[{{"session_id":"a","n_artefact":2,"n_undelivered":2,"latest_seq":9,"n_value_undelivered":"1","requests":[{held}]}}]"#),
            r#"[{"session_id":"a","n_artefact":2,"n_undelivered":2,"latest_seq":9,"n_value_undelivered":1,"requests":null}]"#.to_string(),
        ] {
            assert_eq!(super::work_of(&sessions(&row).sessions, &me), vec![("a".into(), Work { untaken: 2.0, notes: 0.0, seq: Some(9.0) })], "{row}");
        }
    }

    #[test]
    fn the_seen_record_keeps_its_shape_and_holds_this_sessions_own_work() {
        let p = held_row(2, Some(4), &[(5, Some(ME), SOON), (6, Some(OTHER), SOON), (3, None, None)]);
        let w = super::work_of(&p.sessions, &Me { name: Some(ME.into()), now: T });
        assert_eq!(w, vec![("a".into(), Work { untaken: 4.0, notes: 0.0, seq: Some(5.0) })]);
        assert_eq!(js::stringify(&seen_json(&w)), r#"[["a",4,0,5]]"#);
        assert_eq!(seen_from_json(Some(&seen_json(&w))), w);
    }

    // ── idle backoff ──
    #[test]
    fn someone_working_is_answered_at_full_rate() {
        assert_eq!(poll_every_beats(0.0, false), 1);
        assert_eq!(poll_every_beats(4.0 * MIN, false), 1);
        for b in 0..16 {
            assert!(should_poll(b, BEAT_MS, false), "beat {b}");
        }
    }

    #[test]
    fn the_ladder_rungs() {
        assert_eq!(poll_every_beats(5.0 * MIN, false), 4);
        assert_eq!(poll_every_beats(29.0 * MIN, false), 4);
        for m in [30.0, 90.0, 600.0] {
            assert_eq!(poll_every_beats(m * MIN, false), 20);
        }
    }

    #[test]
    fn the_ladder_only_ever_slows_down() {
        let mut last = 1;
        for m in 0..=120 {
            let e = poll_every_beats(m as f64 * MIN, false);
            assert!(e >= last, "rate increased again at {m}m");
            last = e;
        }
    }

    #[test]
    fn an_idle_hour_costs_a_fraction_of_what_it_did() {
        let per_hour = 3_600_000 / BEAT_MS;
        assert_eq!(per_hour, 240);
        assert_eq!((0..per_hour).filter(|b| should_poll(*b, BEAT_MS, false)).count(), 51);
        assert_eq!((per_hour..per_hour * 2).filter(|b| should_poll(*b, BEAT_MS, false)).count(), 12);
    }

    #[test]
    fn the_beat_never_slows() {
        assert!(BEAT_MS as f64 * 3.0 <= marker::STALE_MS);
        let slowest = poll_every_beats(60.0 * MIN, false) * BEAT_MS;
        assert_eq!(slowest, 300_000);
        assert!(slowest as f64 > marker::STALE_MS);
    }

    // ── presence ──
    #[test]
    fn a_long_quiet_canvas_with_someone_present_polls_once_a_minute() {
        assert_eq!(poll_every_beats(45.0 * MIN, false), 20);
        assert_eq!(poll_every_beats(45.0 * MIN, true), 4);
    }

    #[test]
    fn presence_never_speeds_the_ladder_past_what_quiet_earned() {
        assert_eq!(poll_every_beats(0.0, true), 1);
        assert_eq!(poll_every_beats(4.0 * MIN, true), 1);
        assert_eq!(poll_every_beats(10.0 * MIN, true), 4);
        assert_eq!(poll_every_beats(10.0 * MIN, false), 4);
    }

    #[test]
    fn idle_hour_costs_with_and_without_someone_present() {
        let count = |active| (240u64..480).filter(|b| b % poll_every_beats((b * 15_000) as f64, active) == 0).count();
        assert_eq!(count(false), 12);
        assert_eq!(count(true), 60);
    }

    // ── the watcher's voice ──
    fn wake(v: &Voice) -> Option<&str> {
        match v {
            Voice::Wake(t) => Some(t),
            _ => None,
        }
    }

    #[test]
    fn new_work_wakes_the_same_work_does_not() {
        let one = sessions(r#"[{"session_id":"a","n_artefact":2,"title":"Deck"}]"#);
        let (voice, st) = step(&one, &State::default(), &cwd(), 0.0);
        assert!(wake(&voice).is_some());
        assert_eq!(step(&one, &st, &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn a_second_edit_on_the_same_canvas_is_new_work() {
        let (_, st) = step(&sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#), &State::default(), &cwd(), 0.0);
        assert!(wake(&step(&sessions(r#"[{"session_id":"a","n_artefact":2,"title":"Deck"}]"#), &st, &cwd(), 0.0).0).is_some());
    }

    #[test]
    fn work_that_clears_then_returns_wakes_again() {
        let one = sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#);
        let (_, st) = step(&one, &State::default(), &cwd(), 0.0);
        let (_, st) = step(&sessions("[]"), &st, &cwd(), 0.0);
        assert!(wake(&step(&one, &st, &cwd(), 0.0).0).is_some());
    }

    #[test]
    fn items_only_the_user_can_act_on_never_wake_the_agent() {
        let p = sessions(r#"[{"session_id":"a","n_needs_human":3,"brand_slug":"acme"}]"#);
        assert_eq!(step(&p, &State::default(), &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn edits_already_in_an_agents_hands_never_wake() {
        // The phantom: an expired canvas whose five edits an agent took and
        // never finished. The inbox lists them; nobody can take them; the
        // server's own wait does not wake on them, and neither does this.
        let p = sessions(r#"[{"session_id":"old","recoverable":true,"n_artefact":5,"n_undelivered":0,"title":"Old badge"}]"#);
        assert_eq!(step(&p, &State::default(), &cwd(), 0.0).0, Voice::Silent);
        let page = sessions(r#"[{"session_id":"p","n_page":2,"n_undelivered":0}]"#);
        assert_eq!(step(&page, &State::default(), &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn an_expired_canvas_with_untaken_edits_wakes_as_the_server_wait_does() {
        let p = sessions(r#"[{"session_id":"old","recoverable":true,"n_artefact":2,"n_undelivered":2,"title":"Old badge"}]"#);
        let line = step(&p, &State::default(), &cwd(), 0.0).0;
        let line = wake(&line).unwrap();
        assert!(line.contains("revive in place"), "{line}");
    }

    #[test]
    fn the_agents_own_progress_never_wakes_it() {
        let rows = |n: u32| sessions(&format!(r#"[{{"session_id":"a","n_artefact":{n},"n_undelivered":{n},"title":"Deck"}}]"#));
        let (first, mut st) = step(&rows(3), &State::default(), &cwd(), 0.0);
        assert!(wake(&first).is_some());
        for n in [2, 1, 0] {
            let (v, next) = step(&rows(n), &st, &cwd(), 0.0);
            assert_eq!(v, Voice::Silent, "applying down to {n} is not news");
            st = next;
        }
        assert!(wake(&step(&rows(1), &st, &cwd(), 0.0).0).is_some(), "a new edit after that is");
    }

    #[test]
    fn a_new_edit_after_the_agent_applied_is_news_at_the_same_count() {
        // The agent was told about one edit and applied it while no watcher
        // ran, so nothing saw the count fall. The next edit brings it back to
        // one: only the edit number says it is new.
        let told = sessions(r#"[{"session_id":"a","n_artefact":1,"n_undelivered":1,"latest_seq":4}]"#);
        let st = State { seen: work_of(&told.sessions), ..Default::default() };
        let next = sessions(r#"[{"session_id":"a","n_artefact":1,"n_undelivered":1,"latest_seq":5}]"#);
        assert!(wake(&step(&next, &st, &cwd(), 0.0).0).is_some());
        // The same edit, still waiting, is not.
        assert_eq!(step(&told, &st, &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn applying_does_not_advance_the_edit_number() {
        let rows = |n: u32| sessions(&format!(r#"[{{"session_id":"a","n_artefact":{n},"n_undelivered":{n},"latest_seq":7}}]"#));
        let (_, mut st) = step(&rows(2), &State::default(), &cwd(), 0.0);
        for n in [1, 0] {
            let (v, next) = step(&rows(n), &st, &cwd(), 0.0);
            assert_eq!(v, Voice::Silent);
            st = next;
        }
    }

    #[test]
    fn claiming_an_edit_is_not_news_either() {
        let (_, st) = step(&sessions(r#"[{"session_id":"a","n_artefact":1,"n_undelivered":1}]"#), &State::default(), &cwd(), 0.0);
        let claimed = sessions(r#"[{"session_id":"a","n_artefact":1,"n_undelivered":0}]"#);
        assert_eq!(step(&claimed, &st, &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn a_note_in_hand_does_not_wake_again_and_a_new_note_does() {
        let notes = |n: u32| sessions(&format!(r#"[{{"session_id":"a","n_annotation":{n}}}]"#));
        let (v, st) = step(&notes(1), &State::default(), &cwd(), 0.0);
        assert!(wake(&v).is_some());
        // Reading a note does not take it: the same count is not news.
        assert_eq!(step(&notes(1), &st, &cwd(), 0.0).0, Voice::Silent);
        assert!(wake(&step(&notes(2), &st, &cwd(), 0.0).0).is_some());
    }

    #[test]
    fn the_line_carries_only_the_canvases_with_news() {
        let (_, st) = step(&sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#), &State::default(), &cwd(), 0.0);
        let both = sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"},{"session_id":"b","n_artefact":1,"title":"Poster"}]"#);
        let v = step(&both, &st, &cwd(), 0.0).0;
        let line = wake(&v).unwrap();
        assert!(line.contains("\"Poster\""), "{line}");
        assert!(!line.contains("\"Deck\""), "told before, so not told again: {line}");
    }

    #[test]
    fn the_watchers_line_never_asks_for_a_wait() {
        // The watcher IS the wait. A line that asked for one more turned every
        // edit into a loop of whole-conversation re-reads.
        let v = step(&sessions(r#"[{"session_id":"a","n_artefact":1,"n_annotation":1,"recoverable":true}]"#), &State::default(), &cwd(), 0.0).0;
        let line = wake(&v).unwrap();
        for w in ["less_stream", "wait_seconds", "loop"] {
            assert!(!line.contains(w), "{w}: {line}");
        }
    }

    #[test]
    fn a_row_from_an_older_server_counts_every_edit_as_untaken() {
        assert_eq!(work_of(&sessions(r#"[{"session_id":"a","n_page":1,"n_artefact":2}]"#).sessions), vec![("a".into(), Work { untaken: 3.0, notes: 0.0, seq: None })]);
        assert_eq!(work_of(&sessions(r#"[{"session_id":"a","n_page":1,"n_artefact":2,"n_undelivered":null}]"#).sessions)[0].1.untaken, 3.0);
        assert_eq!(work_of(&sessions(r#"[{"session_id":"a","n_artefact":2,"n_undelivered":1}]"#).sessions)[0].1.untaken, 1.0);
    }

    #[test]
    fn what_was_seen_survives_its_disk_shape() {
        let w = work_of(&sessions(r#"[{"session_id":"b","n_page":"2"},{"session_id":"a","n_annotation":1}]"#).sessions);
        assert_eq!(seen_from_json(Some(&js::parse(&js::stringify(&seen_json(&w))).unwrap())), w);
        assert_eq!(seen_from_json(None), vec![]);
        assert_eq!(seen_from_json(Some(&J::Str("junk".into()))), vec![]);
        assert_eq!(seen_from_json(Some(&js::parse(r#"[["a",1],"x",["b",1,0]]"#).unwrap())), vec![("b".into(), Work { untaken: 1.0, notes: 0.0, seq: None })]);
        let with_seq = work_of(&sessions(r#"[{"session_id":"c","n_artefact":1,"latest_seq":9}]"#).sessions);
        assert_eq!(with_seq[0].1.seq, Some(9.0));
        assert_eq!(seen_from_json(Some(&js::parse(&js::stringify(&seen_json(&with_seq))).unwrap())), with_seq);
    }

    #[test]
    fn work_already_told_does_not_wake_a_new_watcher() {
        let p = sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#);
        let told = State { seen: work_of(&p.sessions), ..Default::default() };
        assert_eq!(step(&p, &told, &cwd(), 0.0).0, Voice::Silent);
    }

    // ── a desktop that cannot answer ──
    #[test]
    fn a_blind_watcher_logs_once_and_never_wakes() {
        let (voice, st) = step(&blind("timeout after 700ms"), &State::default(), &cwd(), 0.0);
        let Voice::Log(line) = voice else { panic!("{voice:?}") };
        assert!(line.contains(MISSED_HINT));
        assert!(line.contains("still watching"));
        assert!(!line.contains("less_canvas_inbox"), "a log line asks nothing of anyone");
        for w in ["closed", "shut", "quit", "not running", "unreachable"] {
            assert!(!line.to_lowercase().contains(w), "{w}");
        }
        assert_eq!(step(&blind("timeout after 700ms"), &st, &cwd(), 0.0).0, Voice::Silent);
    }

    #[test]
    fn blindness_keeps_what_was_seen() {
        let p = sessions(r#"[{"session_id":"a","n_artefact":1}]"#);
        let (_, st) = step(&p, &State::default(), &cwd(), 0.0);
        let (_, st) = step(&blind("x"), &st, &cwd(), 0.0);
        assert_eq!(st.seen, work_of(&p.sessions));
        assert_eq!(step(&p, &st, &cwd(), 0.0).0, Voice::Silent, "sight returning is not news");
    }

    #[test]
    fn the_blind_line_stays_a_signal_not_a_lesson() {
        const CEILING: usize = 190;
        for r in ["timeout after 700ms", "desktop replied no_session_stale", "socket connect failed"] {
            let Voice::Log(line) = step(&blind(r), &State::default(), &cwd(), 0.0).0 else { panic!() };
            assert!(line.len() <= CEILING, "{} > {CEILING} with {r}", line.len());
        }
        assert!(MISSED_HINT.len() <= 60);
        assert!(!MISSED_HINT.contains('\u{2014}'));
    }

    #[test]
    fn an_accelerator_that_flaps_is_logged_once() {
        let (b, g) = (blind("timeout after 700ms"), sessions("[]"));
        let mut st = State::default();
        let mut spoke = 0;
        for p in [&b, &g, &b, &g, &b, &g, &b] {
            let (v, next) = step(p, &st, &cwd(), 0.0);
            assert!(wake(&v).is_none());
            spoke += (v != Voice::Silent) as u32;
            st = next;
        }
        assert_eq!(spoke, 1);
    }

    #[test]
    fn sight_returning_steadily_makes_a_relapse_news() {
        let (b, g) = (blind("timeout after 700ms"), sessions("[]"));
        let mut st = step(&b, &State::default(), &cwd(), 0.0).1;
        for _ in 0..3 {
            st = step(&g, &st, &cwd(), 0.0).1;
        }
        assert!(matches!(step(&b, &st, &cwd(), 0.0).0, Voice::Log(_)));
    }

    #[test]
    fn a_changed_reason_is_the_same_blindness() {
        let reasons = ["timeout after 700ms", "desktop replied no_session_stale"];
        let mut st = State::default();
        let mut spoke = 0;
        for i in 0..12 {
            let (v, next) = step(&blind(reasons[i % 2]), &st, &cwd(), 0.0);
            spoke += (v != Voice::Silent) as u32;
            st = next;
        }
        assert_eq!(spoke, 1);
    }

    #[test]
    fn the_watcher_waits_for_the_desktop_and_the_hooks_do_not() {
        // The hooks' 700 ms is theirs. The bounds on the watcher's own budget
        // are checked where it is declared, at compile time.
        let src = include_str!("watch.rs");
        let body = &src[src.find("pub async fn run(").unwrap()..src.find("#[cfg(test)]").unwrap()];
        assert!(body.contains("probe_inbox_within(WATCH_BUDGET_MS)"));
        assert!(!body.contains("probe::probe_inbox()"), "the hooks' budget is not the watcher's");
    }

    #[test]
    fn a_watcher_whose_starter_is_gone_leaves() {
        assert!(!left_behind(4242, 4242));
        assert!(left_behind(4242, 1), "reparented: the session that wanted it has ended");
        assert!(!left_behind(1, 1), "started orphaned, so there is nothing to notice");
    }

    #[test]
    fn a_host_that_names_itself_is_watched_and_one_that_does_not_never_ends_the_watch() {
        assert_eq!(host_pid(Some("4242")), Some(4242));
        assert_eq!(host_pid(Some(" 4242\n")), Some(4242));
        for absent in [None, Some(""), Some("abc"), Some("1"), Some("0"), Some("-5")] {
            assert_eq!(host_pid(absent), None, "{absent:?}");
        }
        assert!(!host_gone(None), "no host named (Codex, other hosts): keep watching");
        assert!(!host_gone(Some(std::process::id() as i32)), "a live host");
        // A process that has certainly ended: start one, wait for it.
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let dead = child.id() as i32;
        child.wait().unwrap();
        assert!(host_gone(Some(dead)), "a host that has exited");
    }

    #[test]
    fn the_host_check_comes_before_the_look_and_says_nothing() {
        let src = include_str!("watch.rs");
        let body = &src[src.find("pub async fn run(").unwrap()..];
        let check = body.find("if host_gone(host)").unwrap();
        assert!(check < body.find("probe_inbox_within(").unwrap(), "a gone host costs no desktop read");
        let leave = &body[check..check + body[check..].find('}').unwrap()];
        assert!(leave.contains("return stop()") && !leave.contains("say("), "{leave}");
    }

    // ── the one-shot ──
    #[test]
    fn the_command_line_takes_once_anywhere() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(&a(&["--once", "s1"])), (Some("s1"), true));
        assert_eq!(parse_args(&a(&["s1", "--once"])), (Some("s1"), true));
        assert_eq!(parse_args(&a(&["s1"])), (Some("s1"), false));
        assert_eq!(parse_args(&a(&["--once"])), (None, true));
        assert_eq!(parse_args(&a(&[])), (None, false));
    }

    #[test]
    fn the_one_shot_says_how_to_keep_watching() {
        let line = once_line("Designless canvas: 1 edit(s) are waiting.", "/bin/sh '/p/bin/designless' inbox-watch", "s1");
        assert!(line.starts_with("Designless canvas: 1 edit(s) are waiting. Once they are applied"));
        assert!(line.ends_with("/bin/sh '/p/bin/designless' inbox-watch --once s1"));
        assert!(line.contains("\"Designless Agent\""));
        assert!(!line.contains('\u{2014}'));
    }

    #[test]
    fn the_one_shot_never_stands_down_and_never_logs_to_stdout() {
        // Wiring guard, read from this file's own loop: a one-shot's exit is a
        // wake, so only news may end it; and a log line never reaches stdout.
        let src = include_str!("watch.rs");
        let body = &src[src.find("pub async fn run(").unwrap()..src.find("#[cfg(test)]").unwrap()];
        assert!(body.contains("if !once && should_stand_down(&state"));
        let at = body.find("Voice::Log(line) =>").unwrap();
        let arm = &body[at..at + 120];
        assert!(arm.contains("log(&line)") && !arm.contains("say("), "{arm}");
    }

    #[test]
    fn a_desktop_gone_for_half_an_hour_ends_the_watch() {
        let t0 = 1_000_000.0;
        let st = step(&blind("timeout after 700ms"), &State::default(), &cwd(), t0).1;
        assert!(!should_stand_down(&st, t0, STAND_DOWN_MS));
        assert!(!should_stand_down(&st, t0 + 29.0 * MIN, STAND_DOWN_MS));
        assert!(should_stand_down(&st, t0 + 30.0 * MIN, STAND_DOWN_MS));
    }

    #[test]
    fn the_stand_down_clock_is_not_reset_by_failing() {
        let t0 = 1_000_000.0;
        let mut st = step(&blind("x"), &State::default(), &cwd(), t0).1;
        for i in 1..=20 {
            st = step(&blind("x"), &st, &cwd(), t0 + i as f64 * MIN).1;
        }
        assert_eq!(st.blind_since, Some(t0));
        assert!(should_stand_down(&st, t0 + 30.0 * MIN, STAND_DOWN_MS));
    }

    #[test]
    fn a_recovered_desktop_clears_the_clock() {
        let t0 = 1_000_000.0;
        let mut st = step(&blind("x"), &State::default(), &cwd(), t0).1;
        for i in 1..=3 {
            st = step(&sessions("[]"), &st, &cwd(), t0 + i as f64 * 1000.0).1;
        }
        assert_eq!(st.blind_since, None);
        assert!(!should_stand_down(&st, t0 + 60.0 * MIN, STAND_DOWN_MS));
    }

    #[test]
    fn a_watcher_that_can_see_keeps_watching() {
        let now = js::now_ms();
        assert!(!should_stand_down(&State { seen: vec![("a".into(), Work { untaken: 1.0, notes: 0.0, seq: None })], healthy: 9, ..Default::default() }, now, STAND_DOWN_MS));
        assert!(!should_stand_down(&State::default(), now, STAND_DOWN_MS));
        assert!(!should_stand_down(&State { blind: false, blind_since: Some(now - 60.0 * MIN), ..Default::default() }, now, STAND_DOWN_MS));
    }

    #[test]
    fn the_stand_down_is_silent() {
        // Wiring guard: between deciding to stand down and stopping, nothing is
        // printed. Read from this file's own loop, as the original guard read its.
        let src = include_str!("watch.rs");
        let body = &src[src.find("pub async fn run(").unwrap()..src.find("#[cfg(test)]").unwrap()];
        let at = body.find("should_stand_down(&state").unwrap();
        let after = &body[at..at + 200];
        let stop_at = after.find("return stop()").unwrap();
        assert!(!after[..stop_at].contains("say("));
        assert!(!body.to_lowercase().contains("standing down"));
    }

    #[test]
    fn work_of_ignores_canvases_with_nothing_for_an_agent() {
        assert_eq!(work_of(&sessions(r#"[{"session_id":"a","n_needs_human":5}]"#).sessions), vec![]);
        assert_eq!(work_of(&[]), vec![]);
        assert_eq!(
            work_of(&sessions(r#"[{"session_id":"b","n_page":"2"},{"n_artefact":1}]"#).sessions),
            vec![("".into(), Work { untaken: 1.0, notes: 0.0, seq: None }), ("b".into(), Work { untaken: 2.0, notes: 0.0, seq: None })]
        );
    }
}
