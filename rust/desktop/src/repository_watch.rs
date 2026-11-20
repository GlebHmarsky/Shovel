//! Windows directory notifications are hints. Git and repository identity remain
//! authoritative when a view consumes them.
use eframe::egui;
use shovel_core::{case::path_key, identity};
use std::{
    cell::UnsafeCell,
    ffi::OsString,
    io,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_IO_INCOMPLETE, ERROR_NOTIFY_ENUM_DIR, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
    Storage::FileSystem::{
        CreateFileW, FILE_ACTION_MODIFIED, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
        FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME,
        FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
    },
};

const DEBOUNCE: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Default)]
pub(super) struct Changes(u8);

impl Changes {
    const HISTORY: Self = Self(1);
    const WORKTREE: Self = Self(2);
    const BOTH: Self = Self(3);

    pub(super) fn history(self) -> bool {
        self.0 & Self::HISTORY.0 != 0
    }

    pub(super) fn worktree(self) -> bool {
        self.0 & Self::WORKTREE.0 != 0
    }
}

pub(super) struct Watch {
    stop: Arc<OwnedHandle>,
    pending: Arc<AtomicU8>,
    available: Arc<AtomicBool>,
    identity: Arc<OnceLock<String>>,
    due: Option<Instant>,
}

impl Watch {
    pub(super) fn start(path: PathBuf, ctx: &egui::Context) -> Option<(Self, mpsc::Receiver<()>)> {
        let stop = Arc::new(event().ok()?);
        let pending = Arc::new(AtomicU8::new(0));
        let available = Arc::new(AtomicBool::new(false));
        let identity = Arc::new(OnceLock::new());
        let (started, ready) = mpsc::channel();
        let worker_stop = stop.clone();
        let worker_pending = pending.clone();
        let worker_available = available.clone();
        let worker_identity = identity.clone();
        let ctx = ctx.clone();
        thread::Builder::new()
            .name("repository notifications".into())
            .spawn(move || {
                let publish = |changes: Changes| {
                    if changes.0 != 0 && worker_pending.fetch_or(changes.0, Ordering::AcqRel) == 0 {
                        ctx.request_repaint();
                    }
                };
                let opened = identity::read(&path).and_then(|identity| {
                    let roots = Roots::new(&identity);
                    roots
                        .open(&identity)
                        .map(|directories| (roots, directories, identity.fingerprint))
                });
                let Ok((roots, directories, fingerprint)) = opened else {
                    return;
                };
                let _ = worker_identity.set(fingerprint);
                worker_available.store(true, Ordering::Release);
                let _ = started.send(());
                if observe(&roots, directories, &worker_stop, publish).is_err() {
                    worker_available.store(false, Ordering::Release);
                    publish(Changes::BOTH);
                }
            })
            .ok()?;
        Some((
            Self {
                stop,
                pending,
                available,
                identity,
                due: None,
            },
            ready,
        ))
    }

    pub(super) fn available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    pub(super) fn covers(&self, fingerprint: &str) -> bool {
        self.identity
            .get()
            .is_none_or(|identity| identity == fingerprint)
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context) -> Changes {
        if self.pending.load(Ordering::Acquire) == 0 {
            return Changes::default();
        }
        let now = Instant::now();
        let due = *self.due.get_or_insert(now + DEBOUNCE);
        if now < due {
            ctx.request_repaint_after(due - now);
            return Changes::default();
        }
        self.due = None;
        Changes(self.pending.swap(0, Ordering::AcqRel))
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // The observer drains its kernel requests on its own thread.
        unsafe { SetEvent(self.stop.as_raw_handle()) };
    }
}

impl super::Tab {
    pub(super) fn start_watching(&mut self, ctx: &egui::Context) -> Option<mpsc::Receiver<()>> {
        if self.view.watch.is_some() {
            return None;
        }
        let (watch, ready) = Watch::start(self.path.clone(), ctx)?;
        self.view.watch = Some(watch);
        Some(ready)
    }
}

