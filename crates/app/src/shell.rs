//! Compact native controls backed by real session and capability state.
use crate::cloud::{Cloud, Command as CloudCommand, Event as CloudEvent};
use crate::{
    model::Model,
    strings::t,
    worker::{Command, EngineState, Event, Worker},
};
use cleanup_service::{CleanupError, Consent, Model as CleanupModel};
use dictation_desktop::{
    hotkey::{HotkeyLease, ShortcutSignal},
    native::{foreground_target, NativeHotkeys},
    DesktopError,
};
use dictation_domain::{Generation, Phase, Transcript};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::{DialogAction, DialogFooter},
    input::{Input, InputEvent, InputState},
    Disableable, Selectable, Sizable, WindowExt,
};
use gpui_kit::{
    div, prelude::*, px, rgb, App, Context, Entity, FocusHandle, KeyBinding, Render, Subscription,
    Task, Window,
};
use phonon_service::Cancellation;
use saytwig_account::{CancellationToken, Profile, ProfileId};
use std::{sync::Arc, time::Duration};

const PLAN_NOTICE_KEY: &str = "chatgpt_plan_notice_seen_v1";
const HISTORY_PAGE_SIZE: usize = 40;
const HISTORY_DETAIL_CHARS: usize = 4096;

#[derive(Default)]
struct HistorySearch {
    text: gpui_kit::SharedString,
    query: Vec<char>,
    prefix: Vec<usize>,
    too_long: bool,
}
impl HistorySearch {
    fn update(&mut self, text: gpui_kit::SharedString) -> bool {
        if self.text == text {
            return false;
        }
        self.too_long = text.chars().take(513).count() > 512;
        self.query = if self.too_long {
            Vec::new()
        } else {
            text.chars().flat_map(char::to_lowercase).collect()
        };
        self.prefix = vec![0; self.query.len()];
        for index in 1..self.query.len() {
            let mut matched = self.prefix[index - 1];
            while matched > 0 && self.query[index] != self.query[matched] {
                matched = self.prefix[matched - 1];
            }
            if self.query[index] == self.query[matched] {
                matched += 1;
            }
            self.prefix[index] = matched;
        }
        self.text = text;
        true
    }
    fn matches(&self, text: &str) -> bool {
        if self.too_long {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }
        // Stream case folding instead of allocating a lowercase copy of each entry.
        let mut matched = 0;
        for character in text.chars().flat_map(char::to_lowercase) {
            while matched > 0 && character != self.query[matched] {
                matched = self.prefix[matched - 1];
            }
            if character == self.query[matched] {
                matched += 1;
            }
            if matched == self.query.len() {
                return true;
            }
        }
        false
    }
}

fn text_preview(text: &str, limit: usize) -> (String, bool) {
    let mut characters = text.chars();
    let mut preview: String = characters.by_ref().take(limit).collect();
    let shortened = characters.next().is_some();
    if shortened {
        preview.push('…');
    }
    (preview, shortened)
}

gpui_kit::actions!(saytwig, [CancelDictation]);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Dictation,
    History,
    Preferences,
    Cleanup,
}

