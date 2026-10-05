//! The live canvas watcher: the stretch between turns that hooks cannot reach.
//!
//! The host runs it as a persistent monitor that streams its output, and every
//! line it prints wakes the agent. It prints ONLY when new edits arrive: a
//! background task that chatters is throttled and then stopped by the host.
//! One per session, enforced by the marker it claims on start.
//!
//! Usage: `designless-mcp-bridge inbox-watch <host-session-id>`

use super::js::{self, J};
use super::marker::{self, BEAT_MS};
use super::probe::{self, Probe, MISSED_HINT};
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
/// an accelerator that flaps is announced once, not once per cycle.
const HEALTHY_STREAK: u64 = 3;

/// How long the desktop may stay unanswering before the watcher stands down.
/// Half an hour means the app is closed or the machine asleep; nobody is
/// editing, and the turn-boundary hook covers the person's return.
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

/// What is drainable right now, as a value that changes only when the work
/// does. Counts, not just ids: a second edit on one canvas is new work.
/// Attention items are absent: they are the user's, not the agent's.
pub fn drain_digest(sessions: &[J]) -> String {
    let mut rows: Vec<String> = sessions
        .iter()
        .filter_map(|s| {
            let n = |k: &str| js::num_or_zero(js::get(Some(s), k));
            let (p, a, x) = (n("n_page"), n("n_artefact"), n("n_annotation"));
            // NaN counts as nothing, as it does in the original comparison.
            if (p + a + x).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return None;
            }
            let id = js::get(Some(s), "session_id");
            let id = if js::nullish(id) { String::new() } else { js::to_string(id) };
            Some(format!("{id}:{}:{}:{}", js::num_to_string(p), js::num_to_string(a), js::num_to_string(x)))
        })
        .collect();
    rows.sort_by(|a, b| js::utf16_cmp(a, b));
    rows.join("|")
}

/// The state carried from one poll to the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub digest: String,
    pub healthy: u64,
    pub blind: bool,
    pub blind_since: Option<f64>,
}

/// One poll's decision, pure: the line to print (or silence) and the next state.
pub fn step(probe: &Probe, prev: &State, cwd: &str, now: f64) -> (Option<String>, State) {
    if let Some(unknown) = &probe.unknown {
        let blind_since = prev.blind_since.or(Some(now));
        // Latched on the FACT, not the reason: an unreachable desktop does not
        // fail the same way twice, so a reason-keyed latch spoke every poll.
        if prev.blind {
            return (None, State { blind_since, healthy: 0, ..prev.clone() });
        }
        return (
            Some(format!(
                "Designless canvas: the watcher's quick check did not answer ({unknown}). Not an all-clear: read less_canvas_inbox. {MISSED_HINT}"
            )),
            State { blind: true, blind_since, healthy: 0, ..prev.clone() },
        );
    }
    let healthy = prev.healthy + 1;
    let digest = drain_digest(&probe.sessions);
    let recovered = healthy >= HEALTHY_STREAK;
    let next = State {
        digest: digest.clone(),
        healthy,
        blind: if recovered { false } else { prev.blind },
        blind_since: if recovered { None } else { prev.blind_since },
    };
    if digest.is_empty() || digest == prev.digest {
        return (None, next);
    }
    let text = probe::summarize_inbox(&probe.sessions, cwd, probe::Opts { include_attention: false, attn_dark: None });
    if text.is_empty() {
        return (None, next);
    }
    (Some(format!("Designless canvas: {text}")), next)
}

