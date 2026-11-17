//! Rust-only signed update transport. Velopack receives only a frozen verified offer.
use crate::strings::{format, text};
use eframe::egui;
use shovel_core::{
    process::Cancellation,
    updates::{self, Channel, VerifiedRelease},
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};
use velopack::{
    UpdateInfo, UpdateManager, VelopackAsset, VelopackAssetFeed, sources::UpdateSource,
};
const CHECK_INTERVAL: Duration = Duration::from_secs(3600);
const CHECK_HTTP_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const UPDATE_CARD_WIDTH: f32 = 116.;

pub(super) fn offer_card(ui: &mut egui::Ui, version: Option<&semver::Version>) -> bool {
    let Some(version) = version else {
        return false;
    };
    let palette = crate::workspace_style::Palette::current(ui);
    ui.add_sized(
        [UPDATE_CARD_WIDTH, 34.],
        egui::Button::new(
            egui::RichText::new(text("UpdateShovel"))
                .strong()
                .color(palette.selection_text),
        )
        .fill(palette.active_selection),
    )
    .on_hover_text(format("UpdateOffered", &[&version.to_string()]))
    .clicked()
}
#[derive(Clone)]
struct Offer {
    release: VerifiedRelease,
    asset: VelopackAsset,
}
fn failure(_: impl std::fmt::Display) -> String {
    text("RustUpdateFailed")
}
fn bytes(url: &str, max: u64, cancel: &Cancellation) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    transfer(url, &mut output, max, CHECK_HTTP_TIMEOUT, cancel, None)?;
    Ok(output)
}
fn transfer(
    url: &str,
    out: &mut impl Write,
    max: u64,
    timeout: Duration,
    cancel: &Cancellation,
    progress: Option<&mpsc::Sender<i16>>,
) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Err(text("ActionResultCancelled"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_global(Some(timeout))
        .timeout_resolve(Some(Duration::from_secs(30)))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "ShovelRust")
        .call()
        .map_err(failure)?;
    let mut reader = response.body_mut().as_reader();
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err(text("ActionResultCancelled"));
        }
        let n = reader.read(&mut buffer).map_err(failure)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            return Err(text("RustUpdateInvalid"));
        }
        out.write_all(&buffer[..n]).map_err(failure)?;
        if let Some(tx) = progress {
            let _ = tx.send((total.saturating_mul(100) / max.max(1)).min(100) as i16);
        }
    }
    Ok(())
}
fn with_progress(
    run: impl FnOnce(mpsc::Sender<i16>) -> Result<(), String>,
    report: impl Fn(i16) + Send + 'static,
) -> Result<(), String> {
    let (progress, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        while let Ok(value) = rx.recv() {
            report(value);
        }
    });
    let result = run(progress);
    worker.join().map_err(|_| text("RustUpdateFailed"))?;
    result
}
fn trusted_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ShovelRust/TrustedUpdates")
}
enum CheckError {
    Unavailable(String),
    Rejected(&'static str),
}
impl From<String> for CheckError {
    fn from(error: String) -> Self {
        Self::Unavailable(error)
    }
}
fn check(channel: Channel, cancel: &Cancellation) -> Result<Option<Offer>, CheckError> {
    let feed = bytes(&channel.feed_url(), updates::MAX_FEED_BYTES as u64, cancel)?;
    let signature = bytes(&format!("{}.sig", channel.feed_url()), 2048, cancel)?;
    let release = updates::verify_feed(channel, &feed, &signature)
        .map_err(|_| CheckError::Rejected("RustUpdateInvalid"))?;
    // Immutable, signed floor records shared by this user's Rust profiles.
    let directory = trusted_directory().join(channel.id());
    fs::create_dir_all(&directory).map_err(failure)?;
    let records = fs::read_dir(&directory)
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    if records.len() > 1024 {
        return Err(CheckError::Rejected("RustUpdateInvalid"));
    }
    for entry in records {
        if cancel.is_cancelled() {
            return Err(text("ActionResultCancelled").into());
        }
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let mut data = Vec::new();
        File::open(path)
            .map_err(failure)?
            .take(8 * updates::MAX_FEED_BYTES as u64)
            .read_to_end(&mut data)
            .map_err(failure)?;
        let (old_feed, old_sig): (Vec<u8>, Vec<u8>) =
            serde_json::from_slice(&data).map_err(|_| CheckError::Rejected("RustUpdateInvalid"))?;
        let old = updates::verify_feed(channel, &old_feed, &old_sig)
            .map_err(|_| CheckError::Rejected("RustUpdateInvalid"))?;
        if release.version() < old.version() {
            return Err(CheckError::Rejected("RustUpdateReplay"));
        }
    }
    let record = directory.join(format!("{}.json", release.version()));
    if !record.exists() {
        let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        fs::write(
            &temporary,
            serde_json::to_vec(&(feed.clone(), signature)).map_err(failure)?,
        )
        .map_err(failure)?;
        fs::rename(&temporary, &record).map_err(failure)?;
    }
    if !release.is_newer_than(&semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()) {
        return Ok(None);
    }
    let feed: VelopackAssetFeed = serde_json::from_slice(&feed).map_err(failure)?;
    let asset = feed
        .Assets
        .into_iter()
        .next()
        .ok_or(CheckError::Rejected("RustUpdateInvalid"))?;
    Ok(Some(Offer { release, asset }))
}
#[derive(Clone)]
struct FrozenSource {
    offer: Offer,
    cancel: Cancellation,
}
impl UpdateSource for FrozenSource {
    fn get_release_feed(
        &self,
        channel: &str,
        _: &velopack::bundle::Manifest,
        _: &str,
    ) -> Result<VelopackAssetFeed, velopack::Error> {
        if channel != self.offer.release.channel().id() {
            return Err(velopack::Error::NotSupported("Wrong Rust channel".into()));
        }
        Ok(VelopackAssetFeed {
            Assets: vec![self.offer.asset.clone()],
        })
    }
    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        progress: Option<mpsc::Sender<i16>>,
    ) -> Result<(), velopack::Error> {
        let err = |s: String| velopack::Error::NotSupported(s);
        if asset.FileName != self.offer.release.file_name()
            || asset.SHA256 != self.offer.asset.SHA256
        {
            return Err(err(text("RustUpdateInvalid")));
        }
        let mut file = File::create(path)?;
        transfer(
            &self.offer.release.package_url(),
            &mut file,
            self.offer.release.size(),
            Duration::from_secs(600),
            &self.cancel,
            progress.as_ref(),
        )
        .map_err(err)?;
        file.sync_all()?;
        drop(file);
        self.offer
            .release
            .verify_package(File::open(path)?)
            .map_err(|e| err(e.to_string()))
    }
}
fn manager(offer: &Offer, cancel: Cancellation) -> Result<UpdateManager, String> {
    let manager = UpdateManager::new(
        FrozenSource {
            offer: offer.clone(),
            cancel,
        },
        Some(velopack::UpdateOptions {
            ExplicitChannel: Some(offer.release.channel().id().into()),
            AllowVersionDowngrade: false,
            MaximumDeltasBeforeFallback: 0,
        }),
        None,
    )
    .map_err(|_| text("RustUpdateInstallNeeded"))?;
    if manager.get_app_id() != updates::PRODUCT {
        return Err(text("RustUpdateInvalid"));
    }
    Ok(manager)
}
fn package_path(offer: &Offer) -> Result<PathBuf, String> {
    let locator = velopack::locator::auto_locate_app_manifest(
        velopack::locator::LocationContext::FromCurrentExe,
    )
    .map_err(failure)?;
    Ok(locator.get_packages_dir().join(offer.release.file_name()))
}
enum Event {
    Applied(Result<(), String>),
    Checked(u64, Result<Option<Offer>, CheckError>),
    Downloaded(u64, Result<Offer, String>),
    Progress(u64, i16),
}
pub struct Controller {
    pub open: bool,
    pub restart_requested: bool,
    pub ready: bool,
    pub exit_ready: bool,
    generation: u64,
    tools_confirmed: bool,
    channel: Channel,
    offer: Option<Offer>,
    status: String,
    busy: bool,
    cancel: Cancellation,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    last: Option<Instant>,
    workers: crate::read_workers::Workers,
}
impl Default for Controller {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            open: false,
            restart_requested: false,
            ready: false,
            exit_ready: false,
            generation: 0,
            tools_confirmed: false,
            channel: Channel::Beta,
            offer: None,
            status: String::new(),
            busy: false,
            cancel: Default::default(),
            workers: Default::default(),
            tx,
            rx,
            last: if cfg!(test) {
                Some(Instant::now())
            } else {
                None
            },
        }
    }
}
impl Controller {
    pub fn stop_and_drained(&self) -> bool {
        self.cancel();
        self.workers.cancel_and_drained()
    }
    pub fn resume(&self) {
        self.workers.resume();
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn available_version(&self, channel: Channel) -> Option<&semver::Version> {
        self.offer
            .as_ref()
            .filter(|offer| offer.release.channel() == channel)
            .map(|offer| offer.release.version())
    }
    pub fn tick(&mut self, ctx: &egui::Context, channel: Channel, periodic: bool) {
        self.tick_at(ctx, channel, periodic, Instant::now(), |channel, cancel| {
            check(channel, &cancel)
        });
    }
    fn tick_at(
        &mut self,
        ctx: &egui::Context,
        channel: Channel,
        periodic: bool,
        now: Instant,
        run: impl FnOnce(Channel, Cancellation) -> Result<Option<Offer>, CheckError> + Send + 'static,
    ) {
        if self.channel != channel {
            self.cancel();
            self.generation += 1;
            self.busy = false;
            self.restart_requested = false;
            self.channel = channel;
            self.offer = None;
            self.ready = false;
            self.last = None;
            self.status = text("RustUpdateChecking");
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Applied(result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => self.exit_ready = true,
                        Err(error) => {
                            self.status = error;
                            self.restart_requested = false;
                            self.open = true;
                        }
                    }
                }
                Event::Progress(id, p) if id == self.generation => {
                    self.status = format("RustUpdateProgress", &[&p.to_string()])
                }
                Event::Checked(id, result) if id == self.generation => {
                    self.busy = false;
                    match result {
                        Ok(Some(offer)) if offer.release.channel() == self.channel => {
                            self.ready &= self.offer.as_ref().is_some_and(|current| {
                                current.release.version() == offer.release.version()
                                    && current.release.size() == offer.release.size()
                                    && current
                                        .asset
                                        .SHA256
                                        .eq_ignore_ascii_case(&offer.asset.SHA256)
                            });
                            self.status = if self.ready {
                                text("UpdateReadyShort")
                            } else {
                                format("UpdateOffered", &[&offer.release.version().to_string()])
                            };
                            self.offer = Some(offer);
                        }
                        Ok(None) => {
                            self.offer = None;
                            self.ready = false;
                            self.status = text("RustUpdateCurrent");
                        }
                        Err(CheckError::Unavailable(error)) => self.status = error,
                        Err(CheckError::Rejected(key)) => {
                            self.offer = None;
                            self.ready = false;
                            self.status = text(key);
                        }
                        Ok(Some(_)) => {}
                    }
                }
                Event::Downloaded(id, result) if id == self.generation => {
                    self.busy = false;
                    match result {
                        Ok(offer) if offer.release.channel() == self.channel => {
                            self.offer = Some(offer);
                            self.ready = true;
                            self.status = text("UpdateReadyShort");
                        }
                        Err(e) => self.status = e,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if self.busy || self.restart_requested || self.exit_ready || self.workers.stopping() {
            return;
        }
        match self.last {
            None => self.begin_check(ctx, now, run),
            Some(last) if periodic => {
                let remaining = CHECK_INTERVAL.saturating_sub(now.saturating_duration_since(last));
                if remaining.is_zero() {
                    self.begin_check(ctx, now, run);
                } else {
                    ctx.request_repaint_after(remaining);
                }
            }
            Some(_) => {}
        }
    }
    pub fn check(&mut self, ctx: &egui::Context) {
        self.begin_check(ctx, Instant::now(), |channel, cancel| {
            check(channel, &cancel)
        });
    }
    fn begin_check(
        &mut self,
        ctx: &egui::Context,
        now: Instant,
        run: impl FnOnce(Channel, Cancellation) -> Result<Option<Offer>, CheckError> + Send + 'static,
    ) {
        if self.busy
            || self.restart_requested
            || self.exit_ready
            || self.workers.stopping()
            || !self.workers.idle()
        {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.busy = true;
        self.last = Some(now);
        self.status = text("RustUpdateChecking");
        self.cancel = Cancellation::default();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let channel = self.channel;
        let cancel = self.cancel.clone();
        let lease = self.workers.start(cancel.clone());
        std::thread::spawn(move || {
            let result = run(channel, cancel);
            drop(lease);
            let _ = tx.send(Event::Checked(generation, result));
            ctx.request_repaint();
        });
    }
    pub fn show(&mut self, ctx: &egui::Context, blocked: bool, tools_opened: bool) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new(text("RustUpdatesTitle"))
            .open(&mut open)
            .enabled(!self.restart_requested && !self.workers.stopping())
            .default_width(510.)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Shovel Rust {} · {}",
                    env!("CARGO_PKG_VERSION"),
                    self.channel.label()
                ));
                ui.label(&self.status);
                if let Some(offer) = &self.offer {
                    ui.label(offer.release.notes());
                }
                if self.ready && tools_opened {
                    ui.checkbox(&mut self.tools_confirmed, text("UpdateToolsQuestion"));
                }
                let blocked = blocked || (tools_opened && !self.tools_confirmed);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new(text("UpdateRetry")))
                        .clicked()
                    {
                        self.check(ctx);
                    }
                    if self.busy && ui.button(text("Cancel")).clicked() {
                        self.cancel();
                    }
                    if self.ready {
                        if ui
                            .add_enabled(
                                !blocked && !self.busy,
                                egui::Button::new(text("RustUpdateRestart")),
                            )
                            .on_disabled_hover_text(text("RustUpdateBlocked"))
                            .clicked()
                        {
                            self.restart_requested = true;
                        }
                    } else if self.offer.is_some()
                        && !self.busy
                        && ui.button(text("RustUpdateDownload")).clicked()
                    {
                        self.generation += 1;
                        let generation = self.generation;
                        self.busy = true;
                        self.cancel = Cancellation::default();
                        let offer = self.offer.clone().unwrap();
                        let cancel = self.cancel.clone();
                        let tx = self.tx.clone();
                        let ctx = ctx.clone();
                        let lease = self.workers.start(cancel.clone());
                        std::thread::spawn(move || {
                            let result = (|| {
                                let manager = manager(&offer, cancel)?;
                                let target = package_path(&offer)?;
                                if target.exists() {
                                    offer
                                        .release
                                        .verify_package(File::open(&target).map_err(failure)?)
                                        .map_err(|_| text("RustUpdateInvalid"))?;
                                }
                                let pt = tx.clone();
                                let pc = ctx.clone();
                                with_progress(
                                    |progress| {
                                        manager
                                            .download_updates(
                                                &UpdateInfo {
                                                    TargetFullRelease: offer.asset.clone(),
                                                    ..Default::default()
                                                },
                                                Some(progress),
                                            )
                                            .map_err(failure)
                                    },
                                    move |p| {
                                        let _ = pt.send(Event::Progress(generation, p));
                                        pc.request_repaint();
                                    },
                                )?;
                                offer
                                    .release
                                    .verify_package(File::open(target).map_err(failure)?)
                                    .map_err(|_| text("RustUpdateInvalid"))?;
                                Ok(offer)
                            })();
                            drop(lease);
                            let _ = tx.send(Event::Downloaded(generation, result));
                            ctx.request_repaint();
                        });
                    }
                });
            });
        self.open = open;
    }
    pub fn apply(
        &mut self,
        ctx: &egui::Context,
        git: Option<std::sync::Arc<shovel_core::git::Git>>,
        paths: Vec<PathBuf>,
    ) {
        let offer = self.offer.clone();
        let ready = self.ready;
        let channel = self.channel;
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                other_instances()?;
                if let Some(git) = git {
                    for path in paths {
                        let snapshot = git
                            .inspect(&path, &Cancellation::default())
                            .map_err(|_| text("RustUpdateRepositoryState"))?;
                        if !snapshot.operations.is_empty() {
                            return Err(text("RustUpdateRepositoryState"));
                        }
                    }
                }
                let offer = offer
                    .filter(|o| ready && o.release.channel() == channel)
                    .ok_or_else(|| text("RustUpdateInvalid"))?;
                offer
                    .release
                    .verify_package(File::open(package_path(&offer)?).map_err(failure)?)
                    .map_err(|_| text("RustUpdateInvalid"))?;
                manager(&offer, Cancellation::default())?
                    .wait_exit_then_apply_updates(
                        &offer.asset,
                        false,
                        true,
                        std::env::args_os().skip(1),
                    )
                    .map_err(failure)
            })();
            let _ = tx.send(Event::Applied(result));
            ctx.request_repaint();
        });
    }
}

