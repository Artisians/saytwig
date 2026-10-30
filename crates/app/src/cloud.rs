//! Explicit account actions and transcript-sharing jobs on an owned runtime thread.
use cleanup_service::{CleanupClient, Model, Permit};
use dictation_domain::Generation;
use saytwig_account::{
    AccountError, AccountManager, CancellationToken, LogoutOutcome, Profile, ProfileId,
};
use std::{sync::mpsc, thread};

pub(crate) enum Command {
    Accounts,
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
    Shutdown,
}
pub(crate) enum Event {
    Profiles(Vec<Profile>),
    Models(ProfileId, Vec<Model>),
    Cleaned(Generation, Option<String>),
    Notice(&'static str),
    Finished,
}
pub(crate) struct Cloud {
    pub(crate) commands: mpsc::Sender<(u64, Command)>,
    pub(crate) events: mpsc::Receiver<(u64, Event)>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Cloud {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (commands, input) = mpsc::channel();
        let (output, events) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("saytwig-accounts".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => {
                        let _ = output.send((0, Event::Notice("account_unavailable")));
                        return;
                    }
                };
                let mut manager = None;
                while let Ok((id, command)) = input.recv() {
                    let output = Output {
                        id,
                        sender: output.clone(),
                    };
                    if matches!(command, Command::Shutdown) {
                        break;
                    }
                    runtime.block_on(async {
                        if manager.is_none() {
                            match AccountManager::open().await {
                                Ok(opened) => manager = Some(opened),
                                Err(error) => {
                                    let _ = output.send(Event::Notice(error_key(error)));
                                    if let Command::Clean { generation, .. } = command {
                                        let _ = output.send(Event::Cleaned(generation, None));
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
        })
    }
    pub(crate) fn shutdown(&mut self) -> Option<thread::JoinHandle<()>> {
        let _ = self.commands.send((0, Command::Shutdown));
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
    sender: mpsc::Sender<(u64, Event)>,
}
impl Output {
    fn send(&self, event: Event) -> Result<(), mpsc::SendError<(u64, Event)>> {
        self.sender.send((self.id, event))
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
            if let Err(error) = outcome {
                let _ = output.send(Event::Notice(error_key(error)));
            }
            profiles(manager, output).await;
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
            if let Err(error) = outcome {
                let _ = output.send(Event::Notice(error_key(error)));
            }
            profiles(manager, output).await;
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
                        tokio::select! {biased;_ = lease.revoked.cancelled()=>None,result=client.clean(&lease.bearer,&model,&original,&permit,&cancel)=>result.ok()}
                    }
                    Err(_) => None,
                },
                _ => None,
            };
            let _ = output.send(Event::Cleaned(generation, result));
            // Send original-text completion first, then reconcile account state
            // with this same request ID so the UI can clear stale model/consent.
            profiles(manager, output).await;
        }
        Command::Shutdown => {}
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
