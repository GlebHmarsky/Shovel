//! Opt-in worktree markers. The tab strip only consumes published status.
use super::*;
use egui::Color32;
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

const KEY: &str = "RepositoryTabMarkers";
const INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    const ALL: [Self; 6] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
    ];

    fn properties(self) -> (&'static str, &'static str, Color32) {
        match self {
            Self::Red => ("red", "TabMarkerRed", Color32::from_rgb(245, 68, 62)),
            Self::Orange => ("orange", "TabMarkerOrange", Color32::from_rgb(255, 150, 30)),
            Self::Yellow => ("yellow", "TabMarkerYellow", Color32::from_rgb(255, 202, 36)),
            Self::Green => ("green", "TabMarkerGreen", Color32::from_rgb(110, 207, 56)),
            Self::Blue => ("blue", "TabMarkerBlue", Color32::from_rgb(31, 174, 236)),
            Self::Purple => (
                "purple",
                "TabMarkerPurple",
                Color32::from_rgb(191, 115, 222),
            ),
        }
    }
}

pub(super) fn color(settings: &Settings, path: &Path) -> Option<Color> {
    let key = shovel_core::case::path_key(path);
    let saved = settings.get(KEY)?.get(key)?.as_str()?;
    Color::ALL
        .into_iter()
        .find(|color| color.properties().0 == saved)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Status {
    #[default]
    Checking,
    Clean,
    Changed,
    Unavailable,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Checking => "TabMarkerChecking",
            Self::Clean => "TabMarkerClean",
            Self::Changed => "TabMarkerChanged",
            Self::Unavailable => "TabMarkerUnavailable",
        }
    }
}

pub(super) fn marker<'a>(color: Color, status: Status) -> egui::Atom<'a> {
    let (_, _, color) = color.properties();
    egui::Atom::paint((14., 14.), move |ui, args| {
        let center = args.rect.center();
        let painter = ui.painter();
        if status == Status::Changed {
            painter.circle_filled(center, 4., color);
        } else {
            painter.circle_stroke(center, 3.5, egui::Stroke::new(1.3, color));
            if matches!(status, Status::Checking | Status::Unavailable) {
                painter.hline(
                    center.x - 1.5..=center.x + 1.5,
                    center.y,
                    egui::Stroke::new(1., color),
                );
            }
        }
    })
}

pub(super) fn picker(ui: &mut egui::Ui, current: Option<Color>) -> Option<Option<Color>> {
    ui.separator();
    let mut choice = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.;
        for color in Color::ALL {
            let label = strings::text(color.properties().1);
            let response = ui.add_sized(
                [20., 20.],
                egui::Button::new(marker(color, Status::Changed))
                    .selected(current == Some(color))
                    .frame_when_inactive(false),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label)
            });
            if response.on_hover_text(label).clicked() {
                choice = Some(Some(color));
            }
        }
        let label = strings::text("TabMarkerClear");
        let clear = egui::Atom::paint((14., 14.), |ui, args| {
            let c = args.rect.center();
            let stroke = egui::Stroke::new(1., workspace_style::Palette::current(ui).muted);
            ui.painter().circle_stroke(c, 5., stroke);
            for dy in [-2., 2.] {
                ui.painter()
                    .line_segment([c + egui::vec2(-2., dy), c + egui::vec2(2., -dy)], stroke);
            }
        });
        let response = ui.add_enabled(
            current.is_some(),
            egui::Button::new(clear)
                .min_size(egui::vec2(20., 20.))
                .frame_when_inactive(false),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), &label)
        });
        if response.on_hover_text(label).clicked() {
            choice = Some(None);
        }
    });
    choice
}

struct Observation {
    read: Read,
    due: Option<Instant>,
}

enum Read {
    Marker(Status),
    Count(Option<usize>),
}

struct Scan {
    id: u64,
    count: bool,
    cancel: Cancellation,
    result: Receiver<ScanResult>,
}

struct ScanResult {
    read: Read,
    rows: Option<Arc<[shovel_core::working::Change]>>,
}

#[derive(Default)]
pub(super) struct Controller {
    observations: HashMap<u64, Observation>,
    scan: Option<Scan>,
    preview: Option<(u64, Arc<[shovel_core::working::Change]>)>,
}

impl Controller {
    pub fn preview_rows(&self, tab: &Tab) -> Option<Arc<[shovel_core::working::Change]>> {
        self.preview
            .as_ref()
            .filter(|(id, _)| *id == tab.id && !tab.view.identity_blocked)
            .map(|(_, rows)| rows.clone())
    }
    pub fn status(&self, tab: &Tab) -> Status {
        if tab.view.identity_blocked {
            return Status::Unavailable;
        }
        self.observations
            .get(&tab.id)
            .map_or(Status::Checking, |observation| match observation.read {
                Read::Marker(status) => status,
                Read::Count(Some(0)) => Status::Clean,
                Read::Count(Some(_)) => Status::Changed,
                Read::Count(None) => Status::Unavailable,
            })
    }

