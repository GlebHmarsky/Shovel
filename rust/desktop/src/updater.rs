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
    transfer(url, &mut output, max, cancel, None)?;
    Ok(output)
}
fn transfer(
    url: &str,
    out: &mut impl Write,
    max: u64,
    cancel: &Cancellation,
    progress: Option<&mpsc::Sender<i16>>,
) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_global(Some(Duration::from_secs(600)))
        .timeout_recv_body(Some(Duration::from_secs(30)))
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
fn trusted_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ShovelRust/TrustedUpdates")
}
fn check(channel: Channel, cancel: &Cancellation) -> Result<Option<Offer>, String> {
    let feed = bytes(&channel.feed_url(), updates::MAX_FEED_BYTES as u64, cancel)?;
    let signature = bytes(&format!("{}.sig", channel.feed_url()), 2048, cancel)?;
    let release =
        updates::verify_feed(channel, &feed, &signature).map_err(|_| text("RustUpdateInvalid"))?;
    // Immutable, signed floor records shared by this user's Rust profiles.
    let directory = trusted_directory().join(channel.id());
    fs::create_dir_all(&directory).map_err(failure)?;
    let records = fs::read_dir(&directory)
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    if records.len() > 1024 {
        return Err(text("RustUpdateInvalid"));
    }
    for entry in records {
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
            serde_json::from_slice(&data).map_err(|_| text("RustUpdateInvalid"))?;
        let old = updates::verify_feed(channel, &old_feed, &old_sig)
            .map_err(|_| text("RustUpdateInvalid"))?;
        if release.version() < old.version() {
            return Err(text("RustUpdateReplay"));
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
        .ok_or_else(|| text("RustUpdateInvalid"))?;
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
    Checked(u64, Result<Option<Offer>, String>),
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
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn tick(&mut self, ctx: &egui::Context, channel: Channel, periodic: bool) {
        if self.channel != channel {
            self.cancel();
            self.generation += 1;
            self.busy = false;
            self.restart_requested = false;
            self.channel = channel;
            self.offer = None;
            self.ready = false;
            self.last = None;
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
                            self.status =
                                format("UpdateOffered", &[&offer.release.version().to_string()]);
                            self.offer = Some(offer);
                        }
                        Ok(None) => self.status = text("RustUpdateCurrent"),
                        Err(error) => self.status = error,
                        _ => {}
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
        let due = self.last.is_none()
            || (periodic
                && self
                    .last
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(4 * 3600)));
        if due && !self.busy {
            self.check(ctx);
        }
        if periodic {
            ctx.request_repaint_after(Duration::from_secs(60));
        }
    }
    pub fn check(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.busy = true;
        self.last = Some(Instant::now());
        self.offer = None;
        self.ready = false;
        self.status = text("RustUpdateChecking");
        self.cancel = Cancellation::default();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let channel = self.channel;
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Checked(generation, check(channel, &cancel)));
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
            .enabled(!self.restart_requested)
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
                            .add_enabled(!blocked, egui::Button::new(text("RustUpdateRestart")))
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
                                let (progress, rx) = mpsc::channel();
                                let pt = tx.clone();
                                let pc = ctx.clone();
                                let worker = std::thread::spawn(move || {
                                    while let Ok(p) = rx.recv() {
                                        let _ = pt.send(Event::Progress(generation, p));
                                        pc.request_repaint();
                                    }
                                });
                                manager
                                    .download_updates(
                                        &UpdateInfo {
                                            TargetFullRelease: offer.asset.clone(),
                                            ..Default::default()
                                        },
                                        Some(progress),
                                    )
                                    .map_err(failure)?;
                                let _ = worker.join();
                                offer
                                    .release
                                    .verify_package(File::open(target).map_err(failure)?)
                                    .map_err(|_| text("RustUpdateInvalid"))?;
                                Ok(offer)
                            })();
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
}
