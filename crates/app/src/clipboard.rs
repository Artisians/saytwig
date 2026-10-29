//! A dedicated clipboard owner: bounded requests expire rather than copying later.
use dictation_desktop::{
    native::NativeDesktop, ClipboardPolicy, DeliveryGate, DeliveryMode, DesktopBackend,
    DesktopError, Target,
};
use dictation_domain::Generation;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
enum Operation {
    Copy,
    Paste(Generation, Arc<DeliveryGate>),
}
pub(crate) enum Outcome {
    Copied(bool),
    Pasted(Result<(), DesktopError>),
}
struct Request {
    generation: u64,
    text: String,
    operation: Operation,
    expires: Instant,
}
pub(crate) struct Clipboard {
    requests: mpsc::SyncSender<Request>,
    results: mpsc::Receiver<(u64, Outcome)>,
    gate: Option<Arc<DeliveryGate>>,
    generation: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Clipboard {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (requests, input) = mpsc::sync_channel::<Request>(1);
        let (output, results) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let epoch = generation.clone();
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        let thread = thread::Builder::new()
            .name("saytwig-clipboard".into())
            .spawn(move || {
                let mut desktop = None;
                while running.load(Ordering::Acquire) {
                    let request = match input.recv_timeout(Duration::from_millis(50)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    if !running.load(Ordering::Acquire)
                        || epoch.load(Ordering::Acquire) != request.generation
                    {
                        continue;
                    }
                    if desktop.is_none() {
                        desktop = NativeDesktop::new().ok();
                    }
                    // Check after clipboard initialization: slow desktop setup cannot defer a copy.
                    if !running.load(Ordering::Acquire)
                        || epoch.load(Ordering::Acquire) != request.generation
                    {
                        continue;
                    }
                    if Instant::now() > request.expires {
                        let outcome = match request.operation {
                            Operation::Copy => Outcome::Copied(false),
                            Operation::Paste(..) => Outcome::Pasted(Err(DesktopError::Stale)),
                        };
                        let _ = output.send((request.generation, outcome));
                        continue;
                    }
                    let outcome = match request.operation {
                        Operation::Copy => {
                            Outcome::Copied(desktop.as_mut().is_some_and(|desktop| {
                                desktop.write_text(&request.text, None).is_ok()
                            }))
                        }
                        Operation::Paste(generation, delivery) => Outcome::Pasted(
                            desktop
                                .as_mut()
                                .ok_or(DesktopError::ClipboardUnavailable)
                                .and_then(|desktop| {
                                    delivery
                                        .deliver(
                                            desktop,
                                            generation,
                                            &request.text,
                                            DeliveryMode::Paste,
                                            ClipboardPolicy::KeepNewText,
                                        )
                                        .map(|_| ())
                                }),
                        ),
                    };
                    let _ = output.send((request.generation, outcome));
                }
            })?;
        Ok(Self {
            requests,
            results,
            gate: None,
            generation,
            alive,
            thread: Some(thread),
        })
    }
    pub(crate) fn arm(&mut self, generation: Generation, target: Target) -> bool {
        self.cancel();
        // One gate per dictation: arm before sharing, so the UI cannot contend
        // with an OS commit. Revoked gates are permanently retired.
        let gate = Arc::new(DeliveryGate::default());
        if gate.arm(generation, Some(target)).is_err() {
            return false;
        }
        self.gate = Some(gate);
        true
    }
    pub(crate) fn paste(&self, generation: Generation, text: String) -> bool {
        let Some(gate) = &self.gate else {
            return false;
        };
        self.enqueue(text, Operation::Paste(generation, gate.clone()))
    }
    pub(crate) fn copy(&mut self, text: String) -> bool {
        // Explicit Copy supersedes a not-yet-dispatched automatic delivery.
        self.cancel();
        self.enqueue(text, Operation::Copy)
    }
    fn enqueue(&self, text: String, operation: Operation) -> bool {
        if !self.alive.load(Ordering::Acquire) {
            return false;
        }
        let Ok(previous) =
            self.generation
                .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1)
                })
        else {
            self.alive.store(false, Ordering::Release);
            return false;
        };
        self.requests
            .try_send(Request {
                generation: previous + 1,
                text,
                operation,
                expires: Instant::now() + Duration::from_secs(2),
            })
            .is_ok()
    }
    pub(crate) fn poll(&self) -> Option<Outcome> {
        self.results
            .try_iter()
            .filter(|(generation, _)| *generation == self.generation.load(Ordering::Acquire))
            .map(|(_, copied)| copied)
            .last()
    }
    pub(crate) fn cancel(&mut self) {
        // Never wait on a native clipboard commit from the GPUI event thread.
        // Revocation rejects queued/not-yet-committed effects. An OS commit already
        // entered may finish; cancellation does not recall submitted input.
        if let Some(gate) = self.gate.take() {
            gate.revoke();
        }
        if self
            .generation
            .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .is_err()
        {
            // An exhausted epoch must not leave a queued explicit Copy valid.
            self.alive.store(false, Ordering::Release);
        }
    }
    pub(crate) fn shutdown(&mut self) -> Option<thread::JoinHandle<()>> {
        self.alive.store(false, Ordering::Release);
        self.cancel();
        self.thread.take()
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
