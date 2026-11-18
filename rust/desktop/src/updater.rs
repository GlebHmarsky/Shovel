//! Rust-only signed update transport. Velopack receives only a frozen verified offer.
use crate::strings::{format, text};
use eframe::egui;
use shovel_core::{
    process::Cancellation,
    updates::{self, Channel, VerifiedRelease},
};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    panic::{AssertUnwindSafe, catch_unwind},
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
fn bytes(url: &str, max: u64, cancel: &Cancellation) -> Result<Vec<u8>, CheckError> {
    let mut output = Vec::new();
    transfer(
        url,
        &mut output,
        ResponseSize::AtMost(max),
        CHECK_HTTP_TIMEOUT,
        cancel,
        None,
    )
    .map_err(|key| match key {
        "RustUpdateInvalid" => CheckError::Rejected(key),
        _ => CheckError::Unavailable(text(key)),
    })?;
    Ok(output)
}
#[derive(Clone, Copy)]
enum ResponseSize {
    AtMost(u64),
    Exact(u64),
}
impl ResponseSize {
    fn limit(self) -> u64 {
        match self {
            Self::AtMost(size) | Self::Exact(size) => size,
        }
    }
    fn accepts(self, length: u64) -> bool {
        match self {
            Self::AtMost(max) => length <= max,
            Self::Exact(size) => length == size,
        }
    }
}
fn transfer(
    url: &str,
    out: &mut impl Write,
    size: ResponseSize,
    timeout: Duration,
    cancel: &Cancellation,
    progress: Option<&mpsc::Sender<i16>>,
) -> Result<(), &'static str> {
    if cancel.is_cancelled() {
        return Err("ActionResultCancelled");
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
    let response = agent
        .get(url)
        .header("User-Agent", "ShovelRust")
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|_| "RustUpdateFailed")?;
    receive_response(response, out, size, cancel, progress)
}
fn receive_response(
    mut response: ureq::http::Response<ureq::Body>,
    out: &mut impl Write,
    size: ResponseSize,
    cancel: &Cancellation,
    progress: Option<&mpsc::Sender<i16>>,
) -> Result<(), &'static str> {
    if response.status() != ureq::http::StatusCode::OK
        || response
            .headers()
            .contains_key(ureq::http::header::CONTENT_RANGE)
        || response
            .body()
            .content_length()
            .is_some_and(|length| !size.accepts(length))
    {
        return Err("RustUpdateInvalid");
    }
    copy_response(response.body_mut().as_reader(), out, size, cancel, progress)
}
fn copy_response(
    mut reader: impl Read,
    out: &mut impl Write,
    size: ResponseSize,
    cancel: &Cancellation,
    progress: Option<&mpsc::Sender<i16>>,
) -> Result<(), &'static str> {
    let mut total = 0;
    let mut last_percent = None;
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err("ActionResultCancelled");
        }
        let read = reader.read(&mut buffer);
        if cancel.is_cancelled() {
            return Err("ActionResultCancelled");
        }
        let n = match read {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err("RustUpdateIncomplete");
            }
            result => result.map_err(|_| "RustUpdateFailed")?,
        };
        if n == 0 {
            return if size.accepts(total) {
                Ok(())
            } else {
                Err("RustUpdateIncomplete")
            };
        }
        total += n as u64;
        if total > size.limit() {
            return Err("RustUpdateInvalid");
        }
        out.write_all(&buffer[..n])
            .map_err(|_| "RustUpdateCacheUnavailable")?;
        if let Some(tx) = progress {
            let percent = (total.saturating_mul(100) / size.limit().max(1)).min(100) as i16;
            if last_percent != Some(percent) {
                let _ = tx.send(percent);
                last_percent = Some(percent);
            }
        }
    }
}
fn background_result<T, E: From<String>>(run: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    catch_unwind(AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(text("RustUpdateWorkerFailed").into()))
}