pub(crate) struct Shell {
    model: Model,
    mark: Arc<gpui_kit::Image>,
    history_page: usize,
    history_search: HistorySearch,
    clipboard: Option<crate::clipboard::Clipboard>,
    cloud: Option<Cloud>,
    updates: Option<crate::updates::Updates>,
    update_consent: saytwig_updates::Consent,
    update_message: &'static str,
    update_release: Option<crate::updates::VerifiedRelease>,
    profiles: Vec<Profile>,
    plan_welcome: Option<ProfileId>,
    plan_limited: bool,
    cleanup_models: Vec<CleanupModel>,
    cleanup_model: Option<CleanupModel>,
    cleanup_profile: Option<ProfileId>,
    consent: Consent,
    account_busy: bool,
    cloud_sequence: u64,
    cloud_active: Option<u64>,
    account_cancel: Option<CancellationToken>,
    cleanup_cancel: Option<(Generation, CancellationToken)>,
    cleaning_sent: Option<Generation>,
    worker: Option<Worker>,
    page: Page,
    cancellation: Option<Cancellation>,
    delivery_generation: Option<Generation>,
    capture_cancel: Option<(Generation, dictation_audio::CaptureCancellation)>,
    shortcut: Entity<InputState>,
    vocabulary: Entity<InputState>,
    search: Entity<InputState>,
    hotkey: Option<HotkeyLease<NativeHotkeys>>,
    overlay: Option<(gpui_kit::AnyWindowHandle, Entity<RecordingPill>)>,
    selected: Option<usize>,
    show_original: bool,
    setup_help: bool,
    dictation_help: bool,
    sync_inputs: bool,
    restore_hotkey: bool,
    overlay_attempted: bool,
    focus: FocusHandle,
    _poll: Task<()>,
    _subscriptions: Vec<Subscription>,
}
impl Shell {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shortcut =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("shortcut_placeholder")));
        let vocabulary =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("vocabulary_placeholder")));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(t("history")));
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&search, |this, input, _: &InputEvent, cx| {
            if this.history_search.update(input.read(cx).value()) {
                this.history_page = 0;
            }
            cx.notify();
        }));
        subscriptions.push(cx.on_app_quit(|this, cx| {
            this.cancel();
            this.consent.set_enabled(false);
            if let Some(cancel) = this.account_cancel.take() {
                cancel.cancel();
            }
            let clipboard = this
                .clipboard
                .as_mut()
                .and_then(crate::clipboard::Clipboard::shutdown);
            let updates = this
                .updates
                .as_mut()
                .and_then(crate::updates::Updates::shutdown);
            let cloud = this.cloud.as_mut().and_then(Cloud::shutdown);
            let worker = this.worker.as_mut().and_then(Worker::shutdown);
            cx.background_executor().spawn(async move {
                if let Some(worker) = worker {
                    let _ = worker.join();
                }
                if let Some(clipboard) = clipboard {
                    let _ = clipboard.join();
                }
                if let Some(cloud) = cloud {
                    let _ = cloud.join();
                }
                if let Some(updates) = updates {
                    let _ = updates.join();
                }
            })
        }));
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                break;
            }
        });
        let worker = Worker::start().ok();
        let mut model = Model::default();
        if worker.is_none() {
            model.notice = Some("worker_failed");
            model.engine = EngineState::Failed;
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            model,
            mark: Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Svg,
                include_bytes!("../../../assets/saytwig.svg").to_vec(),
            )),
            history_page: 0,
            history_search: HistorySearch::default(),
            clipboard: crate::clipboard::Clipboard::start().ok(),
            cloud: Cloud::start().ok(),
            updates: crate::updates::Updates::start().ok(),
            update_consent: saytwig_updates::Consent::Disabled,
            update_message: "updates_off",
            update_release: None,
            profiles: Vec::new(),
            plan_welcome: None,
            plan_limited: false,
            cleanup_models: Vec::new(),
            cleanup_model: None,
            cleanup_profile: None,
            consent: Consent::default(),
            account_busy: false,
            cloud_sequence: 0,
            cloud_active: None,
            account_cancel: None,
            cleanup_cancel: None,
            cleaning_sent: None,
            worker,
            page: Page::Dictation,
            cancellation: None,
            capture_cancel: None,
            delivery_generation: None,
            shortcut,
            vocabulary,
            search,
            hotkey: None,
            overlay: None,
            selected: None,
            show_original: false,
            setup_help: false,
            dictation_help: false,
            sync_inputs: true,
            restore_hotkey: false,
            overlay_attempted: false,
            focus,
            _poll: poll,
            _subscriptions: subscriptions,
        }
    }
    fn send(&mut self, command: Command) {
        let result = match &self.worker {
            Some(worker) => worker.commands.send(command),
            None => Err(std::sync::mpsc::TrySendError::Disconnected(command)),
        };
        let Err(error) = result else { return };
        // Recover the owned payload without logging private settings or text.
        let (command, notice) = match error {
            std::sync::mpsc::TrySendError::Full(command) => (command, "worker_busy"),
            std::sync::mpsc::TrySendError::Disconnected(command) => (command, "worker_failed"),
        };
        match command {
            Command::Start { cancellation, .. } | Command::Install(cancellation) => {
                cancellation.cancel();
                self.cancel();
                self.model.installing = false;
                self.model.install_cancelling = false;
                self.model.setup_progress = None;
                self.model.notice = Some(notice);
            }
            Command::Stop => {
                // A disconnected actor must not leave the capture running.
                self.cancel();
                self.model.notice = Some(notice);
            }
            Command::SaveTranscript { .. } => {
                // The accepted final transcript stays in model.last for Copy.
                self.model.notice = Some("history_not_saved");
            }
            Command::SaveSettings(_) => self.model.notice = Some("settings_not_saved"),
            _ => self.model.notice = Some(notice),
        }
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        // Consume the shortcut edge before applying completed inference events.
        // A new request must revoke a queued prior delivery before final results run.
        let pressed = self
            .hotkey
            .as_mut()
            .is_some_and(|hotkey| hotkey.poll() == Some(ShortcutSignal::Pressed));
        if pressed {
            self.toggle_recording(true);
        }
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|worker| worker.events.try_iter().take(64).collect())
            .unwrap_or_default();
        let mut changed = pressed || !events.is_empty();
        for event in events {
            if matches!(&event, Event::Notice("setup_recovery_required")) {
                self.setup_help = true;
            }
            if matches!(&event, Event::Loaded(..) | Event::History(_)) {
                self.history_page = 0;
            }
            match &event {
                Event::Recording(generation, cancel) => {
                    if self.model.session.generation() == Some(*generation)
                        && self.model.session.phase() == Phase::Preparing
                    {
                        self.capture_cancel = Some((*generation, cancel.clone()));
                    } else {
                        cancel.cancel();
                    }
                }
                Event::Failed(generation, _)
                    if self
                        .capture_cancel
                        .as_ref()
                        .is_some_and(|(owner, _)| owner == generation) =>
                {
                    if let Some((_, cancel)) = self.capture_cancel.take() {
                        cancel.cancel();
                    }
                }
                _ => {}
            }
            if let Event::Loaded(settings, _) = &event {
                self.sync_inputs = true;
                self.restore_hotkey = settings
                    .extra
                    .get("shortcut_enabled")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
            }
            if let Event::Failed(generation, _) = &event {
                if self.delivery_generation == Some(*generation) {
                    self.cancel_delivery();
                }
            }
            if let Some((generation, result)) = self.model.apply(event) {
                self.complete_result(generation, result);
            }
        }
        let cloud_events: Vec<_> = self
            .cloud
            .as_ref()
            .map(|cloud| cloud.events.try_iter().take(32).collect())
            .unwrap_or_default();
        changed |= !cloud_events.is_empty();
        for (request, event) in cloud_events {
            if !matches!(event, CloudEvent::Cleaned { .. }) && self.cloud_active != Some(request) {
                continue;
            }
            match event {
                CloudEvent::Profiles(profiles) => {
                    let old = self
                        .profiles
                        .iter()
                        .find(|p| p.active && p.connected)
                        .map(|p| (&p.id, p.plan_use_granted));
                    let new = profiles
                        .iter()
                        .find(|p| p.active && p.connected)
                        .map(|p| (&p.id, p.plan_use_granted));
                    if old != new {
                        self.plan_limited = false;
                        self.plan_welcome = None;
                        self.disable_cleanup();
                        self.cleanup_models.clear();
                        self.cleanup_model = None;
                        self.cleanup_profile = None;
                    }
                    self.profiles = profiles;
                }
                CloudEvent::Models(profile, models) => {
                    if self
                        .profiles
                        .iter()
                        .any(|p| p.active && p.connected && p.plan_use_granted && p.id == profile)
                    {
                        self.cleanup_models = models;
                        self.cleanup_model = None;
                        self.cleanup_profile = Some(profile);
                    }
                }
                CloudEvent::PlanConnected(profile) => {
                    if !self.plan_notice_seen() {
                        self.plan_welcome = Some(profile);
                    }
                }
                CloudEvent::Notice(notice) => self.model.notice = Some(notice),
                CloudEvent::Finished => {
                    self.account_busy = false;
                    self.account_cancel = None;
                }
                CloudEvent::Cleaned {
                    generation,
                    profile,
                    result,
                } => {
                    let limited = result == Err(CleanupError::UsageLimit)
                        && self.cleanup_profile.as_ref() == Some(&profile)
                        && self.profiles.iter().any(|p| {
                            p.id == profile && p.active && p.connected && p.plan_use_granted
                        });
                    let failed = result.is_err();
                    let cleaned = result.ok();
                    if self
                        .model
                        .session
                        .cleanup_completed(generation, cleaned)
                        .is_ok()
                    {
                        if let Some(result) = self.model.session.take_transcript() {
                            self.complete_result(generation, result);
                        }
                        if limited {
                            self.disable_cleanup();
                            self.plan_limited = true;
                            self.model.notice = Some("plan_usage_limited");
                        } else if failed {
                            self.model.notice = Some("cleanup_failed");
                        }
                    }
                    if self
                        .cleanup_cancel
                        .as_ref()
                        .is_some_and(|(owner, _)| *owner == generation)
                    {
                        self.cleanup_cancel = None;
                    }
                }
            }
        }
        if self.model.session.phase() == Phase::Cleaning {
            if let Some(generation) = self
                .model
                .session
                .generation()
                .filter(|generation| self.cleaning_sent != Some(*generation))
            {
                self.cleaning_sent = Some(generation);
                let request = self
                    .consent
                    .permit()
                    .zip(self.cleanup_model.clone().zip(self.cleanup_profile.clone()))
                    .zip(self.model.session.transcript().cloned());
                if let Some(((permit, (model, profile)), transcript)) = request {
                    let cancel = CancellationToken::new();
                    self.cleanup_cancel = Some((generation, cancel.clone()));
                    self.send_cloud(CloudCommand::Clean {
                        generation,
                        original: transcript.original,
                        model,
                        profile,
                        permit,
                        cancel,
                    });
                } else {
                    let _ = self.model.session.cleanup_completed(generation, None);
                    if let Some(result) = self.model.session.take_transcript() {
                        self.complete_result(generation, result);
                    }
                }
            }
        }
        if let Some(result) = self
            .clipboard
            .as_ref()
            .and_then(crate::clipboard::Clipboard::poll)
        {
            self.model.notice = Some(match result {
                crate::clipboard::Outcome::Copied(true) => "copied",
                crate::clipboard::Outcome::Copied(false) => "copy_failed",
                crate::clipboard::Outcome::Pasted(Ok(())) => "paste_dispatched",
                crate::clipboard::Outcome::Pasted(Err(DesktopError::FocusChanged)) => {
                    "paste_focus_changed"
                }
                crate::clipboard::Outcome::Pasted(Err(DesktopError::ModifiersHeld)) => {
                    "paste_modifiers_held"
                }
                crate::clipboard::Outcome::Pasted(Err(DesktopError::InputPartiallyDispatched)) => {
                    "paste_partial"
                }
                crate::clipboard::Outcome::Pasted(Err(_)) => "paste_failed",
            });
            changed = true;
        }
        if let Some(finished) = self
            .updates
            .as_mut()
            .and_then(crate::updates::Updates::poll)
        {
            if let Some(outcome) = finished.outcome {
                self.update_message = outcome.message;
                self.update_release = outcome.release;
            }
            changed = true;
        }
        self.sync_overlay(cx);
        if changed {
            cx.notify();
        }
    }
    fn complete_result(&mut self, generation: Generation, result: Transcript) {
        self.capture_cancel = None;
        if result.output().trim().is_empty() {
            self.cancel_delivery();
            self.cancellation = None;
            return;
        }
        if self.delivery_generation == Some(generation) {
            self.delivery_generation = None;
            if self
                .clipboard
                .as_ref()
                .is_none_or(|clipboard| !clipboard.paste(generation, result.output().to_owned()))
            {
                self.model.notice = Some("paste_failed");
                self.cancel_delivery();
            }
        }
        self.model.last = Some(result.clone());
        self.send(Command::SaveTranscript {
            original: result.original,
            cleaned: result.cleaned,
        });
        self.cancellation = None;
    }
    fn reject_cloud(&mut self, command: CloudCommand) {
        match command {
            CloudCommand::Clean {
                generation, cancel, ..
            } => {
                cancel.cancel();
                if self
                    .model
                    .session
                    .cleanup_completed(generation, None)
                    .is_ok()
                {
                    if let Some(result) = self.model.session.take_transcript() {
                        self.complete_result(generation, result);
                    }
                    self.model.notice = Some("cleanup_failed");
                }
                if self
                    .cleanup_cancel
                    .as_ref()
                    .is_some_and(|(owner, _)| *owner == generation)
                {
                    self.cleanup_cancel = None;
                }
            }
            CloudCommand::SignIn(_, cancel)
            | CloudCommand::EnablePlanUse(_, cancel)
            | CloudCommand::SignOut(_, cancel)
            | CloudCommand::Models(cancel)
            | CloudCommand::ManageUsage(cancel) => cancel.cancel(),
            CloudCommand::Accounts | CloudCommand::Activate(_) => {}
        }
    }
    fn send_cloud(&mut self, command: CloudCommand) {
        if self.account_busy {
            self.reject_cloud(command);
            return;
        }
        let Some(request) = self.cloud_sequence.checked_add(1) else {
            self.model.notice = Some("account_unavailable");
            self.reject_cloud(command);
            return;
        };
        let cancel = match &command {
            CloudCommand::SignIn(_, cancel)
            | CloudCommand::EnablePlanUse(_, cancel)
            | CloudCommand::SignOut(_, cancel)
            | CloudCommand::Models(cancel)
            | CloudCommand::ManageUsage(cancel) => Some(cancel.clone()),
            CloudCommand::Accounts | CloudCommand::Activate(_) | CloudCommand::Clean { .. } => None,
        };
        let outcome = match &self.cloud {
            Some(cloud) => cloud.commands.try_send((request, command)),
            None => Err(std::sync::mpsc::TrySendError::Disconnected((
                request, command,
            ))),
        };
        match outcome {
            Ok(()) => {
                self.account_cancel = cancel;
                self.cloud_sequence = request;
                self.cloud_active = Some(request);
                self.account_busy = true;
            }
            Err(
                std::sync::mpsc::TrySendError::Full((_, command))
                | std::sync::mpsc::TrySendError::Disconnected((_, command)),
            ) => {
                self.model.notice = Some("account_unavailable");
                self.reject_cloud(command);
            }
        }
    }
    fn plan_notice_seen(&self) -> bool {
        self.model
            .settings
            .extra
            .get(PLAN_NOTICE_KEY)
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    }
    fn manage_usage(&mut self) {
        if self.account_busy {
            return;
        }
        let cancel = CancellationToken::new();

        self.send_cloud(CloudCommand::ManageUsage(cancel));
    }
    fn show_plan_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.settings_loaded {
            return;
        }
        let Some(profile) = self.plan_welcome.take() else {
            return;
        };
        let parent = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let Some(shell) = parent.upgrade() else {
                return;
            };
            let current = shell.read(cx);
            if current.plan_notice_seen()
                || !current
                    .profiles
                    .iter()
                    .any(|p| p.id == profile && p.active && p.connected && p.plan_use_granted)
            {
                return;
            }
            window.open_dialog(cx, move |dialog, _, _| {
                let parent = parent.clone();
                dialog
                    .title(t("plan_welcome_title"))
                    .child(div().text_sm().child(t("plan_welcome_body")))
                    .footer(
                        DialogFooter::new()
                            .child(DialogAction::new().child(
                                Button::new("plan-welcome-ok").primary().label(t("got_it")),
                            )),
                    )
                    .on_close(move |_, _, cx| {
                        let _ = parent.update(cx, |this, cx| {
                            this.model
                                .settings
                                .extra
                                .insert(PLAN_NOTICE_KEY.into(), true.into());
                            this.send(Command::SaveSettings(this.model.settings.clone()));
                            cx.notify();
                        });
                    })
            });
        });
    }
    fn disable_cleanup(&mut self) {
        self.consent.set_enabled(false);
        if let Some((_, cancel)) = self.cleanup_cancel.take() {
            cancel.cancel();
        }
        let generation = self.model.session.generation();
        self.model.session.set_cleanup_enabled(false);
        if let Some((generation, result)) = generation.zip(self.model.session.take_transcript()) {
            self.complete_result(generation, result);
        }
    }
    fn sync_overlay(&mut self, cx: &mut Context<Self>) {
        if self.model.active() {
            let Some(generation) = self.model.session.generation() else {
                return;
            };
            let phase = self.model.session.phase();
            if let Some((_, view)) = &self.overlay {
                view.update(cx, |pill, cx| {
                    if pill.phase != phase || pill.generation != generation {
                        pill.phase = phase;
                        pill.generation = generation;
                        cx.notify();
                    }
                });
                return;
            }
            let mut bounds =
                gpui_kit::Bounds::centered(None, gpui_kit::size(px(340.), px(52.)), cx);
            if let Some(display) = cx.primary_display() {
                bounds.origin.y = display.bounds().bottom() - px(100.);
            }
            if self.overlay_attempted {
                return;
            }
            self.overlay_attempted = true;
            let parent = cx.weak_entity();
            let options = gpui_kit::WindowOptions {
                window_bounds: Some(gpui_kit::WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                is_resizable: false,
                is_minimizable: false,
                kind: gpui_kit::WindowKind::PopUp,
                app_id: Some("com.saytwig.desktop".into()),
                ..Default::default()
            };
            match gpui_kit::open_window(options, cx, move |_, cx| {
                cx.new(|_| RecordingPill {
                    parent,
                    phase,
                    generation,
                })
            }) {
                Ok(window) => self.overlay = Some(window),
                Err(_) => self.model.notice = Some("overlay_unavailable"),
            }
        } else if let Some((window, _)) = self.overlay.take() {
            self.overlay_attempted = false;
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    }
    fn toggle_recording(&mut self, from_shortcut: bool) {
        if self.model.session.phase() == Phase::Recording {
            self.send(Command::Stop);
            return;
        }
        if !self.model.can_start() {
            return;
        }
        // Capture before opening/updating any overlay; never activate a target.
        let target = from_shortcut.then(foreground_target).and_then(Result::ok);
        self.cancel_delivery();
        match self.model.session.start() {
            Ok(generation) => {
                let cancellation = Cancellation::new();
                self.cancellation = Some(cancellation.clone());
                self.model.notice = None;
                if let Some(target) = target {
                    if self
                        .clipboard
                        .as_mut()
                        .is_some_and(|clipboard| clipboard.arm(generation, target))
                    {
                        self.delivery_generation = Some(generation);
                    } else {
                        self.model.notice = Some("paste_failed");
                    }
                } else if from_shortcut {
                    self.model.notice = Some("paste_unavailable");
                }
                self.overlay_attempted = false;
                self.send(Command::Start {
                    generation,
                    cancellation,
                    settings: self.model.settings.clone(),
                });
            }
            Err(_) => self.model.notice = Some("worker_failed"),
        }
    }
    fn cancel_delivery(&mut self) {
        self.delivery_generation = None;
        if let Some(clipboard) = &mut self.clipboard {
            clipboard.cancel();
        }
    }
    pub(crate) fn cancel(&mut self) {
        if self.model.installing {
            self.model.install_cancelling = true;
        }
        self.cancel_delivery();
        if let Some((_, cancel)) = self.capture_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        if let Some((_, cancel)) = self.cleanup_cancel.take() {
            cancel.cancel();
        }
        self.model.session.cancel();
        self.send(Command::Cancel);
    }
    fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.delivery_generation = None;
        if self
            .clipboard
            .as_mut()
            .is_none_or(|clipboard| !clipboard.copy(text))
        {
            self.model.notice = Some("copy_failed");
        }
        cx.notify();
    }
    fn set_hotkey(&mut self, cx: &mut Context<Self>) {
        let value = self.shortcut.read(cx).value();
        let value = if value.trim().is_empty() {
            "Control+Space"
        } else {
            value.as_ref()
        };
        if self.hotkey.is_none() {
            self.hotkey = NativeHotkeys::new().ok().map(HotkeyLease::new);
        }
        let bound = self
            .hotkey
            .as_mut()
            .is_some_and(|hotkey| hotkey.bind(value).is_ok());
        self.model.notice = Some(if bound {
            "shortcut_active"
        } else {
            "shortcut_unavailable"
        });
        if bound {
            self.model
                .settings
                .extra
                .insert("shortcut_enabled".into(), serde_json::Value::Bool(true));
            self.model
                .settings
                .extra
                .insert("shortcut".into(), serde_json::Value::String(value.into()));
            self.send(Command::SaveSettings(self.model.settings.clone()));
        }
        cx.notify();
    }
    fn phase_label(&self) -> &'static str {
        t(match self.model.session.phase() {
            Phase::Preparing => "preparing",
            Phase::Recording => "recording",
            Phase::Transcribing => "transcribing",
            Phase::Cleaning => "cleaning",
            _ => "idle",
        })
    }
    fn engine_label(&self) -> &'static str {
        t(match self.model.engine {
            EngineState::Checking => "checking",
            EngineState::MissingRuntime => "missing_runtime",
            EngineState::MissingModel => "missing_model",
            EngineState::Installed => "installed",
            EngineState::Ready => "ready",
            EngineState::Failed => "engine_failed",
        })
    }
    fn navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .pb_2()
            .child(gpui_kit::img(self.mark.clone()).w(px(28.)).h(px(28.)))
            .child(
                div()
                    .mr_4()
                    .text_size(px(16.))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child("Saytwig"),
            )
            .children(
                [
                    (Page::Dictation, "dictation"),
                    (Page::History, "history"),
                    (Page::Preferences, "preferences"),
                ]
                .into_iter()
                .map(|(page, key)| {
                    Button::new(key)
                        .ghost()
                        .small()
                        .label(t(key))
                        .selected(self.page == page)
                        .toggled(self.page == page)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.page = page;
                            cx.notify();
                        }))
                }),
            )
            .child(div().flex_1())
            .child(div().text_xs().text_color(rgb(0xb3b3b3)).child(t("local")))
    }
    fn dictation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.model.last.as_ref().map(|result| {
            if self.show_original {
                result.original.clone()
            } else {
                result.output().to_owned()
            }
        });
        let preparing = self.model.active() && self.model.session.phase() != Phase::Recording;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap_4()
            .child(
                div()
                    .id("transcript")
                    .flex_1()
                    .min_h(px(110.))
                    .overflow_y_scroll()
                    .text_size(px(if text.is_none() { 18. } else { 22. }))
                    .text_color(rgb(if text.is_none() { 0x777777 } else { 0xf2f2f2 }))
                    .child(
                        text.clone()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| t("empty_title").into()),
                    ),
            )
            .when(text.as_ref().is_some_and(|text| !text.is_empty()), |el| {
                el.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("copy-result")
                                .ghost()
                                .label(t("copy"))
                                .disabled(text.as_ref().is_none_or(String::is_empty))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(result) = &this.model.last {
                                        this.copy(
                                            if this.show_original {
                                                result.original.clone()
                                            } else {
                                                result.output().to_owned()
                                            },
                                            cx,
                                        );
                                    }
                                })),
                        )
                        .when(
                            self.model
                                .last
                                .as_ref()
                                .is_some_and(|r| r.cleaned.is_some()),
                            |el| {
                                el.child(
                                    Button::new("toggle-original")
                                        .ghost()
                                        .label(t(if self.show_original {
                                            "view_output"
                                        } else {
                                            "view_original"
                                        }))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_original = !this.show_original;
                                            cx.notify();
                                        })),
                                )
                            },
                        ),
                )
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(0x303030))
                    .pt_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        Button::new("record")
                            .primary()
                            .tooltip(t("record_help"))
                            .label(t(if self.model.session.phase() == Phase::Recording {
                                "stop"
                            } else {
                                "start"
                            }))
                            .disabled(
                                preparing
                                    || (!self.model.can_start()
                                        && self.model.session.phase() != Phase::Recording),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_recording(false);
                                cx.notify();
                            })),
                    )
                    .when(self.model.active(), |el| {
                        el.child(
                            Button::new("cancel-recording")
                                .ghost()
                                .label(t("cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel();
                                    this.model.notice = Some("cancelled");
                                    cx.notify();
                                })),
                        )
                    })
                    .child(div().text_sm().text_color(rgb(0xb3b3b3)).child(
                        if self.model.active() {
                            self.phase_label().to_owned()
                        } else if self.hotkey.as_ref().is_some_and(HotkeyLease::is_bound) {
                            self.model
                                .settings
                                .extra
                                .get("shortcut")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Control+Space")
                                .into()
                        } else {
                            t("shortcut_inactive").into()
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("microphone-preferences")
                            .ghost()
                            .small()
                            .label(
                                self.model
                                    .settings
                                    .input_device
                                    .clone()
                                    .unwrap_or_else(|| t("system_microphone").into()),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page = Page::Preferences;
                                if !this.model.active() {
                                    this.send(Command::ListDevices);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("cleanup-settings")
                            .ghost()
                            .small()
                            .label(t(if self.model.session.cleanup_enabled() {
                                "cleanup_enabled"
                            } else {
                                "cleanup_off"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page = Page::Cleanup;
                                cx.notify();
                            })),
                    ),
            )
    }
    fn begin_install(&mut self) {
        if self.model.active() || self.model.installing {
            return;
        }
        let cancellation = Cancellation::new();
        self.cancellation = Some(cancellation.clone());
        self.model.installing = true;
        self.model.install_cancelling = false;
        self.model.setup_progress = None;
        self.model.notice = None;
        self.send(Command::Install(cancellation));
    }
    fn setup_status(&self) -> String {
        if self.model.install_cancelling {
            return t("setup_cancelling").into();
        }
        if !self.model.installing {
            return self.engine_label().into();
        }
        match self.model.setup_progress.as_ref() {
            Some(phonon_service::SetupProgress::Downloading {
                completed_assets,
                total_assets,
            }) => {
                format!(
                    "{} {}/{}",
                    t("setup_downloading"),
                    completed_assets,
                    total_assets
                )
            }
            Some(phonon_service::SetupProgress::Preparing) => t("setup_preparing").into(),
            Some(phonon_service::SetupProgress::Verifying) => t("setup_verifying").into(),
            Some(phonon_service::SetupProgress::Ready) => t("setup_model").into(),
            None => t("setup_starting").into(),
        }
    }
    fn setup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let needs_runtime = self.model.engine == EngineState::MissingRuntime;
        let native_setup = cfg!(all(target_os = "windows", target_arch = "x86_64"));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .py_2()
            .border_b_1()
            .border_color(rgb(0x303030))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(div().flex_1().text_sm().child(self.setup_status()))
                    .child(
                        Button::new("setup-help")
                            .ghost()
                            .small()
                            .label(t(if needs_runtime && native_setup {
                                "setup_review"
                            } else {
                                "setup_details"
                            }))
                            .selected(self.setup_help)
                            .toggled(self.setup_help)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.setup_help = !this.setup_help;
                                cx.notify();
                            })),
                    )
                    .when(self.model.engine == EngineState::MissingModel, |el| {
                        el.child(
                            Button::new("install-model")
                                .primary()
                                .label(t("install"))
                                .disabled(self.model.installing || self.model.active())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.begin_install();
                                    cx.notify();
                                })),
                        )
                    })
                    .when(self.model.installing, |el| {
                        el.child(
                            Button::new("cancel-install")
                                .ghost()
                                .label(t("cancel"))
                                .disabled(self.model.install_cancelling)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new("retry-installation")
                            .ghost()
                            .label(t("retry"))
                            .disabled(self.model.installing || self.model.active())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send(Command::Reload);
                                cx.notify();
                            })),
                    ),
            )
            .when(self.setup_help, |el| {
                if needs_runtime && native_setup {
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child(t("setup_download_details"))
                            .child(t("setup_sources"))
                            .child(t("setup_disk"))
                            .child(t("setup_prerequisite"))
                            .when(self.model.notice == Some("setup_recovery_required"), |el| {
                                el.child(t("setup_recovery_location"))
                                    .child(t("setup_recovery_instructions"))
                            })
                            .child(
                                Button::new("download-runtime-model")
                                    .primary()
                                    .label(t("setup_download"))
                                    .disabled(self.model.installing || self.model.active())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.begin_install();
                                        cx.notify();
                                    })),
                            ),
                    )
                } else {
                    el.child(div().text_sm().text_color(rgb(0xb3b3b3)).child(t(
                        if self.model.engine == EngineState::MissingModel {
                            "install_note"
                        } else if needs_runtime {
                            "setup_unsupported"
                        } else {
                            "runtime_missing"
                        },
                    )))
                }
            })
    }
    fn history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut matches = self
            .model
            .history
            .entries
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, entry)| self.history_search.matches(&entry.original))
            .skip(self.history_page.saturating_mul(HISTORY_PAGE_SIZE));
        let rows: Vec<_> = matches.by_ref().take(HISTORY_PAGE_SIZE).collect();
        let has_next = matches.next().is_some();
        let chosen = self
            .selected
            .and_then(|index| self.model.history.entries.get(index));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap_3()
            .child(Input::new(&self.search).aria_label(t("history")))
            .when(self.history_search.too_long, |el| {
                el.child(t("history_search_limit"))
            })
            .when(rows.is_empty() && !self.history_search.too_long, |el| {
                el.child(
                    div()
                        .py_8()
                        .child(t(if self.model.history.entries.is_empty() {
                            "no_history"
                        } else {
                            "history_no_matches"
                        })),
                )
            })
            .child(
                div()
                    .id("history-list")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h(px(90.))
                    .children(rows.into_iter().map(|(index, entry)| {
                        let (preview, _) =
                            text_preview(entry.cleaned.as_deref().unwrap_or(&entry.original), 100);
                        Button::new(("history-entry", index))
                            .ghost()
                            .label(preview)
                            .w_full()
                            .selected(self.selected == Some(index))
                            .toggled(self.selected == Some(index))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected = Some(index);
                                cx.notify();
                            }))
                    })),
            )
            .when(self.history_page > 0 || has_next, |el| {
                el.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new("history-previous")
                                .ghost()
                                .small()
                                .label(t("previous"))
                                .disabled(self.history_page == 0)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history_page = this.history_page.saturating_sub(1);
                                    cx.notify();
                                })),
                        )
                        .child(div().text_sm().child(format!(
                            "{} {}",
                            t("page"),
                            self.history_page.saturating_add(1)
                        )))
                        .child(
                            Button::new("history-next")
                                .ghost()
                                .small()
                                .label(t("next"))
                                .disabled(!has_next)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history_page = this.history_page.saturating_add(1);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(chosen, |el, entry| {
                let text = if self.show_original {
                    &entry.original
                } else {
                    entry.cleaned.as_deref().unwrap_or(&entry.original)
                };
                let (preview, shortened) = text_preview(text, HISTORY_DETAIL_CHARS);
                el.child(
                    div()
                        .id("selected-history")
                        .max_h(px(150.))
                        .overflow_y_scroll()
                        .border_t_1()
                        .border_color(rgb(0x303030))
                        .pt_3()
                        .child(preview),
                )
                .when(shortened, |el| {
                    el.child(div().text_xs().child(t("history_preview_shortened")))
                })
                .child(
                    Button::new("copy-history")
                        .ghost()
                        .label(t("copy"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(entry) = this
                                .selected
                                .and_then(|index| this.model.history.entries.get(index))
                            {
                                let text = if this.show_original {
                                    entry.original.clone()
                                } else {
                                    entry
                                        .cleaned
                                        .as_deref()
                                        .unwrap_or(&entry.original)
                                        .to_owned()
                                };
                                this.copy(text, cx);
                            }
                        })),
                )
                .when(entry.cleaned.is_some(), |el| {
                    el.child(
                        Button::new("history-original")
                            .ghost()
                            .label(t(if self.show_original {
                                "view_output"
                            } else {
                                "view_original"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_original = !this.show_original;
                                cx.notify();
                            })),
                    )
                })
            })
    }
    fn preferences(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bound = self.hotkey.as_ref().is_some_and(HotkeyLease::is_bound);
        div()
            .id("preferences-scroll")
            .overflow_y_scroll()
            .flex_1()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().child(t("microphone")))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("default-microphone")
                            .ghost()
                            .label(t("system_microphone"))
                            .selected(self.model.settings.input_device.is_none())
                            .toggled(self.model.settings.input_device.is_none())
                            .disabled(!self.model.settings_loaded || self.model.active())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.settings.input_device = None;
                                this.send(Command::SaveSettings(this.model.settings.clone()));
                                cx.notify();
                            })),
                    )
                    .children(self.model.devices.iter().enumerate().map(|(index, name)| {
                        let name = name.clone();
                        Button::new(("microphone", index))
                            .ghost()
                            .label(name.clone())
                            .disabled(!self.model.settings_loaded || self.model.active())
                            .selected(self.model.settings.input_device.as_ref() == Some(&name))
                            .toggled(self.model.settings.input_device.as_ref() == Some(&name))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.settings.input_device = Some(name.clone());
                                this.send(Command::SaveSettings(this.model.settings.clone()));
                                cx.notify();
                            }))
                    }))
                    .child(
                        Button::new("find-microphones")
                            .ghost()
                            .label(t("refresh_devices"))
                            .disabled(!self.model.settings_loaded || self.model.active())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send(Command::ListDevices);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(0x303030))
                    .pt_3()
                    .child(t("shortcut")),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Input::new(&self.shortcut)
                            .aria_label(t("shortcut"))
                            .disabled(!self.model.settings_loaded),
                    )
                    .child(
                        Button::new("bind-shortcut")
                            .disabled(!self.model.settings_loaded)
                            .label(t("apply_shortcut"))
                            .on_click(cx.listener(|this, _, _, cx| this.set_hotkey(cx))),
                    ),
            )
            .when(bound, |el| {
                el.child(
                    Button::new("unbind-shortcut")
                        .ghost()
                        .label(t("shortcut_disable"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hotkey = None;
                            this.model
                                .settings
                                .extra
                                .insert("shortcut_enabled".into(), serde_json::Value::Bool(false));
                            this.send(Command::SaveSettings(this.model.settings.clone()));
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(0x303030))
                    .pt_3()
                    .child(t("vocabulary")),
            )
            .child(
                Input::new(&self.vocabulary)
                    .aria_label(t("vocabulary"))
                    .disabled(!self.model.settings_loaded),
            )
            .child(
                Button::new("save-preferences")
                    .disabled(!self.model.settings_loaded)
                    .label(t("save"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.settings.vocabulary = this
                            .vocabulary
                            .read(cx)
                            .value()
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect();
                        this.send(Command::SaveSettings(this.model.settings.clone()));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("dictation-details")
                    .ghost()
                    .small()
                    .label(t("dictation_details"))
                    .selected(self.dictation_help)
                    .toggled(self.dictation_help)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dictation_help = !this.dictation_help;
                        cx.notify();
                    })),
            )
            .when(self.dictation_help, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_sm()
                        .text_color(rgb(0xb3b3b3))
                        .child(t("shortcut_body"))
                        .child(t("vocabulary_body"))
                        .child(t("delivery_hint"))
                        .child(t("recording_limit"))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child("Phonon 2.5 English")
                                .child(self.engine_label()),
                        )
                        .child(t("engine_residency")),
                )
            })
            .child(self.update_preferences(cx))
    }
    fn update_preferences(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.update_consent == saytwig_updates::Consent::Enabled;
        let busy = self
            .updates
            .as_ref()
            .is_some_and(crate::updates::Updates::busy);
        let commit = option_env!("SAYTWIG_BUILD_COMMIT")
            .filter(|sha| {
                matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .map(|sha| sha.chars().take(12).collect::<String>());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .border_t_1()
            .border_color(rgb(0x303030))
            .pt_3()
            .child(t("updates_title"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .text_sm()
                    .child(t("build_source_version"))
                    .child(env!("CARGO_PKG_VERSION"))
                    .child(t(crate::updates::channel_key()))
                    .when_some(commit, |el, sha| el.child(sha)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .child(t("updates_privacy")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("update-consent")
                            .ghost()
                            .selected(enabled)
                            .toggled(enabled)
                            .label(t(if enabled {
                                "updates_disable"
                            } else {
                                "updates_allow"
                            }))
                            .disabled(self.updates.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_release = None;
                                if this.update_consent == saytwig_updates::Consent::Enabled {
                                    this.update_consent = saytwig_updates::Consent::Disabled;
                                    if let Some(updates) = &mut this.updates {
                                        updates.cancel();
                                    }
                                    this.update_message = "updates_off";
                                } else {
                                    // Permission alone never initiates an HTTP request.
                                    this.update_consent = saytwig_updates::Consent::Enabled;
                                    this.update_message = if this
                                        .updates
                                        .as_ref()
                                        .is_some_and(crate::updates::Updates::busy)
                                    {
                                        "updates_waiting"
                                    } else {
                                        "updates_ready"
                                    };
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("check-stable-release")
                            .ghost()
                            .tooltip(t("updates_no_install"))
                            .label(t("updates_check"))
                            .disabled(!enabled || busy || self.updates.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_release = None;
                                this.update_message = if this
                                    .updates
                                    .as_mut()
                                    .is_some_and(|updates| updates.check(this.update_consent))
                                {
                                    "updates_checking"
                                } else {
                                    "updates_unavailable"
                                };
                                cx.notify();
                            })),
                    )
                    .when(busy, |el| {
                        el.child(
                            Button::new("cancel-release-check")
                                .ghost()
                                .label(t("cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(updates) = &mut this.updates {
                                        updates.cancel();
                                    }
                                    this.update_release = None;
                                    this.update_message = "updates_cancelled";
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .when(
                self.updates.is_none()
                    || !matches!(self.update_message, "updates_off" | "updates_ready"),
                |el| {
                    el.child(div().text_sm().child(t(if self.updates.is_some() {
                        self.update_message
                    } else {
                        "updates_unavailable"
                    })))
                },
            )
            .when_some(self.update_release.as_ref(), |el, release| {
                let release = release.clone();
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(release.version.clone())
                        .child(
                            Button::new("open-stable-release")
                                .ghost()
                                .label(t("updates_open_release"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.update_consent == saytwig_updates::Consent::Enabled
                                        && this.update_release.as_ref() == Some(&release)
                                        && !release.open()
                                    {
                                        this.update_message = "updates_browser_failed";
                                    }
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
    fn cleanup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self
            .profiles
            .iter()
            .any(|profile| profile.active && profile.connected && profile.plan_use_granted);
        let enabled = self.model.session.cleanup_enabled();
        div()
            .id("cleanup-scroll")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .flex_1()
            .child(div().text_lg().child(t("cleanup_title")))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("load-accounts")
                            .ghost()
                            .label(t("manage_accounts"))
                            .disabled(self.account_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send_cloud(CloudCommand::Accounts);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("connect-account")
                            .label(t("connect"))
                            .disabled(self.account_busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.disable_cleanup();
                                this.cleanup_models.clear();
                                this.cleanup_model = None;
                                this.cleanup_profile = None;
                                let cancel = CancellationToken::new();

                                this.send_cloud(CloudCommand::SignIn(None, cancel));
                                cx.notify();
                            })),
                    ),
            )
            .children(self.profiles.iter().enumerate().map(|(index, profile)| {
                let id = profile.id.clone();
                let retry_id = id.clone();
                let logout_id = id.clone();
                let plan_id = id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .border_t_1()
                    .border_color(rgb(0x303030))
                    .pt_3()
                    .child(div().flex_1().child(profile.label.clone()).child(
                        div().text_xs().text_color(rgb(0xb3b3b3)).child(t(
                            if profile.active && profile.connected {
                                "account_active"
                            } else if profile.connected {
                                "connected"
                            } else {
                                "disconnected"
                            },
                        )),
                    ))
                    .when(profile.connected, |el| {
                        el.child(
                            Button::new(("activate-profile", index))
                                .ghost()
                                .label(t("use_account"))
                                .disabled(self.account_busy || profile.active)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.disable_cleanup();
                                    this.cleanup_models.clear();
                                    this.cleanup_model = None;
                                    this.send_cloud(CloudCommand::Activate(id.clone()));
                                    cx.notify();
                                })),
                        )
                    })
                    .when(profile.connected && !profile.plan_use_granted, |el| {
                        el.child(
                            Button::new(("enable-plan-profile", index))
                                .ghost()
                                .label(t("enable_plan_use"))
                                .disabled(self.account_busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.disable_cleanup();
                                    this.cleanup_models.clear();
                                    this.cleanup_model = None;
                                    this.cleanup_profile = None;
                                    let cancel = CancellationToken::new();

                                    this.send_cloud(CloudCommand::EnablePlanUse(
                                        plan_id.clone(),
                                        cancel,
                                    ));
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new(("continue-profile", index))
                            .ghost()
                            .label(t(if profile.connected {
                                "signin_again"
                            } else {
                                "continue_signin"
                            }))
                            .disabled(self.account_busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.disable_cleanup();
                                this.cleanup_models.clear();
                                this.cleanup_model = None;
                                this.cleanup_profile = None;
                                let cancel = CancellationToken::new();

                                this.send_cloud(CloudCommand::SignIn(
                                    Some(retry_id.clone()),
                                    cancel,
                                ));
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("disconnect-profile", index))
                            .ghost()
                            .label(t("disconnect"))
                            .disabled(self.account_busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.disable_cleanup();
                                this.cleanup_models.clear();
                                this.cleanup_model = None;
                                this.cleanup_profile = None;
                                let cancel = CancellationToken::new();

                                this.send_cloud(CloudCommand::SignOut(logout_id.clone(), cancel));
                                cx.notify();
                            })),
                    )
            }))
            .when(
                self.profiles
                    .iter()
                    .any(|profile| profile.connected && !profile.plan_use_granted),
                |el| {
                    el.child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb3b3b3))
                            .child(t("plan_use_explanation")),
                    )
                },
            )
            .when(self.account_busy, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(t("account_busy"))
                        .when(self.account_cancel.is_some(), |el| {
                            el.child(
                                Button::new("cancel-account")
                                    .ghost()
                                    .label(t("cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(cancel) = &this.account_cancel {
                                            cancel.cancel();
                                        }
                                        cx.notify();
                                    })),
                            )
                        }),
                )
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(0x303030))
                    .pt_3()
                    .child(t("cleanup_model")),
            )
            .when(connected, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_2()
                        .child(div().text_sm().child(t(if enabled {
                            "using_chatgpt_plan"
                        } else {
                            "plan_sharing_off"
                        })))
                        .child(
                            Button::new("manage-plan-usage")
                                .ghost()
                                .small()
                                .label(t("manage_usage"))
                                .disabled(self.account_busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.manage_usage();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(self.plan_limited, |el| {
                el.child(div().text_sm().child(t("plan_usage_limited")))
            })
            .child(
                Button::new("load-cleanup-models")
                    .ghost()
                    .label(t("load_models"))
                    .disabled(!connected || self.account_busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.disable_cleanup();
                        let cancel = CancellationToken::new();

                        this.send_cloud(CloudCommand::Models(cancel));
                        cx.notify();
                    })),
            )
            .child(
                div().flex().flex_wrap().gap_2().children(
                    self.cleanup_models
                        .iter()
                        .enumerate()
                        .map(|(index, model)| {
                            let model = model.clone();
                            Button::new(("cleanup-model", index))
                                .ghost()
                                .label(model.display_name.clone())
                                .selected(self.cleanup_model.as_ref() == Some(&model))
                                .toggled(self.cleanup_model.as_ref() == Some(&model))
                                .disabled(self.account_busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.disable_cleanup();
                                    this.cleanup_model = Some(model.clone());
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xb3b3b3))
                    .child(t("sharing_explanation")),
            )
            .child(
                Button::new("transcript-consent")
                    .label(t(if enabled {
                        "disable_sharing"
                    } else {
                        "enable_sharing"
                    }))
                    .disabled(
                        !enabled
                            && (!connected || self.cleanup_model.is_none() || self.account_busy),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.model.session.cleanup_enabled() {
                            this.disable_cleanup();
                        } else {
                            this.plan_limited = false;
                            this.consent.set_enabled(true);
                            this.model.session.set_cleanup_enabled(true);
                        }
                        cx.notify();
                    })),
            )
    }
}
impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.show_plan_welcome(window, cx);
        if self.sync_inputs {
            self.sync_inputs = false;
            let shortcut = self
                .model
                .settings
                .extra
                .get("shortcut")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Control+Space")
                .to_owned();
            self.shortcut
                .update(cx, |input, cx| input.set_value(shortcut, window, cx));
            self.vocabulary.update(cx, |input, cx| {
                input.set_value(self.model.settings.vocabulary.join(", "), window, cx)
            });
        }
        if self.restore_hotkey {
            self.restore_hotkey = false;
            self.set_hotkey(cx);
        }
        let body = match self.page {
            Page::Dictation => self.dictation(cx).into_any_element(),
            Page::History => self.history(cx).into_any_element(),
            Page::Preferences => self.preferences(cx).into_any_element(),
            Page::Cleanup => self.cleanup(cx).into_any_element(),
        };
        div()
            .id("saytwig-shell")
            .track_focus(&self.focus)
            .key_context("Saytwig")
            .on_action(cx.listener(|this, _: &CancelDictation, _, cx| {
                if this.model.active() || this.model.installing {
                    this.cancel();
                    cx.notify();
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0x0d0d0f))
            .text_color(rgb(0xf2f2f2))
            .text_size(px(14.))
            .p_5()
            .gap_3()
            .child(self.navigation(cx))
            .when(
                self.page == Page::Dictation
                    && !matches!(
                        self.model.engine,
                        EngineState::Installed | EngineState::Ready
                    ),
                |el| el.child(self.setup(cx)),
            )
            .when_some(self.model.notice, |el, key| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x222222))
                        .rounded_md()
                        .child(div().flex_1().text_sm().child(t(key)))
                        .when(key == "plan_usage_limited", |el| {
                            el.child(
                                Button::new("notice-manage-usage")
                                    .ghost()
                                    .small()
                                    .label(t("manage_usage"))
                                    .disabled(self.account_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.manage_usage();
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            Button::new("dismiss-notice")
                                .ghost()
                                .small()
                                .label(t("dismiss"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(body)
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        if let Some((_, cancel)) = self.capture_cancel.take() {
            cancel.cancel();
        }
        self.consent.set_enabled(false);
        if let Some(cancel) = self.account_cancel.take() {
            cancel.cancel();
        }
        if let Some((_, cancel)) = self.cleanup_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
    }
}
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelDictation, Some("Saytwig"))]);
}

struct RecordingPill {
    parent: gpui_kit::WeakEntity<Shell>,
    phase: Phase,
    generation: Generation,
}
impl Render for RecordingPill {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Opening this window may render it inside Shell.update. Never read Shell here.
        let label = t(match self.phase {
            Phase::Preparing => "preparing",
            Phase::Recording => "recording",
            Phase::Transcribing => "transcribing",
            Phase::Cleaning => "cleaning",
            _ => "idle",
        });
        let generation = self.generation;
        let stop_parent = self.parent.clone();
        let cancel_parent = self.parent.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .size_full()
            .px_3()
            .gap_2()
            .bg(rgb(0x0d0d0f))
            .text_color(rgb(0xf2f2f2))
            .text_size(px(13.))
            .child(div().flex_1().child(label))
            .when(self.phase == Phase::Recording, |el| {
                el.child(
                    Button::new("overlay-stop")
                        .ghost()
                        .small()
                        .label(t("stop"))
                        .on_click(move |_, _, cx| {
                            let parent = stop_parent.clone();
                            cx.defer(move |cx| {
                                let _ = parent.update(cx, |this, cx| {
                                    if this.model.session.generation() == Some(generation)
                                        && this.model.session.phase() == Phase::Recording
                                    {
                                        this.send(Command::Stop);
                                        cx.notify();
                                    }
                                });
                            });
                        }),
                )
            })
            .child(
                Button::new("overlay-cancel")
                    .ghost()
                    .small()
                    .label(t("cancel"))
                    .on_click(move |_, _, cx| {
                        let parent = cancel_parent.clone();
                        cx.defer(move |cx| {
                            let _ = parent.update(cx, |this, cx| {
                                if this.model.session.generation() == Some(generation)
                                    && this.model.active()
                                {
                                    this.cancel();
                                    cx.notify();
                                }
                            });
                        });
                    }),
            )
    }
}