impl super::Workspace {
    pub(super) fn poll_repository_changes(&mut self, ctx: &egui::Context) {
        let active = self.tabs.get(self.selected).map(|tab| tab.id);
        for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
            let changes = tab
                .view
                .watch
                .as_mut()
                .map_or_else(Changes::default, |watch| watch.poll(ctx));
            let mut fallback = false;
            if Some(tab.id) == active
                && !tab.loading
                && !tab.view.identity_blocked
                && tab
                    .view
                    .watch
                    .as_ref()
                    .is_none_or(|watch| !watch.available())
                && let Some(validated) = tab.view.validated
            {
                let delay = Duration::from_secs(30).saturating_sub(validated.elapsed());
                fallback = delay.is_zero();
                if !fallback {
                    ctx.request_repaint_after(delay);
                }
            }
            if changes.history() || fallback {
                tab.discard_initial_history();
                tab.view.stale = true;
                tab.view.external_history_change = true;
                tab.refresh_again = true;
                tab.divergence.mark_stale();
                tab.remotes.invalidate();
                tab.sidebar.invalidate();
                for view in &mut self.stash_views {
                    if tab.matches_path(&view.path) {
                        view.changed();
                    }
                }
            }
            if changes.worktree() || fallback {
                self.tab_markers.invalidate(tab.id);
                for editor in &mut self.editors {
                    if tab.owns_editor(editor) {
                        editor.external_changed();
                    }
                }
            }
        }
    }
}

struct Roots {
    worktree: String,
    git: String,
    common: String,
    marker: String,
}

impl Roots {
    fn new(identity: &identity::Identity) -> Self {
        Self {
            worktree: path_key(&identity.root),
            git: path_key(&identity.git),
            common: path_key(&identity.common),
            marker: path_key(&identity.root.join(".git")),
        }
    }

    fn open(&self, identity: &identity::Identity) -> io::Result<Vec<DirectoryWatch>> {
        let mut watches = vec![DirectoryWatch::open(&identity.root, true)?];
        if relative(&self.git, &self.worktree).is_none()
            && relative(&self.git, &self.common).is_none()
            && self.git != self.common
        {
            watches.push(DirectoryWatch::open(&identity.git, true)?);
        }
        if relative(&self.common, &self.worktree).is_none() && self.common != self.worktree {
            watches.push(DirectoryWatch::open(&identity.common, true)?);
        }
        // A directory handle alone does not see its own rename/removal.
        if let Some(parent) = identity.root.parent()
            && let Ok(watch) = DirectoryWatch::open(parent, false)
        {
            watches.push(watch);
        }
        Ok(watches)
    }

    fn classify(&self, absolute: &Path, action: u32) -> Changes {
        let path = path_key(absolute);
        if action == FILE_ACTION_MODIFIED
            && (path == self.worktree || path == self.git || path == self.common)
        {
            return Changes::default();
        }
        if path == self.worktree || path == self.marker || path == self.git || path == self.common {
            return Changes::BOTH;
        }
        if let Some(name) = relative(&path, &self.git).or_else(|| relative(&path, &self.common)) {
            return metadata_change(name);
        }
        match relative(&path, &self.worktree) {
            Some(".GITMODULES") => Changes::BOTH,
            Some(_) => Changes::WORKTREE,
            None => Changes::default(),
        }
    }
}

fn relative<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    path.strip_prefix(directory.trim_end_matches('\\'))?
        .strip_prefix('\\')
}

