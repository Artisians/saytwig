//! Explicit, bounded stable-release checks. No startup network or installation.
use saytwig_updates::{CheckResult, Consent, UpdateError};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

pub(crate) fn channel_key() -> &'static str {
    match option_env!("SAYTWIG_BUILD_CHANNEL") {
        Some("stable") => "build_stable",
        Some("nightly") => "build_nightly",
        _ => "build_development",
    }
}
fn comparison_version() -> &'static str {
    if channel_key() == "build_stable" {
        env!("CARGO_PKG_VERSION")
    } else {
        // This is a comparison baseline, NOT our displayed source version.
        // Every stable version is newer than this prerelease: a development
        // 0.1.0 must not be presented as equivalent to a released stable 0.1.0.
        "0.0.0-0"
    }
}
/// Constructed only from saytwig-updates' strict repository/tag/URL validation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct VerifiedRelease {
    pub(crate) version: String,
    url: String,
}
impl VerifiedRelease {
    pub(crate) fn open(&self) -> bool {
        // Only explicit UI clicks call this OS browser opener. Never auto-open.
        open::that_detached(&self.url).is_ok()
    }
}
pub(crate) struct Outcome {
    pub(crate) message: &'static str,
    pub(crate) release: Option<VerifiedRelease>,
}
impl Outcome {
    fn from_result(result: Result<CheckResult, UpdateError>) -> Self {
        let mut release = None;
        let message = match result {
            Ok(CheckResult::Available {
                version,
                release_url,
            }) => {
                release = Some(VerifiedRelease {
                    version,
                    url: release_url,
                });
                if channel_key() == "build_stable" {
                    "update_available"
                } else {
                    "update_available_development"
                }
            }
            Ok(CheckResult::Disabled) => "updates_off",
            Ok(CheckResult::NoPublishedRelease) => "updates_none_published",
            Ok(CheckResult::NoCompatibleRelease) => "updates_none_compatible",
            Ok(CheckResult::UpToDate) if channel_key() == "build_stable" => {
                "updates_current_stable"
            }
            Ok(CheckResult::UpToDate) => "updates_development_unverified",
            Err(UpdateError::RateLimited) => "updates_rate_limited",
            Err(UpdateError::Network) => "updates_network_error",
            Err(UpdateError::Untrusted | UpdateError::TooLarge) => "updates_untrusted",
            Err(UpdateError::CurrentVersion | UpdateError::Unavailable) => "updates_unavailable",
        };
        Self { message, release }
    }
}
struct Job {
    id: u64,
    cancelled: Arc<AtomicBool>,
}
pub(crate) struct Finished {
    pub(crate) outcome: Option<Outcome>,
}
pub(crate) struct Updates {
    requests: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<(u64, Option<Outcome>)>,
    alive: Arc<AtomicBool>,
    sequence: u64,
    active: Option<Job>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Updates {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (requests, input) = mpsc::sync_channel::<Job>(1);
        let (output, results) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        let thread = thread::Builder::new()
            .name("saytwig-release-check".into())
            .spawn(move || {
                while running.load(Ordering::Acquire) {
                    let job = match input.recv_timeout(Duration::from_millis(50)) {
                        Ok(job) => job,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let outcome = if running.load(Ordering::Acquire)
                        && !job.cancelled.load(Ordering::Acquire)
                    {
                        // Existing updater bounds connect to 5s, total request to 15s,
                        // response to 256KiB, rejects redirects, and never downloads assets.
                        let result =
                            saytwig_updates::check_latest(Consent::Enabled, comparison_version());
                        if running.load(Ordering::Acquire) && !job.cancelled.load(Ordering::Acquire)
                        {
                            Some(Outcome::from_result(result))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let _ = output.send((job.id, outcome));
                }
            })?;
        Ok(Self {
            requests,
            results,
            alive,
            sequence: 0,
            active: None,
            thread: Some(thread),
        })
    }
    pub(crate) fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) fn check(&mut self, consent: Consent) -> bool {
        if consent != Consent::Enabled || self.busy() || !self.alive.load(Ordering::Acquire) {
            return false;
        }
        let Some(id) = self.sequence.checked_add(1) else {
            return false;
        };
        self.sequence = id;
        let cancelled = Arc::new(AtomicBool::new(false));
        if self
            .requests
            .try_send(Job {
                id,
                cancelled: cancelled.clone(),
            })
            .is_err()
        {
            return false;
        }
        self.active = Some(Job { id, cancelled });
        true
    }
    pub(crate) fn cancel(&mut self) {
        if let Some(job) = &self.active {
            job.cancelled.store(true, Ordering::Release);
        }
        // Keep busy until terminal acknowledgement; cancellation cannot unsend an
        // already-issued HTTP request or allow overlapping checks. Never queue retries.
    }
    pub(crate) fn poll(&mut self) -> Option<Finished> {
        while let Ok((id, outcome)) = self.results.try_recv() {
            if self.active.as_ref().is_some_and(|job| job.id == id) {
                let outcome = if self
                    .active
                    .as_ref()
                    .is_some_and(|job| job.cancelled.load(Ordering::Acquire))
                {
                    None
                } else {
                    outcome
                };
                self.active = None;
                return Some(Finished { outcome });
            }
        }
        None
    }
    pub(crate) fn shutdown(&mut self) -> Option<thread::JoinHandle<()>> {
        self.alive.store(false, Ordering::Release);
        self.cancel();
        self.thread.take()
    }
}
impl Drop for Updates {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
