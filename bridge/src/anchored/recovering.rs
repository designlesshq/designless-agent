//! A diagnosis that can stop being true.
//!
//! The three unavailable outcomes — the app is closed, nobody is signed in, the
//! session cannot be used — all describe the desktop at one instant. The bridge
//! outlives that instant by hours. `DeniedAuth` answers every frame from the
//! startup probe, so a session that began while the app was still coming up, or
//! a second before someone signed in, stayed broken for its whole life, and the
//! only cure was a reconnect nobody thinks to try.
//!
//! The probe runs within a few hundred milliseconds of the desktop reaching
//! ready, which is the worst instant in the app's life to ask it anything, and
//! the host allows tens of seconds for a handshake that used to fail in two.
//! A single sample taken that early, held for hours, is the whole defect.
//!
//! So: keep the diagnosis on the wire, and keep asking.
//!
//! - The FIRST frame is the one whose failure ends the session — it is the
//!   host's connect handshake, and the upstream refuses it without a bearer.
//!   That frame may wait, re-asking, for what is left of the host's budget.
//! - Later frames re-ask no more often than the cooldown.
//! - A REFUSAL is never wrapped in this. `user_denied` is an answer, not a
//!   race, and re-asking re-opens the consent window the user just dismissed.
//!
//! Re-asking is cheap because of how the desktop answers `request_access`
//! (electron/bridge-ipc.js): it reads the credential first and replies
//! `no_session` with no window when nobody is signed in, and replies
//! `access_granted` with no window when the grant is already on file. It opens
//! the consent window in exactly one case — signed in, not yet granted — which
//! is the case where a window is the right answer.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use super::{AnchoredAuth, AnchoredInit, DeniedAuth};
use crate::auth::AuthProvider;
use crate::error::BridgeResult;

type ProbeFuture = Pin<Box<dyn Future<Output = BridgeResult<Option<AnchoredInit>>> + Send>>;
type Probe = Arc<dyn Fn() -> ProbeFuture + Send + Sync>;

/// The most the first frame will wait. Hosts allow tens of seconds for the
/// connect handshake (Claude Code: 30s), and the launcher may already have
/// spent some of that bringing the app up — `first_frame_grace` subtracts what
/// it spent, so the two waits cannot add up past the budget.
const FIRST_FRAME_GRACE_CAP: Duration = Duration::from_secs(8);

/// The slice of the host's connect budget this bridge is willing to occupy,
/// launcher time included. Deliberately short of the 30s Claude Code allows:
/// the remainder covers process spawn, the probe round trips, and the frame
/// itself, and a bridge that eats the whole budget turns a recoverable wait
/// into a connect timeout with no hint at all.
const HOST_CONNECT_ALLOWANCE: Duration = Duration::from_secs(24);

/// Pause between probes inside the first-frame grace.
const PROBE_INTERVAL: Duration = Duration::from_millis(750);

/// Between later frames. Long enough that a stuck session does not ask the
/// desktop once per frame, short enough that someone who signs in mid-session
/// is working again before they think to reconnect anything.
const REPROBE_COOLDOWN: Duration = Duration::from_secs(20);

/// Set by `bin/launch.mjs`: milliseconds it spent detecting, opening and
/// waiting for the desktop app before spawning this process. Absent when the
/// binary is run directly, which is also when nothing has been spent.
const LAUNCH_ELAPSED_ENV: &str = "DESIGNLESS_BRIDGE_LAUNCH_ELAPSED_MS";