fn with_progress(
    run: impl FnOnce(mpsc::Sender<i16>) -> Result<(), String>,
    report: impl Fn(i16) + Send + 'static,
) -> Result<(), String> {
    std::thread::scope(|scope| {
        let (progress, rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .spawn_scoped(scope, move || {
                while let Ok(value) = rx.recv() {
                    report(value);
                }
            })
            .map_err(|_| text("RustUpdateWorkerFailed"))?;
        let result = run(progress);
        worker.join().map_err(|_| text("RustUpdateWorkerFailed"))?;
        result
    })
}
fn trusted_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ShovelRust/TrustedUpdates")
}
#[derive(Debug)]
enum CheckError {
    Unavailable(String),
    Rejected(&'static str),
}
impl From<String> for CheckError {
    fn from(error: String) -> Self {
        Self::Unavailable(error)
    }
}
impl From<updates::UpdateError> for CheckError {
    fn from(error: updates::UpdateError) -> Self {
        use std::io::ErrorKind;
        use updates::UpdateError;
        match error {
            UpdateError::Replay => Self::Rejected("RustUpdateReplay"),
            UpdateError::Io(e) => Self::Unavailable(text(match e.kind() {
                ErrorKind::Interrupted => "ActionResultCancelled",
                ErrorKind::WouldBlock => "RustUpdateCheckingElsewhere",
                _ => "RustUpdateFloorUnavailable",
            })),
            _ => Self::Rejected("RustUpdateInvalid"),
        }
    }
}
fn recheck_offer(offer: &Offer, cancel: &Cancellation) -> Result<(), String> {
    updates::floor::recheck(&trusted_directory(), &offer.release, cancel).map_err(|error| {
        match CheckError::from(error) {
            CheckError::Unavailable(message) => message,
            CheckError::Rejected(key) => text(key),
        }
    })
}
fn check(channel: Channel, cancel: &Cancellation) -> Result<Option<Offer>, CheckError> {
    check_for_installation(
        channel,
        &semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
        &trusted_directory(),
        cancel,
    )
}
fn check_for_installation(
    channel: Channel,
    installed: &semver::Version,
    trusted: &Path,
    cancel: &Cancellation,
) -> Result<Option<Offer>, CheckError> {
    let feed = bytes(&channel.feed_url(), updates::MAX_FEED_BYTES as u64, cancel)?;
    let signature = bytes(&format!("{}.sig", channel.feed_url()), 2048, cancel)?;
    let release = updates::floor::verify_and_record(trusted, channel, &feed, &signature, cancel)?;
    if !release.is_newer_than(installed) {
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
        if asset.FileName != self.offer.release.file_name()
            || asset.SHA256 != self.offer.asset.SHA256
        {
            return Err(velopack::Error::InvalidPackage("RustUpdateInvalid".into()));
        }
        let mut file = File::create(path)?;
        transfer(
            &self.offer.release.package_url(),
            &mut file,
            ResponseSize::Exact(self.offer.release.size()),
            Duration::from_secs(600),
            &self.cancel,
            progress.as_ref(),
        )
        .map_err(|key| velopack::Error::Other(key.into()))?;
        file.sync_all()?;
        drop(file);
        self.offer
            .release
            .verify_package_cancellable(File::open(path)?, || self.cancel.is_cancelled())
            .map_err(|error| match error {
                updates::UpdateError::Io(error) => velopack::Error::Io(error),
                _ => velopack::Error::InvalidPackage("RustUpdateInvalid".into()),
            })
    }
}
fn download_error(error: velopack::Error) -> String {
    use velopack::Error;
    match error {
        Error::Other(key)
            if matches!(
                key.as_str(),
                "RustUpdateIncomplete"
                    | "RustUpdateInvalid"
                    | "ActionResultCancelled"
                    | "RustUpdateCacheUnavailable"
            ) =>
        {
            text(&key)
        }
        Error::InvalidPackage(_) | Error::SizeInvalid(..) | Error::ChecksumInvalid(..) => {
            text("RustUpdateInvalid")
        }
        Error::Io(_) | Error::FileNotFound(_) => text("RustUpdateCacheUnavailable"),
        _ => text("RustUpdateFailed"),
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
fn installed_locator() -> Result<velopack::locator::VelopackLocator, String> {
    velopack::locator::auto_locate_app_manifest(velopack::locator::LocationContext::FromCurrentExe)
        .map_err(|_| text("RustUpdateInstallNeeded"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CachedPackage {
    Verified,
    MissingOrDamaged,
    Unavailable,
}

struct Discovered {
    offer: Offer,
    cached: CachedPackage,
}

fn discover(channel: Channel, cancel: Cancellation) -> Result<Option<Discovered>, CheckError> {
    check(channel, &cancel).map(|offer| {
        offer.map(|offer| {
            let cached = std::env::current_exe()
                .ok()
                .and_then(|executable| installed_cache_for_check(&executable))
                .map_or(CachedPackage::Unavailable, |directory| {
                    let package = directory.join(offer.release.file_name());
                    inspect_cached_package(&package, &offer.release, &cancel)
                });
            Discovered { offer, cached }
        })
    })
}

fn installed_cache_for_check(executable: &Path) -> Option<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_ADD_FILE, FILE_FLAG_BACKUP_SEMANTICS};

    let directory = executable.parent()?;
    let root = if directory.join("Update.exe").is_file() {
        directory
    } else {
        directory
            .ancestors()
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("current"))
            })?
            .parent()?
    };
    let config = velopack::locator::create_config_from_root_dir(root);
    if !config.UpdateExePath.is_file() {
        return None;
    }
    let mut manifest = String::new();
    const MAX_MANIFEST: u64 = 64 * 1024;
    File::open(&config.ManifestPath)
        .ok()?
        .take(MAX_MANIFEST + 1)
        .read_to_string(&mut manifest)
        .ok()?;
    if manifest.len() as u64 > MAX_MANIFEST
        || velopack::bundle::read_manifest_from_string(&manifest).ok()?.id != updates::PRODUCT
    {
        return None;
    }

    // Velopack's locator can create a probe file and copy an updater to
    // LocalAppData. Discovery only opens existing paths.
    if OpenOptions::new()
        .access_mode(FILE_ADD_FILE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)
        .is_ok()
    {
        return Some(config.PackagesDir);
    }
    let fallback = velopack::known_path::get_local_app_data()
        .ok()?
        .join(updates::PRODUCT);
    fallback
        .join("Update.exe")
        .is_file()
        .then(|| fallback.join("packages"))
}

fn open_package_read_only(package: &Path) -> std::io::Result<File> {
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(package)
}

fn inspect_cached_package(
    package: &Path,
    release: &VerifiedRelease,
    cancel: &Cancellation,
) -> CachedPackage {
    let file = match open_package_read_only(package) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return CachedPackage::MissingOrDamaged;
        }
        Err(_) => return CachedPackage::Unavailable,
    };
    match release.verify_package_cancellable(file, || cancel.is_cancelled()) {
        Ok(()) => CachedPackage::Verified,
        Err(updates::UpdateError::PackageMismatch) => CachedPackage::MissingOrDamaged,
        Err(_) => CachedPackage::Unavailable,
    }
}

#[derive(Debug)]
enum ApplyError {
    Retry(String),
    DownloadAgain,
    OfferChanged(String),
}
impl From<String> for ApplyError {
    fn from(message: String) -> Self {
        Self::Retry(message)
    }
}
/// Hold the verified file against writes and replacement until Shovel exits.
/// Velopack waits for this process before applying the update.
fn verify_for_restart(package: &Path, release: &VerifiedRelease) -> Result<File, ApplyError> {
    let input = open_package_read_only(package).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ApplyError::DownloadAgain,
        _ => ApplyError::Retry(text("RustUpdateCacheUnavailable")),
    })?;
    release.verify_package(&input).map_err(|error| match error {
        updates::UpdateError::PackageMismatch => ApplyError::DownloadAgain,
        _ => ApplyError::Retry(text("RustUpdateCacheUnavailable")),
    })?;
    Ok(input)
}
fn confirm_published_offer(
    approved: &VerifiedRelease,
    published: Result<Option<Offer>, CheckError>,
) -> Result<(), ApplyError> {
    match published {
        Ok(Some(current)) if approved.same_package(&current.release) => Ok(()),
        Ok(_) => Err(ApplyError::OfferChanged(text("RustUpdateOfferChanged"))),
        Err(CheckError::Rejected(key)) => Err(ApplyError::OfferChanged(text(key))),
        Err(CheckError::Unavailable(error)) => Err(ApplyError::Retry(error)),
    }
}

