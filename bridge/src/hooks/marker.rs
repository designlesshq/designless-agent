//! Two small files under `~/.designless/watch/`, shared with any other copy of
//! the plugin on the machine, so their names and shapes do not change.
//!
//! **The watch marker** (`<session>.json`): whether a live watcher is running
//! for a host session. The WATCHER writes it, not the agent, so an ignored ask
//! looks exactly like one never sent and the next canvas call asks again. It is
//! also the one-watcher rule, enforced: a second watcher finds it and exits.
//!
//! **Host activity** (`host-activity.json`): when a person last submitted a
//! prompt in ANY session. The watcher's idle ladder reads it, so a quiet canvas
//! with someone at the keyboard is not mistaken for an empty room.
//!
//! Never fails: an unreadable file reads as "not armed" or "nobody there".

use super::js::{self, J};

/// A watcher that stopped beating this long ago is gone, whatever the file says.
pub const STALE_MS: f64 = 90_000.0;

/// How often the watcher refreshes its beat. Several misses fit inside STALE_MS.
pub const BEAT_MS: u64 = 15_000;

/// How long a submitted prompt counts as someone being present.
pub const ACTIVE_WINDOW_MS: f64 = 10.0 * 60_000.0;

pub fn watch_dir(home: &str) -> String {
    js::join(&[home, ".designless", "watch"])
}

/// A session id reduced to the characters a file name may carry.
pub fn sanitize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect()
}

pub fn marker_path(home: &str, session: &J) -> String {
    js::join(&[&watch_dir(home), &format!("{}.json", sanitize_id(&js::to_string(Some(session))))])
}

pub fn read_marker(home: &str, session: Option<&J>) -> Option<J> {
    let session = session.filter(|s| js::truthy(Some(s)))?;
    js::parse(&js::read_utf8(&marker_path(home, session)).ok()?)
}

/// Is a watcher alive for this session? An unreadable or stale marker reads as
/// NOT armed: being wrong that way costs one redundant ask; the other way
/// costs a session with no watcher at all.
pub fn is_armed(home: &str, session: Option<&J>, now: f64) -> bool {
    let Some(m) = read_marker(home, session) else { return false };
    let Some(J::Str(beat)) = js::get(Some(&m), "beat_at") else { return false };
    let t = js::parse_date(beat);
    t.is_finite() && now - t < STALE_MS
}

/// Claim the session. False when someone else already holds it.
pub fn arm(home: &str, session: &J, now: f64) -> bool {
    if !js::truthy(Some(session)) || is_armed(home, Some(session), now) {
        return false;
    }
    let body = js::obj(vec![
        ("pid", J::Num(std::process::id() as f64)),
        ("started_at", J::Str(js::iso_string(now))),
        ("beat_at", J::Str(js::iso_string(now))),
    ]);
    std::fs::create_dir_all(watch_dir(home)).is_ok()
        && std::fs::write(marker_path(home, session), js::stringify(&body)).is_ok()
}

/// `{ ...v }` for whatever a file happened to hold.
fn spread(v: &J) -> Vec<(String, J)> {
    match v {
        J::Obj(m) => m.clone(),
        J::Arr(a) => a.iter().enumerate().map(|(i, x)| (i.to_string(), x.clone())).collect(),
        J::Str(s) => s.chars().enumerate().map(|(i, c)| (i.to_string(), J::Str(c.to_string()))).collect(),
        _ => Vec::new(),
    }
}

/// Still here. A missed beat is what makes a marker stale.
pub fn beat(home: &str, session: &J, now: f64) -> bool {
    let Some(m) = read_marker(home, Some(session)) else { return false };
    if !js::truthy(Some(&m)) {
        return false;
    }
    let mut next = spread(&m);
    js::set(&mut next, "beat_at", J::Str(js::iso_string(now)));
    std::fs::write(marker_path(home, session), js::stringify(&J::Obj(next))).is_ok()
}

pub fn disarm(home: &str, session: &J) -> bool {
    std::fs::remove_file(marker_path(home, session)).is_ok()
}

pub fn activity_path(home: &str) -> String {
    js::join(&[home, ".designless", "watch", "host-activity.json"])
}

/// Record that a person just did something. Last writer wins, which is the
/// semantics wanted: the newest prompt is the freshest evidence.
pub fn note_activity(home: &str, now: f64) -> bool {
    let p = activity_path(home);
    std::fs::create_dir_all(js::dirname(&p)).is_ok()
        && std::fs::write(&p, js::stringify(&js::obj(vec![("at", J::Num(now))]))).is_ok()
}

/// Milliseconds since the last prompt anywhere, or `None` with no stamp. A
/// stamp from the future is a clock that moved; it reads as "just now".
pub fn ms_since_activity(home: &str, now: f64) -> Option<f64> {
    let raw = js::parse(&js::read_utf8(&activity_path(home)).ok()?)?;
    let at = js::to_number(js::get(Some(&raw), "at"));
    at.is_finite().then(|| (now - at).max(0.0))
}