/// What is left of the host's connect budget for the first frame to spend.
fn first_frame_grace() -> Duration {
    let spent = std::env::var(LAUNCH_ELAPSED_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_default();
    HOST_CONNECT_ALLOWANCE
        .saturating_sub(spent)
        .min(FIRST_FRAME_GRACE_CAP)
}

struct Recovery {
    /// The diagnosis every frame answers with until the desktop says otherwise.
    /// Replaced whenever a probe returns a different one, so the sentence the
    /// user reads describes the app now and not at startup.
    denied: DeniedAuth,
    /// Set once the desktop says yes. From then on this is a plain
    /// `AnchoredAuth` and every frame reads a fresh token, as it always did.
    healed: Option<AnchoredAuth>,
    /// False until the first frame has spent its grace.
    first_frame_spent: bool,
    /// Earliest next probe.
    next_probe: Instant,
    /// A refusal arrived on a later probe. Stop asking: the next ask would
    /// re-open the window the user just closed.
    refused: bool,
}

/// Auth provider that re-asks the desktop instead of holding one answer for
/// the life of the process. See the module header.
pub struct RecoveringAuth {
    state: Mutex<Recovery>,
    probe: Probe,
}

impl RecoveringAuth {
    /// Wrap a recoverable diagnosis. Never call this with a refusal.
    pub fn new(denied: DeniedAuth) -> Self {
        Self::with_probe(denied, Arc::new(|| Box::pin(super::try_init()) as ProbeFuture))
    }

    fn with_probe(denied: DeniedAuth, probe: Probe) -> Self {
        Self {
            state: Mutex::new(Recovery {
                denied,
                healed: None,
                first_frame_spent: false,
                next_probe: Instant::now(),
                refused: false,
            }),
            probe,
        }
    }
}

/// Run one probe and fold its outcome into the state. Returns true once the
/// desktop has said yes.
async fn probe_once(probe: &Probe, st: &mut Recovery) -> bool {
    match probe().await {
        Ok(Some(AnchoredInit::Ready(a))) => {
            tracing::info!("desktop app became available; anchored auth recovered");
            st.healed = Some(a);
            true
        }
        Ok(Some(AnchoredInit::UserDenied(d))) => {
            tracing::info!("user denied access on a later probe; no longer asking");
            st.denied = d;
            st.refused = true;
            false
        }
        Ok(Some(AnchoredInit::Unavailable(d))) => {
            st.denied = d;
            false
        }
        // Still unavailable, and still for a reason we cannot name. Keep the
        // sentence we already have rather than replacing a specific diagnosis
        // with a vaguer one.
        Ok(None) => false,
        Err(e) => {
            tracing::debug!(error = %e, "recovery probe failed; keeping the current diagnosis");
            false
        }
    }
}

#[async_trait::async_trait]
impl AuthProvider for RecoveringAuth {
    async fn bearer_or_refresh(&self) -> BridgeResult<String> {
        let mut st = self.state.lock().await;

        if st.healed.is_none() && !st.refused {
            if !st.first_frame_spent {
                st.first_frame_spent = true;
                let deadline = Instant::now() + first_frame_grace();
                loop {
                    if probe_once(&self.probe, &mut st).await || st.refused {
                        break;
                    }
                    if Instant::now() >= deadline {
                        tracing::warn!(
                            "desktop app still unavailable after the connect grace; \
                             answering with the recovery hint"
                        );
                        break;
                    }
                    tokio::time::sleep(PROBE_INTERVAL).await;
                }
                st.next_probe = Instant::now() + REPROBE_COOLDOWN;
            } else if Instant::now() >= st.next_probe {
                probe_once(&self.probe, &mut st).await;
                st.next_probe = Instant::now() + REPROBE_COOLDOWN;
            }
        }

        match &st.healed {
            Some(anchored) => anchored.bearer_or_refresh().await,
            None => st.denied.bearer_or_refresh().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BridgeError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A probe that answers from a script and counts how often it was asked.
    fn scripted(
        answers: Vec<Option<AnchoredInit>>,
    ) -> (Probe, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let n = calls.clone();
        let answers = Arc::new(Mutex::new(answers.into_iter()));
        let probe: Probe = Arc::new(move || {
            let n = n.clone();
            let answers = answers.clone();
            Box::pin(async move {
                n.fetch_add(1, Ordering::SeqCst);
                Ok(answers.lock().await.next().flatten())
            }) as ProbeFuture
        });
        (probe, calls)
    }

    fn unavailable() -> DeniedAuth {
        DeniedAuth::not_signed_in("no_session")
    }

    /// THE BUG THIS FILE EXISTS FOR. The desktop said "nobody is signed in" once,
    /// at 350ms old, and the old provider repeated that for the whole session.
    #[tokio::test]
    async fn a_desktop_that_becomes_available_is_noticed() {
        std::env::set_var(LAUNCH_ELAPSED_ENV, "24000"); // no first-frame grace
        let (probe, calls) = scripted(vec![
            Some(AnchoredInit::Ready(AnchoredAuth { user_id: "u1".into() })),
        ]);
        let auth = RecoveringAuth::with_probe(unavailable(), probe);
        // The token read itself still needs a live desktop, so the assertion is
        // that the answer STOPPED being the startup diagnosis — the provider was
        // upgraded rather than answering from memory. Both outcomes prove that:
        // on a machine with the app running the read succeeds, and on one
        // without it the error is a fresh IPC failure, not the sentence this
        // provider was built with.
        //
        // NEVER put the Ok value in a message. It is a live credential, and a
        // failing assertion prints to the terminal and into whatever captures
        // it. The old form ran `unwrap_err()` first and did exactly that.
        match auth.bearer_or_refresh().await {
            Ok(_) => {}
            Err(e) => assert!(
                !matches!(e, BridgeError::NoBearer(ref m) if m.contains("has no signed-in user")),
                "still answering with the startup diagnosis after the desktop said yes: {e}"
            ),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
    }

    /// A refusal is an answer, not a race. Asking again re-opens the window the
    /// user just closed, so it must not happen — not on the first frame, not on
    /// any later one.
    #[tokio::test]
    async fn a_refusal_is_never_asked_again() {
        std::env::set_var(LAUNCH_ELAPSED_ENV, "24000");
        let (probe, calls) = scripted(vec![Some(AnchoredInit::UserDenied(
            DeniedAuth::with_hint("click Disconnect Claude Code"),
        ))]);
        let auth = RecoveringAuth::with_probe(unavailable(), probe);
        for _ in 0..5 {
            let err = auth.bearer_or_refresh().await.unwrap_err();
            assert!(matches!(err, BridgeError::AccessDenied(_)), "got {err}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a refusal was asked again");
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
    }

    /// Frames arrive in bursts. Without the cooldown a stuck session would ask
    /// the desktop once per frame, and in the one case where asking opens a
    /// window, that is a window per frame.
    #[tokio::test]
    async fn later_frames_respect_the_cooldown() {
        std::env::set_var(LAUNCH_ELAPSED_ENV, "24000");
        let (probe, calls) = scripted(vec![None, None, None, None]);
        let auth = RecoveringAuth::with_probe(unavailable(), probe);
        for _ in 0..4 {
            assert!(auth.bearer_or_refresh().await.is_err());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "asked again inside the cooldown");
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
    }

    /// The sentence the user reads must describe the app now. An app that was
    /// closed at startup and is now open-but-signed-out is a different problem
    /// with a different fix.
    #[tokio::test]
    async fn the_diagnosis_follows_the_app() {
        std::env::set_var(LAUNCH_ELAPSED_ENV, "24000");
        let (probe, _) = scripted(vec![Some(AnchoredInit::Unavailable(
            DeniedAuth::not_signed_in("invalid_session"),
        ))]);
        let auth = RecoveringAuth::with_probe(DeniedAuth::app_not_open(), probe);
        let err = auth.bearer_or_refresh().await.unwrap_err();
        match err {
            BridgeError::NoBearer(m) => {
                assert!(m.contains("cannot use"), "stale or wrong diagnosis: {m}")
            }
            other => panic!("expected the fresh diagnosis, got {other}"),
        }
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
    }

    /// The launcher's wait and this one share the host's connect budget. If they
    /// could add up past it the host times out the handshake, and a timeout
    /// carries no hint at all — strictly worse than the error it replaced.
    #[test]
    fn the_two_waits_cannot_outlast_the_host() {
        std::env::set_var(LAUNCH_ELAPSED_ENV, "20000");
        assert_eq!(first_frame_grace(), Duration::from_secs(4));
        std::env::set_var(LAUNCH_ELAPSED_ENV, "30000");
        assert_eq!(first_frame_grace(), Duration::ZERO);
        std::env::set_var(LAUNCH_ELAPSED_ENV, "0");
        assert_eq!(first_frame_grace(), FIRST_FRAME_GRACE_CAP);
        // Run directly, with no launcher: nothing has been spent.
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
        assert_eq!(first_frame_grace(), FIRST_FRAME_GRACE_CAP);
        // Garbage reads as "cannot tell", which here means "spent nothing" —
        // the cap still bounds it, so a bad value cannot lengthen the wait.
        std::env::set_var(LAUNCH_ELAPSED_ENV, "not-a-number");
        assert_eq!(first_frame_grace(), FIRST_FRAME_GRACE_CAP);
        std::env::remove_var(LAUNCH_ELAPSED_ENV);
    }
}