    pub fn count(&self, tab: &Tab) -> Option<usize> {
        if tab.view.identity_blocked {
            return None;
        }
        match self.observations.get(&tab.id)?.read {
            Read::Count(count) => count,
            Read::Marker(Status::Clean) => Some(0),
            Read::Marker(_) => None,
        }
    }

    pub fn invalidate(&mut self, id: u64) {
        if let Some(read) = self.observations.get_mut(&id) {
            read.due = Some(Instant::now());
        }
        if let Some(scan) = &self.scan
            && scan.id == id
        {
            scan.cancel.cancel();
        }
    }

    pub fn reset(&mut self) {
        self.observations.clear();
        self.preview = None;
        if let Some(scan) = &self.scan {
            scan.cancel.cancel();
        }
    }

    pub fn forget(&mut self, id: u64) {
        self.observations.remove(&id);
        if self.preview.as_ref().is_some_and(|(owner, _)| *owner == id) {
            self.preview = None;
        }
        self.invalidate(id);
    }
}

impl Workspace {
    pub(super) fn set_tab_marker(&mut self, id: u64, marker: Option<Color>, ctx: &egui::Context) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        let key = shovel_core::case::path_key(&tab.path);
        let mut values = self
            .settings
            .get(KEY)
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(color) = marker {
            values.insert(key, serde_json::json!(color.properties().0));
        } else {
            values.remove(&key);
            self.tab_markers.forget(id);
        }
        self.settings.set(KEY, serde_json::Value::Object(values));
        self.preferences_dirty = true;
        ctx.request_repaint();
    }

    pub(super) fn poll_tab_markers(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let active = self.tabs.get(self.selected).and_then(|tab| {
            (!tab.view.identity_blocked && tab.history.is_some()).then_some(tab.id)
        });
        let marked =
            |tab: &Tab| !tab.view.identity_blocked && color(&self.settings, &tab.path).is_some();
        let markers = &mut self.tab_markers;
        if let Some(scan) = &markers.scan {
            match scan.result.try_recv() {
                Err(mpsc::TryRecvError::Empty) => return,
                result => {
                    if !scan.cancel.is_cancelled()
                        && let Some(tab) = self.tabs.iter().find(|tab| {
                            tab.id == scan.id
                                && !tab.view.identity_blocked
                                && (Some(tab.id) == active || marked(tab))
                        })
                    {
                        let result = result.unwrap_or(ScanResult {
                            read: if scan.count {
                                Read::Count(None)
                            } else {
                                Read::Marker(Status::Unavailable)
                            },
                            rows: None,
                        });
                        if let Some(rows) = result.rows
                            && Some(tab.id) == active
                        {
                            for editor in &mut self.editors {
                                if tab.owns_editor(editor) {
                                    editor.seed_preview(rows.clone());
                                }
                            }
                            markers.preview = Some((tab.id, rows));
                        }
                        markers.observations.insert(
                            scan.id,
                            Observation {
                                read: result.read,
                                due: marked(tab).then_some(now + INTERVAL),
                            },
                        );
                    }
                    markers.scan = None;
                }
            }
        }
        if self.closing
            || self.pending_exit
            || self.confirm_exit_without_save
            || self.git_selection.busy()
            || self.read_workers.stopping()
        {
            return;
        }
        let Some(git) = &self.git else {
            return;
        };
        let selected = self
            .tabs
            .get(self.selected)
            .filter(|tab| Some(tab.id) == active);
        let count = selected.filter(|tab| {
            markers.observations.get(&tab.id).is_none_or(|observation| {
                observation.due.is_some_and(|due| due <= now)
                    || matches!(observation.read, Read::Marker(_))
            })
        });
        let next = self
            .tabs
            .iter()
            .filter(|tab| marked(tab))
            .map(|tab| {
                (
                    tab,
                    markers.observations.get(&tab.id).and_then(|read| read.due),
                )
            })
            .min_by_key(|(_, due)| *due);
        let next = count.map(|tab| (tab, None)).or(next);
        let Some((tab, due)) = next else {
            return;
        };
        if let Some(delay) = due
            .and_then(|due| due.checked_duration_since(now))
            .filter(|d| !d.is_zero())
        {
            ctx.request_repaint_after(delay);
            return;
        }

        let (tx, result) = mpsc::channel();
        let cancel = Cancellation::default();
        let count = Some(tab.id) == active;
        markers.scan = Some(Scan {
            id: tab.id,
            count,
            cancel: cancel.clone(),
            result,
        });
        let expected = tab
            .observed_snapshot()
            .map(|snapshot| snapshot.identity.fingerprint.clone())
            .or_else(|| self.known_identity(&tab.path));
        let path = tab.path.clone();
        let git = git.clone();
        let ctx = ctx.clone();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            let result = match expected.as_deref().filter(|_| count) {
                Some(expected) => {
                    let rows = git.local_change_rows(&path, expected, &cancel).ok();
                    let count = rows.as_ref().map(|rows| {
                        rows.iter()
                            .map(|entry| entry.path.as_str())
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                    });
                    ScanResult {
                        read: Read::Count(count),
                        rows: rows.map(Arc::from),
                    }
                }
                None => ScanResult {
                    read: Read::Marker(
                        match git.has_local_changes(&path, expected.as_deref(), &cancel) {
                            Ok(true) => Status::Changed,
                            Ok(false) => Status::Clean,
                            Err(_) => Status::Unavailable,
                        },
                    ),
                    rows: None,
                },
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history_view::tests::Fixture,
        workspace_interaction_tests::{frame, sidebar_click, sidebar_point},
    };

    #[test]
    fn context_menu_marks_only_the_chosen_tab_and_tracks_native_changes() {
        let fixture = Fixture::new();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = fixture.app(&ctx);
        app.settings
            .set("CommitInWorkspace", serde_json::json!(true));
        let other_path = fixture._temp.path().join("unmarked");
        app.ensure_tab(other_path.clone());
        let id = app.tabs[0].id;
        let marker_color = Color::Blue.properties().2;
        let painted = |app: &mut Workspace| {
            let output = frame(&ctx, app, vec![]);
            let markers: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Circle(circle)
                        if circle.fill == marker_color || circle.stroke.color == marker_color =>
                    {
                        Some(circle.fill == marker_color)
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            markers
        };
        let wait_count = |app: &mut Workspace, expected| {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                app.receive(&ctx);
                if app.tab_markers.count(&app.tabs[0]) == Some(expected)
                    && app.tab_markers.scan.is_none()
                {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "expected {expected} changed paths"
                );
                thread::sleep(Duration::from_millis(5));
            }
        };
        wait_count(&mut app, 0);
        assert!(painted(&mut app).is_empty());
        let partial = fixture.root.join("partial.txt");
        std::fs::write(&partial, "staged").unwrap();
        fixture.command(&["add", "partial.txt"], &[]);
        std::fs::write(&partial, "unstaged").unwrap();
        let untracked_dir = fixture.root.join("untracked");
        std::fs::create_dir(&untracked_dir).unwrap();
        for name in ["one.txt", "two.txt"] {
            std::fs::write(untracked_dir.join(name), name).unwrap();
        }
        wait_count(&mut app, 3);
        assert!(sidebar_point(&ctx, &mut app, "Local Changes (3)").is_some());
        fixture.command(&["reset", "--", "partial.txt"], &[]);
        std::fs::remove_file(partial).unwrap();
        for name in ["one.txt", "two.txt"] {
            std::fs::remove_file(untracked_dir.join(name)).unwrap();
        }
        wait_count(&mut app, 0);
        sidebar_click(&ctx, &mut app, "history", egui::PointerButton::Secondary);
        assert!(sidebar_point(&ctx, &mut app, "Red tab marker").is_some());
        sidebar_click(
            &ctx,
            &mut app,
            "Blue tab marker",
            egui::PointerButton::Primary,
        );
        assert!(color(&app.settings, &fixture.root) == Some(Color::Blue));
        assert!(color(&app.settings, &other_path).is_none());

        let wait_status = |app: &mut Workspace, status| {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                app.receive(&ctx);
                if app.tab_markers.status(&app.tabs[0]) == status && app.tab_markers.scan.is_none()
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "last status: {:?}",
                    app.tab_markers.status(&app.tabs[0])
                );
                thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(app.tab_markers.observations.len(), 1);
        };
        let poll_due = |app: &mut Workspace| {
            app.tab_markers.observations.get_mut(&id).unwrap().due = Some(Instant::now());
        };
        wait_status(&mut app, Status::Clean);
        assert_eq!(painted(&mut app), [false]);
        let file = fixture.root.join("local.txt");
        std::fs::write(&file, "new file").unwrap();
        poll_due(&mut app);
        wait_status(&mut app, Status::Changed);
        assert_eq!(painted(&mut app), [true]);

        fixture.command(&["add", "local.txt"], &[]);
        poll_due(&mut app);
        wait_status(&mut app, Status::Changed);
        fixture.command(&["commit", "-m", "save changes"], &[]);
        poll_due(&mut app);
        wait_status(&mut app, Status::Clean);
        assert_eq!(painted(&mut app), [false]);

        let profile = fixture._temp.path().join("profile");
        let mut store = SettingsStore::new(profile.clone());
        store.load().unwrap();
        store.save(&app.settings).unwrap();
        let restored = SettingsStore::new(profile).load().unwrap();
        assert!(color(&restored, &fixture.root) == Some(Color::Blue));

        sidebar_click(&ctx, &mut app, "history", egui::PointerButton::Secondary);
        sidebar_click(
            &ctx,
            &mut app,
            "Clear tab marker",
            egui::PointerButton::Primary,
        );
        app.receive(&ctx);
        assert!(painted(&mut app).is_empty());
        assert!(color(&app.settings, &fixture.root).is_none());
        wait_count(&mut app, 0);
        assert_eq!(app.tab_markers.observations.len(), 1);
    }
}