/// Is a person present? False with no stamp at all: presence has to be shown.
pub fn host_is_active(home: &str, now: f64, window_ms: f64) -> bool {
    ms_since_activity(home, now).is_some_and(|age| age < window_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn scratch_home(tag: &str) -> String {
        let d = std::env::temp_dir().join(format!("dl-home-{tag}-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }
    fn sid(s: &str) -> J {
        J::Str(s.into())
    }

    #[test]
    fn a_second_watcher_cannot_claim_a_held_session() {
        let h = scratch_home("a");
        let now = js::now_ms();
        assert!(arm(&h, &sid("s-a"), now));
        assert!(!arm(&h, &sid("s-a"), now), "a sub-agent starting its own must find it taken");
        assert!(is_armed(&h, Some(&sid("s-a")), now));
    }

    #[test]
    fn a_watcher_that_stopped_beating_frees_the_session() {
        let h = scratch_home("b");
        let now = js::now_ms();
        arm(&h, &sid("s-b"), now);
        let later = now + STALE_MS + 1000.0;
        assert!(!is_armed(&h, Some(&sid("s-b")), later));
        assert!(arm(&h, &sid("s-b"), later));
        assert!(is_armed(&h, Some(&sid("s-b")), later + 1000.0));
        assert!(beat(&h, &sid("s-b"), later + 1000.0));
        assert!(is_armed(&h, Some(&sid("s-b")), later + STALE_MS), "a beat keeps it alive");
    }

    #[test]
    fn an_unknown_session_is_never_armed() {
        let h = scratch_home("c");
        assert!(!is_armed(&h, Some(&sid("never")), js::now_ms()));
        assert!(!is_armed(&h, None, js::now_ms()));
        assert!(!is_armed(&h, Some(&sid("")), js::now_ms()));
    }

    #[test]
    fn the_marker_keeps_its_shape_and_key_order() {
        let h = scratch_home("d");
        let t = 1_759_670_000_123.0;
        arm(&h, &sid("abc/../x"), t);
        let p = marker_path(&h, &sid("abc/../x"));
        assert!(p.ends_with("/.designless/watch/abcx.json"), "{p}");
        let body = std::fs::read_to_string(&p).unwrap();
        assert_eq!(
            body,
            format!(r#"{{"pid":{},"started_at":"2025-10-05T13:13:20.123Z","beat_at":"2025-10-05T13:13:20.123Z"}}"#, std::process::id())
        );
        beat(&h, &sid("abc/../x"), t + 1000.0);
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.ends_with(r#""started_at":"2025-10-05T13:13:20.123Z","beat_at":"2025-10-05T13:13:21.123Z"}"#), "{body}");
        assert!(disarm(&h, &sid("abc/../x")));
    }

    #[test]
    fn a_submitted_prompt_is_presence_and_it_expires() {
        let h = scratch_home("e");
        let now = js::now_ms();
        assert!(note_activity(&h, now));
        assert!(ms_since_activity(&h, now).unwrap() < 50.0);
        assert!(host_is_active(&h, now, ACTIVE_WINDOW_MS));
        assert!(host_is_active(&h, now + ACTIVE_WINDOW_MS - 1000.0, ACTIVE_WINDOW_MS));
        assert!(!host_is_active(&h, now + ACTIVE_WINDOW_MS + 1000.0, ACTIVE_WINDOW_MS));
    }

    #[test]
    fn no_stamp_reads_as_nobody_there() {
        let h = scratch_home("f");
        assert_eq!(ms_since_activity(&h, js::now_ms()), None);
        assert!(!host_is_active(&h, js::now_ms(), ACTIVE_WINDOW_MS));
    }

    #[test]
    fn a_stamp_from_the_future_is_now() {
        let h = scratch_home("g");
        let now = js::now_ms();
        note_activity(&h, now + 3_600_000.0);
        assert_eq!(ms_since_activity(&h, now), Some(0.0));
        assert!(host_is_active(&h, now, ACTIVE_WINDOW_MS));
    }

    #[test]
    fn an_unreadable_stamp_is_nobody_there() {
        let h = scratch_home("h");
        std::fs::create_dir_all(js::dirname(&activity_path(&h))).unwrap();
        std::fs::write(activity_path(&h), "not json at all").unwrap();
        assert_eq!(ms_since_activity(&h, js::now_ms()), None);
        assert!(!host_is_active(&h, js::now_ms(), ACTIVE_WINDOW_MS));
    }

    #[test]
    fn the_stamp_is_one_small_object() {
        let h = scratch_home("i");
        note_activity(&h, 1_759_670_000_123.0);
        assert_eq!(std::fs::read_to_string(activity_path(&h)).unwrap(), r#"{"at":1759670000123}"#);
    }
}