fn handoff_update(
    manager: &UpdateManager,
    offer: &Offer,
    restart_args: impl IntoIterator<Item = OsString>,
) -> Result<(), ApplyError> {
    manager
        .wait_exit_then_apply_updates(&offer.asset, false, true, restart_args)
        .map_err(|error| match error {
            // Velopack emits this variant when its selected package path is absent.
            // Failure to start Update.exe is an IO error instead.
            velopack::Error::FileNotFound(_) => ApplyError::DownloadAgain,
            _ => ApplyError::Retry(text("RustUpdateHelperFailed")),
        })
}
enum Event {
    Applied(Result<File, ApplyError>),
    Checked(u64, Result<Option<Discovered>, CheckError>),
    Downloaded(u64, Result<Offer, String>),
    Progress(u64, i16),
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Checking,
    Downloading(u8),
    Applying,
}
pub struct Controller {
    pub open: bool,
    pub restart_requested: bool,
    pub ready: bool,
    applying_package: Option<File>,
    generation: u64,
    tools_confirmed: bool,
    channel: Channel,
    offer: Option<Offer>,
    status: String,
    phase: Phase,
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
            applying_package: None,
            generation: 0,
            tools_confirmed: false,
            channel: Channel::Beta,
            offer: None,
            status: String::new(),
            phase: Phase::Idle,
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
    fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }
    pub fn exit_ready(&self) -> bool {
        self.applying_package.is_some()
    }
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
    pub fn profile_save_failed(&mut self) {
        self.restart_requested = false;
        self.open = true;
        self.status = text("RustUpdateSaveBlocked");
    }
    pub fn tool_opened(&mut self) {
        self.tools_confirmed = false;
    }
    pub fn needs_tool_confirmation(&self, tools_opened: bool) -> bool {
        tools_opened && !self.tools_confirmed
    }
    pub fn available_version(&self, channel: Channel) -> Option<&semver::Version> {
        self.offer
            .as_ref()
            .filter(|offer| offer.release.channel() == channel)
            .map(|offer| offer.release.version())
    }
    pub fn tick(&mut self, ctx: &egui::Context, channel: Channel, periodic: bool) {
        self.tick_at(ctx, channel, periodic, Instant::now(), discover);
    }
    fn tick_at(
        &mut self,
        ctx: &egui::Context,
        channel: Channel,
        periodic: bool,
        now: Instant,
        run: impl FnOnce(Channel, Cancellation) -> Result<Option<Discovered>, CheckError> + Send + 'static,
    ) {
        if self.channel != channel {
            self.cancel();
            self.generation += 1;
            self.phase = Phase::Idle;
            self.restart_requested = false;
            self.channel = channel;
            self.offer = None;
            self.tools_confirmed = false;
            self.ready = false;
            self.last = None;
            self.status = text("RustUpdateChecking");
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Applied(result) => {
                    self.phase = Phase::Idle;
                    match result {
                        Ok(package) => self.applying_package = Some(package),
                        Err(error) => {
                            self.status = match error {
                                ApplyError::Retry(message) => message,
                                ApplyError::OfferChanged(message) => {
                                    self.offer = None;
                                    self.ready = false;
                                    self.tools_confirmed = false;
                                    message
                                }
                                ApplyError::DownloadAgain => {
                                    self.ready = false;
                                    self.tools_confirmed = false;
                                    text("RustUpdateDownloadAgain")
                                }
                            };
                            self.restart_requested = false;
                            self.open = true;
                        }
                    }
                }
                Event::Progress(id, p)
                    if id == self.generation && matches!(self.phase, Phase::Downloading(_)) =>
                {
                    let percent = p.clamp(0, 100) as u8;
                    self.phase = Phase::Downloading(percent);
                    self.status = format("RustUpdateProgress", &[&percent.to_string()])
                }
                Event::Checked(id, result) if id == self.generation => {
                    self.phase = Phase::Idle;
                    match result {
                        Ok(Some(Discovered { offer, cached }))
                            if offer.release.channel() == self.channel =>
                        {
                            let same_release = self.offer.as_ref().is_some_and(|current| {
                                current.release.same_package(&offer.release)
                            });
                            self.ready = match cached {
                                CachedPackage::Verified => true,
                                CachedPackage::MissingOrDamaged => false,
                                CachedPackage::Unavailable => self.ready && same_release,
                            };
                            self.tools_confirmed &= same_release && self.ready;
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
                    self.phase = Phase::Idle;
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
        if self.busy() || self.restart_requested || self.exit_ready() || self.workers.stopping() {
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
    pub fn open_window(&mut self, ctx: &egui::Context) {
        self.open_at(ctx, Instant::now(), discover);
    }

    fn open_at(
        &mut self,
        ctx: &egui::Context,
        now: Instant,
        run: impl FnOnce(Channel, Cancellation) -> Result<Option<Discovered>, CheckError> + Send + 'static,
    ) {
        self.open = true;
        self.begin_check(ctx, now, run);
    }
    fn begin_check(
        &mut self,
        ctx: &egui::Context,
        now: Instant,
        run: impl FnOnce(Channel, Cancellation) -> Result<Option<Discovered>, CheckError> + Send + 'static,
    ) {
        if self.busy()
            || self.restart_requested
            || self.exit_ready()
            || self.workers.stopping()
            || !self.workers.idle()
        {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.phase = Phase::Checking;
        self.last = Some(now);
        self.status = text("RustUpdateChecking");
        self.cancel = Cancellation::default();
        let tx = self.tx.clone();
        let worker_ctx = ctx.clone();
        let channel = self.channel;
        let cancel = self.cancel.clone();
        let lease = self.workers.start(cancel.clone());
        let worker = std::thread::Builder::new().spawn(move || {
            let result = background_result(|| run(channel, cancel.clone()));
            let result = if cancel.is_cancelled() {
                Err(text("ActionResultCancelled").into())
            } else {
                result
            };
            drop(lease);
            let _ = tx.send(Event::Checked(generation, result));
            worker_ctx.request_repaint();
        });
        if worker.is_err() {
            let _ = self.tx.send(Event::Checked(
                generation,
                Err(text("RustUpdateWorkerFailed").into()),
            ));
            ctx.request_repaint();
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, blocked: bool, tools_opened: bool) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new(text("RustUpdatesTitle"))
            .open(&mut open)
            .enabled(!self.restart_requested && !self.workers.stopping())
            .default_width(510.)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Shovel {} · {}",
                    env!("CARGO_PKG_VERSION"),
                    self.channel.label()
                ));
                let cancelling = self.cancel.is_cancelled()
                    && matches!(self.phase, Phase::Checking | Phase::Downloading(_));
                let status = if cancelling {
                    text("RustUpdateCancelling")
                } else {
                    self.status.clone()
                };
                match self.phase {
                    Phase::Downloading(percent) => {
                        ui.add(egui::ProgressBar::new(f32::from(percent) / 100.).text(status));
                    }
                    Phase::Checking | Phase::Applying => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(status);
                        });
                    }
                    Phase::Idle => {
                        ui.label(status);
                    }
                }
                if let Some(offer) = &self.offer {
                    release_notes(ui, offer.release.notes());
                }
                if self.ready && tools_opened {
                    ui.checkbox(&mut self.tools_confirmed, text("UpdateToolsQuestion"));
                }
                let blocked = blocked || self.needs_tool_confirmation(tools_opened);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!self.busy(), egui::Button::new(text("UpdateRetry")))
                        .clicked()
                    {
                        self.begin_check(ctx, Instant::now(), discover);
                    }
                    if matches!(self.phase, Phase::Checking | Phase::Downloading(_))
                        && ui
                            .add_enabled(!cancelling, egui::Button::new(text("Cancel")))
                            .clicked()
                    {
                        self.cancel();
                    }
                    if self.ready {
                        if ui
                            .add_enabled(
                                !blocked && !self.busy(),
                                egui::Button::new(text("RustUpdateRestart")),
                            )
                            .on_disabled_hover_text(text("RustUpdateBlocked"))
                            .clicked()
                        {
                            self.restart_requested = true;
                        }
                    } else if self.offer.is_some()
                        && !self.busy()
                        && ui.button(text("RustUpdateDownload")).clicked()
                    {
                        self.generation += 1;
                        let generation = self.generation;
                        self.phase = Phase::Downloading(0);
                        self.status = format("RustUpdateProgress", &["0"]);
                        self.cancel = Cancellation::default();
                        let offer = self.offer.clone().unwrap();
                        let cancel = self.cancel.clone();
                        let tx = self.tx.clone();
                        let worker_ctx = ctx.clone();
                        let lease = self.workers.start(cancel.clone());
                        let worker = std::thread::Builder::new().spawn(move || {
                            let result = background_result(|| {
                                recheck_offer(&offer, &cancel)?;
                                let manager = manager(&offer, cancel.clone())?;
                                let locator = installed_locator()?;
                                let directory = locator.get_packages_dir();
                                {
                                    let _lock = locator
                                        .try_get_exclusive_lock()
                                        .map_err(|_| text("RustUpdateCacheUnavailable"))?;
                                    if offer
                                        .release
                                        .prepare_cached_download(&directory, || {
                                            cancel.is_cancelled()
                                        })
                                        .map_err(|_| text("RustUpdateCacheUnavailable"))?
                                    {
                                        return Ok(offer);
                                    }
                                }
                                let pt = tx.clone();
                                let pc = worker_ctx.clone();
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
                                            .map_err(download_error)
                                    },
                                    move |p| {
                                        let _ = pt.send(Event::Progress(generation, p));
                                        pc.request_repaint();
                                    },
                                )?;
                                offer
                                    .release
                                    .verify_package_cancellable(
                                        File::open(directory.join(offer.release.file_name()))
                                            .map_err(failure)?,
                                        || cancel.is_cancelled(),
                                    )
                                    .map_err(|_| text("RustUpdateInvalid"))?;
                                Ok(offer)
                            });
                            drop(lease);
                            let result = if cancel.is_cancelled() {
                                Err(text("ActionResultCancelled"))
                            } else {
                                result
                            };
                            let _ = tx.send(Event::Downloaded(generation, result));
                            worker_ctx.request_repaint();
                        });
                        if worker.is_err() {
                            let _ = self.tx.send(Event::Downloaded(
                                generation,
                                Err(text("RustUpdateWorkerFailed")),
                            ));
                            ctx.request_repaint();
                        }
                    }
                    dismiss = ui
                        .button(text(if self.offer.is_some() {
                            "RustUpdateLater"
                        } else {
                            "UpdateClose"
                        }))
                        .clicked();
                });
            });
        self.open = open && !dismiss;
    }
    pub fn apply(
        &mut self,
        ctx: &egui::Context,
        git: Option<std::sync::Arc<shovel_core::git::Git>>,
        repositories: Vec<crate::update_restart::Repository>,
    ) {
        let offer = self.offer.clone();
        let ready = self.ready;
        let channel = self.channel;
        self.start_apply(
            ctx,
            move || {
                let offer = offer
                    .filter(|o| ready && o.release.channel() == channel)
                    .ok_or_else(|| text("RustUpdateInvalid"))?;
                let cancel = Cancellation::default();
                confirm_published_offer(&offer.release, check(channel, &cancel))?;
                crate::update_restart::check_repositories(git.as_deref(), &repositories)?;
                crate::update_restart::other_instances()?;
                let target = installed_locator()?
                    .get_packages_dir()
                    .join(offer.release.file_name());
                let package = verify_for_restart(&target, &offer.release)?;
                recheck_offer(&offer, &cancel)?;
                Ok((package, manager(&offer, cancel)?, offer))
            },
            |(package, manager, offer)| {
                handoff_update(&manager, &offer, std::env::args_os().skip(1))?;
                Ok(package)
            },
        );
    }

    fn start_apply<T>(
        &mut self,
        ctx: &egui::Context,
        prepare: impl FnOnce() -> Result<T, ApplyError> + Send + 'static,
        handoff: impl FnOnce(T) -> Result<File, ApplyError> + Send + 'static,
    ) {
        self.phase = Phase::Applying;
        self.status = text("UpdateInstalling");
        let tx = self.tx.clone();
        let worker_ctx = ctx.clone();
        let worker = std::thread::Builder::new().spawn(move || {
            // Retry is safe during preparation. After handoff the helper may already be waiting.
            let result = background_result(prepare).and_then(handoff);
            let _ = tx.send(Event::Applied(result));
            worker_ctx.request_repaint();
        });
        if worker.is_err() {
            let _ = self.tx.send(Event::Applied(Err(ApplyError::Retry(text(
                "RustUpdateWorkerFailed",
            )))));
            ctx.request_repaint();
        }
    }
}