fn metadata_change(name: &str) -> Changes {
    if name.ends_with(".LOCK") {
        return Changes::default();
    }
    let top = name.split('\\').next().unwrap_or_default();
    match top {
        "HEAD" | "PACKED-REFS" | "REFTABLE" | "CONFIG" | "CONFIG.WORKTREE" | "GITDIR"
        | "COMMONDIR" | "MERGE_HEAD" | "MERGE_MSG" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
        | "BISECT_START" | "SEQUENCER" | "REBASE-MERGE" | "REBASE-APPLY" => Changes::BOTH,
        "REFS" if name == "REFS" || name == "REFS\\HEADS" || name.starts_with("REFS\\HEADS\\") => {
            Changes::BOTH
        }
        "REFS" | "SHALLOW" => Changes::HISTORY,
        "INDEX" => Changes::WORKTREE,
        "INFO" if name == "INFO\\GRAFTS" => Changes::HISTORY,
        "INFO"
            if matches!(
                name,
                "INFO\\ATTRIBUTES" | "INFO\\EXCLUDE" | "INFO\\SPARSE-CHECKOUT"
            ) =>
        {
            Changes::WORKTREE
        }
        "LOGS" if name.starts_with("LOGS\\REFS\\STASH") => Changes::HISTORY,
        _ => Changes::default(),
    }
}