// Check all Windows sessions, not just windows in this profile.
fn other_instances() -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(text("RustUpdateInstances"));
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut next = Process32FirstW(snapshot, &mut entry);
        if next == 0 {
            CloseHandle(snapshot);
            return Err(text("RustUpdateInstances"));
        }
        while next != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if entry.th32ProcessID != std::process::id()
                && name.eq_ignore_ascii_case("ShovelRust.exe")
            {
                CloseHandle(snapshot);
                return Err(text("RustUpdateInstances"));
            }
            next = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obsolete_update_events_cannot_restore_previous_channel_or_progress() {
        let mut controller = Controller {
            channel: Channel::Stable,
            generation: 3,
            status: "current".into(),
            ..Default::default()
        };
        controller.tx.send(Event::Checked(2, Ok(None))).unwrap();
        controller
            .tx
            .send(Event::Downloaded(2, Err("old failure".into())))
            .unwrap();
        controller.tx.send(Event::Progress(2, 99)).unwrap();
        controller.tick(&egui::Context::default(), Channel::Stable, false);
        assert_eq!(controller.status, "current");
        assert!(!controller.ready);
        assert!(controller.offer.is_none());
    }

    #[test]
    fn cancelled_transport_returns_before_url_or_network_preparation() {
        let cancel = Cancellation::default();
        cancel.cancel();
        let mut output = Vec::new();
        assert_eq!(
            transfer(
                "not a URL",
                &mut output,
                100,
                CHECK_HTTP_TIMEOUT,
                &cancel,
                None
            ),
            Err(text("ActionResultCancelled"))
        );
        assert!(output.is_empty());
    }

    #[test]
    fn progress_forwarder_finishes_on_download_failure_and_success() {
        use std::sync::{Arc, Mutex};
        for fail in [false, true] {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let output = seen.clone();
            let result = with_progress(
                move |tx| {
                    tx.send(12).unwrap();
                    tx.send(47).unwrap();
                    if fail {
                        Err("controlled download failure".into())
                    } else {
                        Ok(())
                    }
                },
                move |value| output.lock().unwrap().push(value),
            );
            assert_eq!(*seen.lock().unwrap(), vec![12, 47]);
            assert_eq!(result.is_err(), fail);
        }
    }

    #[test]
    fn shutdown_drains_held_update_check_before_final_close() {
        let ctx = egui::Context::default();
        let mut app = crate::workspace_interaction_tests::workspace();
        app.tabs.clear();
        let temp = tempfile::tempdir().unwrap();
        let mut store = crate::SettingsStore::new(temp.path().into());
        app.settings = store.load().unwrap();
        app.store = Some(store);
        let (started, token) = mpsc::channel();
        let (release, held) = mpsc::channel();
        app.updater
            .begin_check(&ctx, Instant::now(), move |_, cancel| {
                started.send(cancel.clone()).unwrap();
                held.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(cancel.is_cancelled());
                Err(text("ActionResultCancelled").into())
            });
        let cancel = token.recv_timeout(Duration::from_secs(5)).unwrap();
        app.close(&ctx);
        assert!(cancel.is_cancelled());
        assert!(app.pending_exit && !app.exit_ready && !app.closing);
        assert_eq!(app.notice, Some(text("RustShutdownUpdates")));
        app.receive(&ctx);
        assert!(!app.exit_ready);
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.exit_ready {
            app.receive(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.updater.stop_and_drained());
        assert!(!app.updater.ready);
    }

    #[test]
    fn channel_change_cannot_forget_old_worker_and_checks_resume_after_drain() {
        let ctx = egui::Context::default();
        let mut controller = Controller::default();
        let (started, token) = mpsc::channel();
        let (release, held) = mpsc::channel();
        controller.begin_check(&ctx, Instant::now(), move |channel, cancel| {
            assert_eq!(channel, Channel::Beta);
            started.send(cancel).unwrap();
            held.recv_timeout(Duration::from_secs(10)).unwrap();
            Err(CheckError::Unavailable("obsolete beta failure".into()))
        });
        let cancel = token.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!controller.stop_and_drained());
        controller.tick(&ctx, Channel::Stable, false);
        assert!(cancel.is_cancelled());
        assert!(!controller.busy);
        assert!(
            !controller.stop_and_drained(),
            "presentation idle is not worker completion"
        );
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !controller.stop_and_drained() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        controller.tick(&ctx, Channel::Stable, false);
        assert_ne!(controller.status, "obsolete beta failure");
        controller.resume();
        controller.begin_check(&ctx, Instant::now(), |channel, _| {
            assert_eq!(channel, Channel::Stable);
            Ok(None)
        });
        while controller.busy {
            controller.tick(&ctx, Channel::Stable, false);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(controller.status, text("RustUpdateCurrent"));
    }

    #[test]
    fn startup_check_is_async_and_hourly_checks_follow_the_channel_and_preference() {
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut controller = Controller {
            last: None,
            ..Default::default()
        };
        let (calls, received) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let startup = calls.clone();
        controller.tick_at(&ctx, Channel::Stable, false, start, move |channel, _| {
            startup.send(channel).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(None)
        });
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            Channel::Stable
        );
        assert!(controller.busy && !controller.open);
        let tick = |controller: &mut Controller, periodic, now| {
            let calls = calls.clone();
            controller.tick_at(&ctx, Channel::Stable, periodic, now, move |channel, _| {
                calls.send(channel).unwrap();
                Ok(None)
            });
        };
        let hour = start + Duration::from_secs(3600);
        tick(&mut controller, true, hour);
        assert_eq!(controller.last, Some(start));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.busy {
            tick(&mut controller, false, start);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        tick(&mut controller, true, hour - Duration::from_secs(1));
        tick(&mut controller, false, hour);
        assert_eq!(controller.last, Some(start));
        assert!(received.try_recv().is_err());
        tick(&mut controller, true, hour);
        assert_eq!(controller.last, Some(hour));
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            Channel::Stable
        );
        assert!(!controller.open);
    }

    #[test]
    fn update_card_only_appears_for_an_offer_and_can_be_clicked() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let version = semver::Version::new(0, 99, 0);
        let frame = |available: bool, events| {
            let mut chosen = false;
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        chosen = offer_card(ui, available.then_some(&version));
                        ui.label("Repository");
                    });
                },
            );
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            let bounds = |label| {
                nodes.iter().find_map(|(_, node)| {
                    (node.label().or_else(|| node.value()) == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
            };
            let card = bounds("Update Shovel");
            let repository = bounds("Repository").unwrap();
            output.drop_without_applying_deltas();
            (card, repository, chosen)
        };
        let (hidden, idle_repository, _) = frame(false, vec![]);
        assert!(hidden.is_none());
        let (visible, repository, _) = frame(true, vec![]);
        let card = visible.unwrap();
        assert!(card.x1 <= repository.x0 && idle_repository.x0 < repository.x0);
        let pos = egui::pos2(
            ((card.x0 + card.x1) / 2.) as f32,
            ((card.y0 + card.y1) / 2.) as f32,
        );
        frame(true, vec![egui::Event::PointerMoved(pos)]);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(true, vec![button(true)]);
        assert!(frame(true, vec![button(false)]).2);
        assert!(frame(false, vec![]).0.is_none());
    }
}