/// Has the desktop been unanswering long enough that watching is pointless?
/// Asks whether it is blind NOW, never whether it ever was.
pub fn should_stand_down(state: &State, now: f64, after_ms: f64) -> bool {
    match state.blind_since {
        Some(t) if state.blind && t != 0.0 => now - t >= after_ms,
        _ => false,
    }
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

pub async fn run(session: Option<&str>, env: &Env) -> i32 {
    let Some(sid) = session.filter(|s| !s.is_empty()) else {
        say("Designless watcher: no session id was passed, so it did not start.\n");
        return 0;
    };
    let sid = J::Str(sid.to_string());
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
    let stop = || {
        marker::disarm(&env.home, &sid);
        0
    };

    let mut state = State::default();
    // Beats since anything was news. Drives the ladder, and nothing else.
    let mut quiet_beats: u64 = 0;
    loop {
        // Presence is read on every beat, so a return to the machine is
        // answered on the next one.
        let active = marker::host_is_active(&env.home, js::now_ms(), marker::ACTIVE_WINDOW_MS);
        if should_poll(quiet_beats, BEAT_MS, active) {
            let probe = probe::probe_inbox().await;
            let (line, next) = step(&probe, &state, &cwd, js::now_ms());
            state = next;
            if let Some(line) = line {
                if !say(&format!("{line}\n")) {
                    return stop();
                }
                quiet_beats = 0;
            } else {
                quiet_beats += 1;
            }
            // In silence: the blind line has already said the one thing an
            // agent may act on, and the host reports the exit itself.
            if should_stand_down(&state, js::now_ms(), STAND_DOWN_MS) {
                return stop();
            }
        } else {
            quiet_beats += 1;
        }
        // The beat never slows: a stale marker frees the session to a second
        // watcher, and a crash that stops beating frees it instead of locking it.
        marker::beat(&env.home, &sid, js::now_ms());
        tokio::time::sleep(std::time::Duration::from_millis(BEAT_MS)).await;
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
    #[test]
    fn new_work_wakes_the_same_work_does_not() {
        let one = sessions(r#"[{"session_id":"a","n_artefact":2,"title":"Deck"}]"#);
        let (line, st) = step(&one, &State::default(), &cwd(), 0.0);
        assert!(line.is_some());
        assert_eq!(step(&one, &st, &cwd(), 0.0).0, None);
    }

    #[test]
    fn a_second_edit_on_the_same_canvas_is_new_work() {
        let (_, st) = step(&sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#), &State::default(), &cwd(), 0.0);
        assert!(step(&sessions(r#"[{"session_id":"a","n_artefact":2,"title":"Deck"}]"#), &st, &cwd(), 0.0).0.is_some());
    }

    #[test]
    fn work_that_clears_then_returns_wakes_again() {
        let one = sessions(r#"[{"session_id":"a","n_artefact":1,"title":"Deck"}]"#);
        let (_, st) = step(&one, &State::default(), &cwd(), 0.0);
        let (_, st) = step(&sessions("[]"), &st, &cwd(), 0.0);
        assert!(step(&one, &st, &cwd(), 0.0).0.is_some());
    }

    #[test]
    fn items_only_the_user_can_act_on_never_wake_the_agent() {
        let p = sessions(r#"[{"session_id":"a","n_needs_human":3,"brand_slug":"acme"}]"#);
        assert_eq!(step(&p, &State::default(), &cwd(), 0.0).0, None);
    }

    #[test]
    fn a_blind_watcher_says_so_once_and_says_it_is_not_an_all_clear() {
        let (line, st) = step(&blind("timeout after 700ms"), &State::default(), &cwd(), 0.0);
        let line = line.unwrap();
        assert!(line.to_lowercase().contains("not an all-clear"));
        assert!(line.contains("less_canvas_inbox"));
        assert!(line.contains(MISSED_HINT));
        for w in ["closed", "shut", "quit", "not running", "unreachable"] {
            assert!(!line.to_lowercase().contains(w), "{w}");
        }
        assert_eq!(step(&blind("timeout after 700ms"), &st, &cwd(), 0.0).0, None);
    }

    #[test]
    fn the_blind_line_stays_a_signal_not_a_lesson() {
        const CEILING: usize = 190;
        for r in ["timeout after 700ms", "desktop replied no_session_stale", "socket connect failed"] {
            let line = step(&blind(r), &State::default(), &cwd(), 0.0).0.unwrap();
            assert!(line.len() <= CEILING, "{} > {CEILING} with {r}", line.len());
        }
        assert!(MISSED_HINT.len() <= 60);
        assert!(!MISSED_HINT.contains('\u{2014}'));
    }

    #[test]
    fn an_accelerator_that_flaps_is_announced_once() {
        let (b, g) = (blind("timeout after 700ms"), sessions("[]"));
        let mut st = State::default();
        let mut spoke = 0;
        for p in [&b, &g, &b, &g, &b, &g, &b] {
            let (line, next) = step(p, &st, &cwd(), 0.0);
            spoke += line.is_some() as u32;
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
        assert!(step(&b, &st, &cwd(), 0.0).0.is_some());
    }

    #[test]
    fn a_changed_reason_is_the_same_blindness() {
        let reasons = ["timeout after 700ms", "desktop replied no_session_stale"];
        let mut st = State::default();
        let mut spoke = 0;
        for i in 0..12 {
            let (line, next) = step(&blind(reasons[i % 2]), &st, &cwd(), 0.0);
            spoke += line.is_some() as u32;
            st = next;
        }
        assert_eq!(spoke, 1);
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
        assert!(!should_stand_down(&State { digest: "a:1:0:0".into(), healthy: 9, ..Default::default() }, now, STAND_DOWN_MS));
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
    fn drain_digest_ignores_sessions_with_nothing_drainable() {
        assert_eq!(drain_digest(&sessions(r#"[{"session_id":"a","n_needs_human":5}]"#).sessions), "");
        assert_eq!(drain_digest(&[]), "");
        assert_eq!(drain_digest(&sessions(r#"[{"session_id":"b","n_page":"2"},{"n_artefact":1}]"#).sessions), ":0:1:0|b:2:0:0");
    }
}