fn event() -> io::Result<OwnedHandle> {
    let handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

struct Request {
    overlapped: UnsafeCell<OVERLAPPED>,
    // DWORD alignment is required by ReadDirectoryChangesW.
    buffer: UnsafeCell<[u32; 8192]>,
}

struct DirectoryWatch {
    path: PathBuf,
    recursive: bool,
    directory: OwnedHandle,
    ready: OwnedHandle,
    request: Box<Request>,
    pending: bool,
}

impl DirectoryWatch {
    fn open(path: &Path, recursive: bool) -> io::Result<Self> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let directory = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut watch = Self {
            path: path.to_owned(),
            recursive,
            directory,
            ready: event()?,
            request: Box::new(Request {
                overlapped: UnsafeCell::new(OVERLAPPED::default()),
                buffer: UnsafeCell::new([0; 8192]),
            }),
            pending: false,
        };
        watch.arm()?;
        Ok(watch)
    }

    fn arm(&mut self) -> io::Result<()> {
        debug_assert!(!self.pending);
        let request = &self.request;
        // The heap allocation stays put and these cells are inaccessible until
        // the kernel completes the request, including after cancellation.
        let result = unsafe {
            request.overlapped.get().write(OVERLAPPED {
                hEvent: self.ready.as_raw_handle(),
                ..Default::default()
            });
            ReadDirectoryChangesW(
                self.directory.as_raw_handle(),
                request.buffer.get().cast(),
                size_of::<[u32; 8192]>() as u32,
                i32::from(self.recursive),
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | if self.recursive {
                        FILE_NOTIFY_CHANGE_LAST_WRITE
                            | FILE_NOTIFY_CHANGE_SIZE
                            | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    } else {
                        0
                    },
                ptr::null_mut(),
                request.overlapped.get(),
                None,
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        self.pending = true;
        Ok(())
    }

    fn receive(&mut self, roots: &Roots) -> io::Result<Changes> {
        let mut written = 0;
        let result = unsafe {
            GetOverlappedResult(
                self.directory.as_raw_handle(),
                self.request.overlapped.get(),
                &mut written,
                0,
            )
        };
        if result == 0 {
            let error = io::Error::last_os_error();
            self.pending = error.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32);
            return if error.raw_os_error() == Some(ERROR_NOTIFY_ENUM_DIR as i32) {
                Ok(Changes::BOTH)
            } else {
                Err(error)
            };
        }
        self.pending = false;
        let bytes = written as usize;
        if bytes == 0 || bytes > size_of::<[u32; 8192]>() {
            return Ok(Changes::BOTH);
        }
        // The request has completed. Only the reported extent is initialized by Windows.
        let buffer =
            unsafe { std::slice::from_raw_parts(self.request.buffer.get().cast::<u8>(), bytes) };
        Ok(classify_buffer(buffer, |name, action| {
            roots.classify(&self.path.join(name), action)
        }))
    }
}

impl Drop for DirectoryWatch {
    fn drop(&mut self) {
        if self.pending {
            // A completion may have raced cancellation. Wait for either outcome
            // before releasing the buffer, OVERLAPPED and event it refers to.
            unsafe {
                let handle = self.directory.as_raw_handle();
                let request = self.request.overlapped.get();
                CancelIoEx(handle, request);
                GetOverlappedResult(handle, request, &mut 0, 1);
            }
        }
    }
}

fn classify_buffer(mut bytes: &[u8], classify: impl Fn(&Path, u32) -> Changes) -> Changes {
    let mut changes = Changes::default();
    loop {
        let Some(header) = bytes.get(..12) else {
            return Changes::BOTH;
        };
        let next = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let action = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let Some(name) = bytes[12..]
            .get(..length)
            .filter(|_| length != 0 && length.is_multiple_of(2))
        else {
            return Changes::BOTH;
        };
        let (units, _) = name.as_chunks::<2>();
        let units: Vec<u16> = units.iter().copied().map(u16::from_le_bytes).collect();
        changes.0 |= classify(Path::new(&OsString::from_wide(&units)), action).0;
        if next == 0 {
            return changes;
        }
        if next < 12 + length || next >= bytes.len() {
            return Changes::BOTH;
        }
        bytes = &bytes[next..];
    }
}

fn observe(
    roots: &Roots,
    mut watches: Vec<DirectoryWatch>,
    stop: &OwnedHandle,
    publish: impl Fn(Changes),
) -> io::Result<()> {
    let handles: Vec<_> = std::iter::once(stop.as_raw_handle())
        .chain(watches.iter().map(|watch| watch.ready.as_raw_handle()))
        .collect();
    loop {
        let ready =
            unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE) };
        if ready == WAIT_OBJECT_0 {
            return Ok(());
        }
        let index = ready.checked_sub(WAIT_OBJECT_0 + 1).unwrap_or(u32::MAX);
        let watch = watches
            .get_mut(index as usize)
            .ok_or_else(io::Error::last_os_error)?;
        let changes = watch.receive(roots)?;
        watch.arm()?;
        publish(changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_view::tests::{Fixture, wait};

    fn until(
        app: &mut crate::Workspace,
        ctx: &egui::Context,
        accept: impl Fn(&crate::Workspace) -> bool,
    ) {
        let start = Instant::now();
        loop {
            app.receive(ctx);
            if accept(app) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(12),
                "notification did not refresh its owner"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn external_files_and_refs_refresh_their_views_without_reloading_inactive_tabs() {
        let original = Fixture::new();
        let other = Fixture::new();
        let ctx = egui::Context::default();
        let mut app = original.app(&ctx);
        let id = app.tabs[0].id;
        app.set_tab_marker(id, Some(crate::tab_markers::Color::Blue), &ctx);
        until(&mut app, &ctx, |app| {
            app.tab_markers.status(&app.tabs[0]) == crate::tab_markers::Status::Clean
        });
        assert!(app.tabs[0].view.watch.as_ref().unwrap().available());
        let generation = app.tabs[0].view.generation;
        let started = Instant::now();
        for index in 0..8 {
            std::fs::write(
                original.root.join(format!("external-{index}.txt")),
                "working changes",
            )
            .unwrap();
        }
        until(&mut app, &ctx, |app| {
            app.tab_markers.status(&app.tabs[0]) == crate::tab_markers::Status::Changed
        });
        println!(
            "external worktree burst to dirty marker: {:?}",
            started.elapsed()
        );
        assert_eq!(app.tabs[0].view.generation, generation);
        assert!(!app.tabs[0].loading);

        let tip = app.tabs[0].page.as_ref().unwrap().entries[1]
            .commit
            .hash
            .clone();
        original.command(&["update-ref", "refs/heads/main", &tip], &[]);
        let started = Instant::now();
        until(&mut app, &ctx, |app| {
            !app.tabs[0].loading
                && app.tabs[0].history.as_ref().unwrap().snapshot.head.as_ref() == Some(&tip)
        });
        println!(
            "external branch update to fresh selected history: {:?}",
            started.elapsed()
        );

        app.open(other.root.clone(), &ctx);
        wait(&mut app, &ctx);
        let visible_generation = app.tabs[1].view.generation;
        let prior_entry = app.tabs[0].page.as_ref().unwrap().entries[0]
            .commit
            .hash
            .clone();
        let next_tip = app.tabs[0].page.as_ref().unwrap().entries[1]
            .commit
            .hash
            .clone();
        original.command(&["update-ref", "refs/heads/main", &next_tip], &[]);
        until(&mut app, &ctx, |app| app.tabs[0].view.stale);
        assert!(!app.tabs[0].loading);
        assert_eq!(app.tabs[1].view.generation, visible_generation);

        let started = Instant::now();
        app.select_tab(0, &ctx);
        assert_eq!(
            app.tabs[0].page.as_ref().unwrap().entries[0].commit.hash,
            prior_entry
        );
        println!(
            "return to dirty tab with cached history: {:?}",
            started.elapsed()
        );
        until(&mut app, &ctx, |app| {
            !app.tabs[0].loading
                && app.tabs[0].history.as_ref().unwrap().snapshot.head.as_ref() == Some(&next_tip)
        });
        println!(
            "return to dirty tab with fresh history: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn shared_git_metadata_refreshes_a_linked_worktree() {
        let fixture = Fixture::new();
        let ctx = egui::Context::default();
        let linked = fixture._temp.path().join("linked");
        fixture.command(
            &["worktree", "add", "--detach", linked.to_str().unwrap()],
            &[],
        );
        let mut app = fixture.app(&ctx);
        app.open(linked, &ctx);
        wait(&mut app, &ctx);
        assert!(app.tabs[1].view.watch.as_ref().unwrap().available());
        fixture.command(&["tag", "external-tag"], &[]);
        until(&mut app, &ctx, |app| {
            !app.tabs[1].loading
                && app.tabs[1]
                    .history
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .references
                    .iter()
                    .any(|reference| reference.name == "refs/tags/external-tag")
        });
        assert!(app.tabs[0].view.stale);
        assert!(!app.tabs[0].loading);
    }

    #[test]
    fn external_edits_refresh_local_changes_and_closed_tabs_observe_repository_identity() {
        let fixture = Fixture::new();
        let ctx = egui::Context::default();
        let mut app = fixture.app(&ctx);
        let id = app.tabs[0].id;
        app.settings
            .set("CommitInWorkspace", serde_json::json!(true));
        app.draft_store = Some(shovel_core::drafts::Store {
            directory: fixture._temp.path().join("drafts"),
            legacy_directory: None,
        });
        let started = Instant::now();
        app.open_commit();
        crate::history_view::tests::paint(&mut app, &ctx, vec![]);
        println!(
            "inline Commit open to first layout: {:?}",
            started.elapsed()
        );
        until(&mut app, &ctx, |app| {
            app.editors[0].current_context().is_some()
        });
        println!(
            "inline Commit open to first fresh status: {:?}",
            started.elapsed()
        );
        let generation = app.tabs[0].view.generation;
        let started = Instant::now();
        std::fs::write(fixture.root.join("outside-shovel.txt"), "external edit").unwrap();
        until(&mut app, &ctx, |app| {
            app.editors[0].current_context().is_some_and(|context| {
                context
                    .change(shovel_core::working::Group::Untracked, "outside-shovel.txt")
                    .is_some()
            })
        });
        println!(
            "external edit to fresh inline Local Changes: {:?}",
            started.elapsed()
        );
        assert_eq!(app.tabs[0].view.generation, generation);
        app.close_tab(id, &ctx);
        std::fs::rename(fixture.root.join(".git"), fixture.root.join("old-metadata")).unwrap();
        until(&mut app, &ctx, |app| app.retained_tabs[0].view.stale);
        assert!(!app.retained_tabs[0].loading);
        app.open(fixture.root.clone(), &ctx);
        until(&mut app, &ctx, |app| app.tabs[0].view.identity_blocked);
        assert!(app.tabs[0].page.is_none());
    }
}