fn release_notes(ui: &mut egui::Ui, notes: &str) {
    if notes.trim().is_empty() {
        return;
    }
    ui.strong(text("RustUpdateWhatsNew"));
    egui::ScrollArea::vertical()
        .id_salt("update-release-notes")
        .max_height(150.)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add(egui::Label::new(notes).wrap().selectable(true));
        });
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "downloads the current public Beta and requires its expected version and commit"]
    fn published_beta_downloads_with_native_updater() {
        let version =
            semver::Version::parse(&std::env::var("SHOVEL_VERIFY_BETA_VERSION").unwrap()).unwrap();
        let commit = std::env::var("SHOVEL_VERIFY_BETA_COMMIT").unwrap();
        let root = tempfile::tempdir().unwrap();
        let trusted = root.path().join("trusted");
        let cancel = Cancellation::default();
        let discover = |installed: &semver::Version| {
            check_for_installation(Channel::Beta, installed, &trusted, &cancel)
        };
        let offer = discover(&semver::Version::new(0, 0, 0)).unwrap().unwrap();
        assert_eq!(offer.release.version(), &version);
        assert_eq!(offer.release.source_commit(), commit);
        let target = root.path().join(offer.release.file_name());
        assert_eq!(
            inspect_cached_package(&target, &offer.release, &cancel),
            CachedPackage::MissingOrDamaged
        );
        let source = FrozenSource {
            offer: offer.clone(),
            cancel: cancel.clone(),
        };
        let (send, progress) = mpsc::channel();
        source
            .download_release_entry(&offer.asset, &target, Some(send))
            .unwrap();
        let progress: Vec<_> = progress.into_iter().collect();
        assert_eq!(progress.last(), Some(&100));
        assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(target.metadata().unwrap().len(), offer.release.size());
        let cached = inspect_cached_package(&target, &offer.release, &cancel);
        assert_eq!(cached, CachedPackage::Verified);
        let verified = verify_for_restart(&target, &offer.release).unwrap();
        let mut controller = Controller::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        controller
            .tx
            .send(Event::Checked(
                0,
                Ok(Some(Discovered {
                    offer: offer.clone(),
                    cached,
                })),
            ))
            .unwrap();
        controller.tick(&ctx, Channel::Beta, false);
        assert!(controller.ready && !controller.restart_requested && !controller.exit_ready());
        assert!(!controller.open);
        controller.open = true;
        let label = text("RustUpdateRestart");
        let frame = |controller: &mut Controller| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 620.),
                    )),
                    ..Default::default()
                },
                |_| controller.show(&ctx, false, false),
            )
        };
        frame(&mut controller).drop_without_applying_deltas();
        let output = frame(&mut controller);
        let restart_available = output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|access| {
                access.nodes.iter().any(|(_, node)| {
                    node.label() == Some(label.as_str()) && !node.is_disabled()
                })
            });
        output.drop_without_applying_deltas();
        assert!(restart_available);
        controller.tx.send(Event::Applied(Ok(verified))).unwrap();
        controller.tick(&ctx, Channel::Beta, false);
        assert!(controller.exit_ready());
        assert!(
            File::open(&target).is_ok(),
            "the updater can still read the package"
        );
        assert!(OpenOptions::new().write(true).open(&target).is_err());
        assert!(std::fs::remove_file(&target).is_err());
        drop(controller);
        assert!(OpenOptions::new().write(true).open(&target).is_ok());
        updates::floor::recheck(&trusted, &offer.release, &cancel).unwrap();
        assert!(discover(&version).unwrap().is_none());
    }
    use super::*;

    fn published_offer(feed: &[u8], signature: &[u8]) -> Offer {
        Offer {
            release: updates::verify_feed(Channel::Beta, feed, signature).unwrap(),
            asset: serde_json::from_slice::<VelopackAssetFeed>(feed)
                .unwrap()
                .Assets
                .remove(0),
        }
    }

    #[test]
    fn discovery_drops_readiness_for_changed_or_damaged_packages_but_keeps_a_sharing_retry() {
        let older = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.3.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.3.json.sig"),
        );
        let newer = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        );
        let ctx = egui::Context::default();
        let mut controller = Controller::default();
        for (offer, cached, ready) in [
            (older.clone(), CachedPackage::Unavailable, false),
            (older.clone(), CachedPackage::Verified, true),
            (older.clone(), CachedPackage::Unavailable, true),
            (older.clone(), CachedPackage::MissingOrDamaged, false),
            (older, CachedPackage::Verified, true),
            (newer.clone(), CachedPackage::Unavailable, false),
            (newer.clone(), CachedPackage::Verified, true),
            (newer, CachedPackage::MissingOrDamaged, false),
        ] {
            controller
                .tx
                .send(Event::Checked(0, Ok(Some(Discovered { offer, cached }))))
                .unwrap();
            controller.tick(&ctx, Channel::Beta, false);
            assert_eq!(controller.ready, ready);
            assert!(!controller.restart_requested && !controller.exit_ready());
        }
    }

    #[test]
    fn install_cache_discovery_reads_the_manifest_without_touching_installation_files() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let current = root.join("current");
        std::fs::create_dir(&current).unwrap();
        let app = current.join("Shovel.exe");
        let updater = root.join("Update.exe");
        std::fs::write(&updater, b"fixture placeholder").unwrap();
        let marker = root.join(".velopack_dir_test");
        std::fs::write(&marker, b"keep these bytes").unwrap();
        let manifest = current.join("sq.version");
        let set_product = |product| {
            std::fs::write(
                &manifest,
                format!(
                    "<package><metadata><id>{product}</id><version>0.50.17</version>+                     <mainExe>Shovel.exe</mainExe></metadata></package>"
                ),
            )
            .unwrap();
        };
        set_product("Shovel");
        assert_eq!(installed_cache_for_check(&app), None);
        set_product(updates::PRODUCT);
        assert_eq!(installed_cache_for_check(&app), Some(root.join("packages")));
        assert_eq!(std::fs::read(&marker).unwrap(), b"keep these bytes");
        assert_eq!(std::fs::read(&updater).unwrap(), b"fixture placeholder");
        assert!(!root.join("packages").exists());
    }

    #[test]
    fn native_helper_launch_failure_keeps_the_package_ready_for_retry() {
        let directory = tempfile::tempdir().unwrap();
        let config = velopack::locator::create_config_from_root_dir(directory.path());
        std::fs::create_dir(&config.CurrentBinaryDir).unwrap();
        std::fs::create_dir(&config.PackagesDir).unwrap();
        std::fs::write(
            &config.ManifestPath,
            "<package><metadata><id>ShovelRust</id><version>0.50.3</version>+             <mainExe>Shovel.exe</mainExe></metadata></package>",
        )
        .unwrap();
        // Windows rejects this file as a malformed executable, so no helper starts.
        std::fs::write(&config.UpdateExePath, b"invalid executable").unwrap();
        let offer = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        );
        let package = config.PackagesDir.join(offer.release.file_name());
        let manager = UpdateManager::new(
            FrozenSource {
                offer: offer.clone(),
                cancel: Cancellation::default(),
            },
            None,
            Some(config),
        )
        .unwrap();
        assert!(matches!(
            handoff_update(&manager, &offer, []),
            Err(ApplyError::DownloadAgain)
        ));
        // Verification is covered by the public-package test. This fixture
        // checks only the result of handing an existing path to the launcher.
        std::fs::write(&package, b"package fixture").unwrap();
        let result = handoff_update(&manager, &offer, []);
        let error = result.unwrap_err();
        let mut controller = Controller {
            offer: Some(offer),
            ready: true,
            restart_requested: true,
            phase: Phase::Applying,
            ..Default::default()
        };
        controller.tx.send(Event::Applied(Err(error))).unwrap();
        controller.tick(&egui::Context::default(), Channel::Beta, false);
        assert_eq!(controller.status, text("RustUpdateHelperFailed"));
        assert!(controller.open && controller.ready);
        assert!(!controller.restart_requested && !controller.exit_ready());
        assert_eq!(std::fs::read(package).unwrap(), b"package fixture");
    }

    #[test]
    fn restart_rechecks_the_signed_release_and_requires_new_approval_after_withdrawal() {
        let old = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.3.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.3.json.sig"),
        );
        let new = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        );
        assert!(confirm_published_offer(&old.release, Ok(Some(old.clone()))).is_ok());
        let mut controller = Controller::default();
        for (result, retained) in [
            (Ok(None), false),
            (Ok(Some(new)), false),
            (Err(CheckError::Rejected("RustUpdateInvalid")), false),
            (Err(CheckError::Unavailable(text("RustUpdateFailed"))), true),
        ] {
            controller.offer = Some(old.clone());
            controller.ready = true;
            controller.tools_confirmed = true;
            controller.restart_requested = true;
            let error = confirm_published_offer(&old.release, result).unwrap_err();
            controller
                .tx
                .send(Event::Applied(Err(error)))
                .unwrap();
            controller.tick(&egui::Context::default(), Channel::Beta, false);
            assert_eq!(controller.offer.is_some(), retained);
            assert_eq!(controller.ready, retained);
            assert_eq!(controller.tools_confirmed, retained);
            assert!(controller.open && !controller.restart_requested && !controller.exit_ready());
        }
    }

    #[test]
    fn pending_restart_survives_a_read_found_after_the_settings_save() {
        let ctx = egui::Context::default();
        let mut app = crate::workspace_interaction_tests::workspace();
        app.tabs.clear();
        app.saving = true;
        app.closing = true;
        app.updater.restart_requested = true;
        app.updater.ready = true;
        let pending = app.read_workers.start_required();
        let profile = tempfile::tempdir().unwrap();
        let store = crate::SettingsStore::new(profile.path().into());
        app.sender
            .send(crate::Event::Saved(Box::new(store), Ok(())))
            .unwrap();
        app.receive(&ctx);
        assert!(app.pending_exit && app.updater.restart_requested && !app.exit_ready);
        assert!(!app.closing && !app.saving);
        drop(pending);
        app.sender.send(crate::Event::ToolFinished(Ok(()))).unwrap();
        app.receive(&ctx);
        assert!(!app.pending_exit && !app.exit_ready && !app.updater.restart_requested);
        assert!(app.updater.ready && app.updater.open);
    }

    #[test]
    fn a_new_tool_handoff_reopens_restart_confirmation_while_exit_is_pending() {
        let ctx = egui::Context::default();
        let mut app = crate::workspace_interaction_tests::workspace();
        app.tabs.clear();
        app.external_tools_opened = true;
        app.updater.tools_confirmed = true;
        app.updater.ready = true;
        app.updater.restart_requested = true;
        app.pending_exit = true;
        app.sender.send(crate::Event::ToolFinished(Ok(()))).unwrap();
        app.receive(&ctx);
        assert!(app.updater.open && app.updater.ready);
        assert!(app.updater.needs_tool_confirmation(true));
        assert!(!app.updater.restart_requested && !app.updater.exit_ready());
        assert!(!app.pending_exit && !app.closing && !app.exit_ready);
    }

    #[test]
    fn interrupted_restart_preflight_restores_the_verified_update_and_workspace() {
        let ctx = egui::Context::default();
        let mut app = crate::workspace_interaction_tests::workspace();
        app.tabs.clear();
        app.closing = true;
        app.updater.last = Some(Instant::now());
        app.updater.offer = Some(published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        ));
        app.updater.ready = true;
        app.updater.restart_requested = true;
        assert!(app.updater.stop_and_drained());
        let (started, helper) = mpsc::channel();
        app.updater.start_apply(
            &ctx,
            || -> Result<(), ApplyError> { panic!("fixture restart preflight unwind") },
            move |()| {
                started.send(()).unwrap();
                Ok(tempfile::tempfile().unwrap())
            },
        );

        let deadline = Instant::now() + Duration::from_secs(3);
        while app.updater.busy() {
            app.receive(&ctx);
            assert!(Instant::now() < deadline, "restart preflight did not settle");
            std::thread::sleep(Duration::from_millis(2));
        }
        app.receive(&ctx);

        assert!(matches!(
            helper.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        assert_eq!(app.updater.status, text("RustUpdateWorkerFailed"));
        assert!(app.updater.open && app.updater.ready && app.updater.offer.is_some());
        assert!(!app.updater.workers.stopping() && !app.updater.restart_requested);
        assert!(!app.closing && !app.pending_exit && !app.exit_ready && !app.updater.exit_ready());
    }

    #[test]
    fn updater_handoff_waits_for_accepted_work_before_requesting_native_close() {
        use shovel_core::scheduler::Lane;
        let ctx = egui::Context::default();
        let mut app = crate::workspace_interaction_tests::workspace();
        app.closing = true;
        app.updater.restart_requested = true;
        let (request, _) = app.scheduler.reserve(
            "workspace fixture".into(),
            "workspace fixture".into(),
            "accepted Git command".into(),
            Lane::Exclusive,
            |_| {},
        );
        app.updater
            .tx
            .send(Event::Applied(Ok(tempfile::tempfile().unwrap())))
            .unwrap();
        app.receive(&ctx);
        assert!(app.updater.exit_ready());
        assert!(!app.exit_ready);
        assert!(app.scheduler.remove_waiting(request));
        app.receive(&ctx);
        assert!(app.exit_ready);
    }

    #[test]
    fn failed_profile_save_returns_to_the_ready_update_without_retrying_or_exiting() {
        use crate::workspace_interaction_tests::{frame, sidebar_click, workspace};
        use shovel_core::settings::SettingsStore;

        let directory = tempfile::tempdir().unwrap();
        let mut store = SettingsStore::new(directory.path().into());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        app.tabs.clear();
        app.settings = store.load().unwrap();
        store.save(&app.settings).unwrap();
        app.store = Some(store);
        app.updater.offer = Some(published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        ));
        app.updater.ready = true;
        app.updater.open = true;
        let foreign = b"competing profile writer";
        let profile = directory.path().join("settings.json");
        std::fs::write(&profile, foreign).unwrap();

        sidebar_click(
            &ctx,
            &mut app,
            &text("RustUpdateRestart"),
            egui::PointerButton::Primary,
        );
        assert!(app.saving);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.saving {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
            app.receive(&ctx);
            frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        }
        for _ in 0..3 {
            app.receive(&ctx);
            frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        }
        assert!(!app.saving && !app.closing && !app.exit_ready);
        assert!(!app.confirm_exit_without_save && !app.updater.restart_requested);
        assert!(app.updater.open && app.updater.ready && app.updater.offer.is_some());
        assert_eq!(app.updater.status, text("RustUpdateSaveBlocked"));
        assert_eq!(std::fs::read(profile).unwrap(), foreign);
    }

    #[test]
    fn damaged_or_missing_package_returns_to_download_but_a_blocked_restart_stays_ready() {
        let directory = tempfile::tempdir().unwrap();
        let feed = include_bytes!("../../core/tests/fixtures/beta-0.50.14.json");
        let sig = include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig");
        let release = updates::verify_feed(Channel::Beta, feed, sig).unwrap();
        let asset = serde_json::from_slice::<VelopackAssetFeed>(feed)
            .unwrap()
            .Assets
            .remove(0);
        let path = directory.path().join(release.file_name());
        let mut controller = Controller {
            offer: Some(Offer { release, asset }),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        for damaged in [false, true] {
            controller.ready = true;
            controller.restart_requested = true;
            if damaged {
                std::fs::write(&path, b"incomplete").unwrap();
            }
            let result = verify_for_restart(&path, &controller.offer.as_ref().unwrap().release);
            assert!(matches!(result, Err(ApplyError::DownloadAgain)));
            controller.tx.send(Event::Applied(result)).unwrap();
            controller.tick(&ctx, Channel::Beta, false);
            assert!(controller.open && controller.offer.is_some());
            assert!(!controller.ready && !controller.restart_requested && !controller.exit_ready());
            assert_eq!(controller.status, text("RustUpdateDownloadAgain"));
        }
        assert_eq!(std::fs::read(path).unwrap(), b"incomplete");
        controller.ready = true;
        controller
            .tx
            .send(Event::Applied(Err(ApplyError::Retry(text(
                "RustUpdateInstances",
            )))))
            .unwrap();
        controller.tick(&ctx, Channel::Beta, false);
        assert!(controller.ready && !controller.exit_ready());
        assert_eq!(controller.status, text("RustUpdateInstances"));
    }

    #[test]
    fn package_open_for_writing_blocks_restart_without_marking_it_damaged() {
        let directory = tempfile::tempdir().unwrap();
        let offer = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        );
        let path = directory.path().join(offer.release.file_name());
        let writer = File::create(&path).unwrap();
        assert!(matches!(
            verify_for_restart(&path, &offer.release),
            Err(ApplyError::Retry(message)) if message == text("RustUpdateCacheUnavailable")
        ));
        drop(writer);
        assert!(matches!(
            verify_for_restart(&path, &offer.release),
            Err(ApplyError::DownloadAgain)
        ));
    }

    #[test]
    fn long_release_notes_keep_the_download_action_visible_and_empty_notes_take_no_space() {
        let ctx = egui::Context::default();
        let notes = "- Local Changes keeps the previous file selection after refresh.\n".repeat(100);
        for value in [notes.as_str(), ""] {
            let mut action = egui::Rect::NOTHING;
            let mut height = 0.;
            let out = ctx.run_ui(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600., 380.))),
                ..Default::default()
            }, |ui| {
                let before = ui.cursor().top();
                release_notes(ui, value);
                height = ui.cursor().top() - before;
                action = ui.button(text("RustUpdateDownload")).rect;
            });
            out.drop_without_applying_deltas();
            assert!(action.bottom() < 380.);
            if value.is_empty() {
                assert_eq!(height, 0.);
            } else {
                assert!((150. ..200.).contains(&height), "{height}");
            }
        }
    }

    #[test]
    fn bounded_transfer_reports_each_percentage_once_and_renders_download_progress() {
        struct ShortReads(usize);
        impl Read for ShortReads {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(self.0).min(2);
                buffer[..count].fill(7);
                self.0 -= count;
                Ok(count)
            }
        }
        let (tx, rx) = mpsc::channel();
        let mut output = vec![];
        copy_response(
            ShortReads(1000),
            &mut output,
            ResponseSize::Exact(1000),
            &Cancellation::default(),
            Some(&tx),
        )
        .unwrap();
        assert_eq!(output, vec![7; 1000]);
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            (0..=100).collect::<Vec<_>>()
        );
        assert!(
            copy_response(
                ShortReads(1002),
                &mut Vec::new(),
                ResponseSize::Exact(1000),
                &Cancellation::default(),
                None
            )
            .is_err()
        );

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut controller = Controller {
            open: true,
            phase: Phase::Downloading(0),
            ..Default::default()
        };
        controller.tx.send(Event::Progress(0, 47)).unwrap();
        controller.tick(&ctx, Channel::Beta, false);
        let expected = format("RustUpdateProgress", &["47"]);
        assert_eq!(controller.status, expected);
        let mut visible = false;
        for _ in 0..3 {
            let frame = ctx.run_ui(egui::RawInput::default(), |_| {
                controller.show(&ctx, false, false)
            });
            if let Some(update) = &frame.platform_output.accesskit_update {
                visible |= update.nodes.iter().any(|(_, node)| {
                    node.role() == egui::Role::ProgressIndicator
                        && node.label() == Some(expected.as_str())
                });
            }
            frame.drop_without_applying_deltas();
        }
        assert!(visible);
    }

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
                ResponseSize::AtMost(100),
                CHECK_HTTP_TIMEOUT,
                &cancel,
                None
            ),
            Err("ActionResultCancelled")
        );
        assert!(output.is_empty());
    }

    #[test]
    fn interrupted_loopback_response_can_be_retried_with_a_complete_signed_feed() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        let feed = include_bytes!("../../core/tests/fixtures/beta-0.50.14.json");
        let signature = include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig");
        let fetch = |headers: String, bytes: &'static [u8], size, out: &mut Vec<u8>| {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(4)))
                    .unwrap();
                let mut request = BufReader::new(&stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    if request.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                }
                write!(stream, "{headers}\r\nConnection: close\r\n\r\n").unwrap();
                stream.write_all(bytes).unwrap();
            });
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .proxy(None)
                .timeout_global(Some(Duration::from_secs(4)))
                .build()
                .into();
            let response = agent
                .get(format!("http://{address}/fixture"))
                .call()
                .unwrap();
            let result = receive_response(response, out, size, &Cancellation::default(), None);
            server.join().unwrap();
            result
        };
        let size = ResponseSize::Exact(feed.len() as u64);
        let headers = format!("HTTP/1.1 200 OK\r\nContent-Length: {}", feed.len());
        let mut partial = Vec::new();
        assert!(fetch(headers.clone(), &feed[..64], size, &mut partial).is_err());
        assert!(updates::verify_feed(Channel::Beta, &partial, signature).is_err());

        let mut complete = Vec::new();
        fetch(headers, feed, size, &mut complete).unwrap();
        assert_eq!(
            updates::verify_feed(Channel::Beta, &complete, signature)
                .unwrap()
                .version(),
            &semver::Version::new(0, 50, 14)
        );

        for (headers, bytes, size, expected) in [
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 4",
                &b"data"[..],
                ResponseSize::AtMost(3),
                "RustUpdateInvalid",
            ),
            (
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-3/8",
                &b"data"[..],
                ResponseSize::Exact(4),
                "RustUpdateInvalid",
            ),
            (
                "HTTP/1.1 200 OK\r\nContent-Range: bytes 0-3/8",
                &b"data"[..],
                ResponseSize::AtMost(4),
                "RustUpdateInvalid",
            ),
            (
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked",
                &b"2\r\nab\r\n0\r\n\r\n"[..],
                ResponseSize::Exact(4),
                "RustUpdateIncomplete",
            ),
        ] {
            let mut out = Vec::new();
            assert_eq!(fetch(headers.into(), bytes, size, &mut out), Err(expected));
            if expected == "RustUpdateInvalid" {
                assert!(out.is_empty());
            }
            assert_eq!(
                download_error(velopack::Error::Other(expected.into())),
                text(expected)
            );
        }
    }

    #[test]
    fn cancellation_during_a_read_discards_that_chunk_and_plain_http_is_rejected() {
        let cancel = Cancellation::default();
        struct CancelRead(Cancellation);
        impl Read for CancelRead {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.0.cancel();
                buffer[0] = 7;
                Ok(1)
            }
        }
        let mut out = Vec::new();
        let (tx, rx) = mpsc::channel();
        assert_eq!(
            copy_response(
                CancelRead(cancel.clone()),
                &mut out,
                ResponseSize::Exact(1),
                &cancel,
                Some(&tx)
            ),
            Err("ActionResultCancelled")
        );
        assert!(out.is_empty() && rx.try_recv().is_err());

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        assert!(
            transfer(
                &format!("http://{address}/"),
                &mut out,
                ResponseSize::AtMost(32),
                Duration::from_millis(300),
                &Cancellation::default(),
                None
            )
            .is_err()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
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
    fn interrupted_discovery_releases_the_update_worker_and_allows_retry() {
        let ctx = egui::Context::default();
        let offer = published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        );
        let mut controller = Controller {
            offer: Some(offer),
            ready: true,
            ..Default::default()
        };
        let start = Instant::now();
        let settle = |controller: &mut Controller| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while controller.busy() {
                controller.tick_at(&ctx, Channel::Beta, false, start, |_, _| {
                    unreachable!("manual checks must not trigger automatic retries")
                });
                assert!(Instant::now() < deadline, "update worker did not settle");
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(controller.workers.idle());
        };

        controller.begin_check(&ctx, start, |_, _| panic!("fixture discovery unwind"));
        settle(&mut controller);
        assert_eq!(controller.status, text("RustUpdateWorkerFailed"));
        assert!(controller.ready && controller.offer.is_some());

        controller.begin_check(&ctx, start, |_, _| Ok(None));
        settle(&mut controller);
        assert_eq!(controller.status, text("RustUpdateCurrent"));
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
        assert!(!controller.busy());
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
        while controller.busy() {
            controller.tick(&ctx, Channel::Stable, false);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(controller.status, text("RustUpdateCurrent"));
    }

    #[test]
    fn later_closes_updates_and_preserves_the_verified_offer() {
        use crate::workspace_interaction_tests::{frame, sidebar_click, workspace};

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        app.tabs.clear();
        app.updater.open = true;
        app.updater.ready = true;
        app.updater.offer = Some(published_offer(
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
            include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
        ));
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        sidebar_click(
            &ctx,
            &mut app,
            &text("RustUpdateLater"),
            egui::PointerButton::Primary,
        );

        assert!(!app.updater.open && app.updater.ready);
        assert_eq!(
            app.updater.available_version(Channel::Beta),
            Some(&semver::Version::new(0, 50, 14)),
        );
        assert!(!app.updater.restart_requested && !app.updater.exit_ready());
        assert!(!app.closing && !app.exit_ready);
    }

    #[test]
    fn opening_updates_refreshes_once_and_keeps_restart_ready_if_offline() {
        let ctx = egui::Context::default();
        let now = Instant::now();
        let mut controller = Controller {
            ready: true,
            offer: Some(published_offer(
                include_bytes!("../../core/tests/fixtures/beta-0.50.14.json"),
                include_bytes!("../../core/tests/fixtures/beta-0.50.14.json.sig"),
            )),
            ..Default::default()
        };
        let (started, checking) = mpsc::channel();
        let (release, held) = mpsc::channel();
        controller.open_at(&ctx, now, move |channel, _| {
            started.send(channel).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            Err(CheckError::Unavailable(text("RustUpdateFailed")))
        });
        assert_eq!(
            checking.recv_timeout(Duration::from_secs(5)).unwrap(),
            Channel::Beta,
        );
        assert!(controller.open && controller.ready && controller.busy());
        assert!(controller.phase == Phase::Checking);
        let generation = controller.generation;
        controller.open_at(&ctx, now, |_, _| panic!("overlapping discovery"));
        assert_eq!(controller.generation, generation);
        assert_eq!(controller.last, Some(now));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.busy() {
            controller.tick_at(&ctx, Channel::Beta, false, now, |_, _| {
                panic!("unexpected periodic discovery")
            });
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(controller.status, text("RustUpdateFailed"));
        assert!(controller.open && controller.ready && controller.offer.is_some());
        assert!(!controller.restart_requested && !controller.exit_ready());
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
        assert!(controller.busy() && !controller.open);
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
        while controller.busy() {
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
