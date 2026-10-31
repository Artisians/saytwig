//! Explicit account actions and transcript-sharing jobs on an owned runtime thread.
use cleanup_service::{CleanupClient, CleanupError, Model, Permit};
use dictation_domain::Generation;
use saytwig_account::{
    AccountError, AccountManager, CancellationToken, LogoutOutcome, Profile, ProfileId,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

pub(crate) enum Command {
    Accounts,
    ManageUsage(CancellationToken),
    SignIn(Option<ProfileId>, CancellationToken),
    EnablePlanUse(ProfileId, CancellationToken),
    Activate(ProfileId),
    SignOut(ProfileId, CancellationToken),
    Models(CancellationToken),
    Clean {
        generation: Generation,
        original: String,
        model: Model,
        profile: ProfileId,
        permit: Permit,
        cancel: CancellationToken,
    },
}
pub(crate) enum Event {
    Profiles(Vec<Profile>),
    Models(ProfileId, Vec<Model>),
    PlanConnected(ProfileId),
    Cleaned {
        generation: Generation,
        profile: ProfileId,
        result: Result<String, CleanupError>,
    },
    Notice(&'static str),
    Finished,
}
pub(crate) struct Cloud {
    pub(crate) commands: mpsc::SyncSender<(u64, Command)>,
    pub(crate) events: mpsc::Receiver<(u64, Event)>,
    thread: Option<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}
impl Cloud {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (commands, input) = mpsc::sync_channel(2);
        let (output, events) = mpsc::sync_channel(8);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stopping = shutdown.clone();
        let thread = thread::Builder::new()
            .name("saytwig-accounts".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => {
                        let _ = output.try_send((0, Event::Notice("account_unavailable")));
                        return;
                    }
                };
                let mut manager = None;
                while !stopping.load(Ordering::Acquire) {
                    let (id, command) = match input.recv_timeout(Duration::from_millis(100)) {
                        Ok(command) => command,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    if stopping.load(Ordering::Acquire) {
                        break;
                    }
                    let output = Output {
                        id,
                        sender: output.clone(),
                        stopping: stopping.clone(),
                    };
                    if let Command::ManageUsage(cancel) = command {
                        if !cancel.is_cancelled()
                            && open::that_detached("https://chatgpt.com/settings/usage").is_err()
                        {
                            let _ = output.send(Event::Notice("usage_browser_failed"));
                        }
                        let _ = output.send(Event::Finished);
                        continue;
                    }
                    runtime.block_on(async {
                        if manager.is_none() {
                            match AccountManager::open().await {
                                Ok(opened) => manager = Some(opened),
                                Err(error) => {
                                    let _ = output.send(Event::Notice(error_key(error)));
                                    if let Command::Clean {
                                        generation,
                                        profile,
                                        ..
                                    } = command
                                    {
                                        let _ = output.send(Event::Cleaned {
                                            generation,
                                            profile,
                                            result: Err(CleanupError::Credential),
                                        });
                                    }
                                    let _ = output.send(Event::Finished);
                                    return;
                                }
                            }
                        }
                        if let Some(manager) = &manager {
                            perform(manager, command, &output).await;
                        }
                        let _ = output.send(Event::Finished);
                    });
                }
                // Native-vault calls cannot be force-cancelled. Account methods
                // bound their caller wait and retain ownership/locking inside
                // those blocking jobs; ordinary Runtime drop would wait forever
                // for a stuck OS call after the UI has requested shutdown.
                runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            })?;
        Ok(Self {
            commands,
            events,
            thread: Some(thread),
            shutdown,
        })
    }
    pub(crate) fn shutdown(&mut self) -> Option<thread::JoinHandle<()>> {
        self.shutdown.store(true, Ordering::Release);
        self.thread.take()
    }
}
impl Drop for Cloud {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
struct Output {
    id: u64,
    sender: mpsc::SyncSender<(u64, Event)>,
    stopping: Arc<AtomicBool>,
}
impl Output {
    fn send(&self, event: Event) -> Result<(), mpsc::SendError<(u64, Event)>> {
        let mut pending = (self.id, event);
        loop {
            if self.stopping.load(Ordering::Acquire) {
                return Err(mpsc::SendError(pending));
            }
            match self.sender.try_send(pending) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(event)) => return Err(mpsc::SendError(event)),
                Err(mpsc::TrySendError::Full(event)) => pending = event,
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
async fn profiles(manager: &AccountManager, output: &Output) {
    match manager.profiles().await {
        Ok(profiles) => {
            let _ = output.send(Event::Profiles(profiles));
        }
        Err(error) => {
            let _ = output.send(Event::Notice(error_key(error)));
        }
    }
}
async fn perform(manager: &AccountManager, command: Command, output: &Output) {
    match command {
        Command::Accounts => profiles(manager, output).await,
        Command::SignIn(profile, cancel) => {
            let outcome = async {
                let pending = manager.begin_sign_in(profile.as_ref(), &cancel).await?;
                // The authorization URL goes only to the OS opener, never UI text or logs.
                if open::that_detached(pending.authorization_url()).is_err() {
                    cancel.cancel();
                }
                manager.complete_sign_in(pending, &cancel).await
            }
            .await;
            let connected = match outcome {
                Ok(profile) if profile.connected && profile.plan_use_granted => Some(profile.id),
                Ok(_) => None,
                Err(error) => {
                    let _ = output.send(Event::Notice(error_key(error)));
                    None
                }
            };
            profiles(manager, output).await;
            if let Some(profile) = connected {
                let _ = output.send(Event::PlanConnected(profile));
            }
        }
        Command::EnablePlanUse(profile, cancel) => {
            let outcome = async {
                let pending = manager
                    .begin_plan_use_authorization(&profile, &cancel)
                    .await?;
                if open::that_detached(pending.authorization_url()).is_err() {
                    cancel.cancel();
                }
                manager.complete_sign_in(pending, &cancel).await
            }
            .await;
            let connected = match outcome {
                Ok(profile) if profile.connected && profile.plan_use_granted => Some(profile.id),
                Ok(_) => None,
                Err(error) => {
                    let _ = output.send(Event::Notice(error_key(error)));
                    None
                }
            };
            profiles(manager, output).await;
            if let Some(profile) = connected {
                let _ = output.send(Event::PlanConnected(profile));
            }
        }
        Command::Activate(profile) => {
            if let Err(error) = manager.activate(&profile).await {
                let _ = output.send(Event::Notice(error_key(error)));
            }
            profiles(manager, output).await;
        }
        Command::SignOut(profile, cancel) => {
            match manager.sign_out(&profile, &cancel).await {
                Ok(LogoutOutcome::LocalOnly) => {
                    let _ = output.send(Event::Notice("logout_local_only"));
                }
                Err(error) => {
                    let _ = output.send(Event::Notice(error_key(error)));
                }
                _ => {}
            }
            profiles(manager, output).await;
        }
        Command::Models(cancel) => {
            match manager.authorized_bearer(&cancel).await {
                Ok(lease) => match CleanupClient::new() {
                    Ok(client) => {
                        let models = tokio::select! {biased;_ = lease.revoked.cancelled()=>None,result=client.models(&lease.bearer,&cancel)=>result.ok()};
                        let _ = output.send(
                            models
                                .map(|models| Event::Models(lease.profile_id, models))
                                .unwrap_or(Event::Notice("models_unavailable")),
                        );
                    }
                    Err(_) => {
                        let _ = output.send(Event::Notice("models_unavailable"));
                    }
                },
                Err(error) => {
                    let _ = output.send(Event::Notice(error_key(error)));
                }
            };
            // Refresh may change the local session even if catalog loading fails.
            profiles(manager, output).await;
        }
        Command::Clean {
            generation,
            original,
            model,
            profile,
            permit,
            cancel,
        } => {
            let result = match manager.authorized_bearer(&cancel).await {
                Ok(lease) if lease.profile_id == profile => match CleanupClient::new() {
                    Ok(client) => {
                        tokio::select! {biased;_ = lease.revoked.cancelled()=>Err(CleanupError::Cancelled),result=client.clean(&lease.bearer,&model,&original,&permit,&cancel)=>result}
                    }
                    Err(error) => Err(error),
                },
                _ => Err(CleanupError::Credential),
            };
            let _ = output.send(Event::Cleaned {
                generation,
                profile,
                result,
            });
            // Send original-text completion first, then reconcile account state
            // with this same request ID so the UI can clear stale model/consent.
            profiles(manager, output).await;
        }
        Command::ManageUsage(_) => {}
    }
}
fn error_key(error: AccountError) -> &'static str {
    match error {
        AccountError::Vault | AccountError::AlreadyOpen | AccountError::StoredData => {
            "account_store_unavailable"
        }
        AccountError::Cancelled => "account_cancelled",
        AccountError::TimedOut => "account_timed_out",
        AccountError::Denied | AccountError::Permission => "account_permission",
        AccountError::NotConnected | AccountError::Reauthorize => "account_reconnect",
        _ => "account_unavailable",
    }
}
