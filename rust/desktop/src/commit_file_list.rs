use super::*;
use crate::workspace_style::Palette;

/// The two panes borrow sorted slices from the same reviewed snapshot.
pub(super) struct FileList<'a> {
    groups: [&'a [Change]; 3],
}
pub(super) enum Row<'a> {
    Heading(Group, usize),
    File(&'a Change),
}
impl<'a> FileList<'a> {
    pub fn new(context: Option<&'a Context>, staged: bool) -> Self {
        Self::from_entries(context.map_or(&[], |context| &context.entries), staged)
    }

    pub fn from_entries(entries: &'a [Change], staged: bool) -> Self {
        let rows = |group| {
            let start = entries.partition_point(|entry| entry.group < group);
            let remaining = &entries[start..];
            &remaining[..remaining.partition_point(|entry| entry.group == group)]
        };
        let groups = if staged {
            [rows(Group::Staged), &[], &[]]
        } else {
            [
                rows(Group::Conflicted),
                rows(Group::Unstaged),
                rows(Group::Untracked),
            ]
        };
        Self { groups }
    }
    pub fn count(&self) -> usize {
        self.groups.iter().map(|group| group.len()).sum()
    }
    pub fn row_count(&self) -> usize {
        self.count()
            + self.groups.iter().filter(|group| {
                group.first().is_some_and(|entry| has_heading(entry.group))
            }).count()
    }
    pub fn row(&self, mut index: usize) -> Option<Row<'a>> {
        for group in self.groups {
            let Some(first) = group.first() else { continue };
            if has_heading(first.group) {
                if index == 0 {
                    return Some(Row::Heading(first.group, group.len()));
                }
                index -= 1;
            }
            if let Some(entry) = group.get(index) {
                return Some(Row::File(entry));
            }
            index -= group.len();
        }
        None
    }
    pub fn entries(&self) -> impl Iterator<Item = &'a Change> {
        self.groups.into_iter().flatten()
    }
    pub fn inclusive_range(
        &self,
        anchor: &(Group, String),
        end: &(Group, String),
    ) -> Option<impl Iterator<Item = &'a Change>> {
        let index = |(group, path): &(Group, String)| {
            self.entries().position(|file| file.group == *group && file.path == *path)
        };
        let first = index(anchor)?;
        let last = index(end)?;
        Some(self.entries().skip(first.min(last)).take(first.abs_diff(last) + 1))
    }
    pub fn stageable(&self, staged: bool) -> impl Iterator<Item = &'a Change> {
        self.entries().filter(move |entry| staged || entry.group.can_stage())
    }
}
fn has_heading(group: Group) -> bool {
    matches!(group, Group::Conflicted | Group::Untracked)
}
fn group_name(group: Group) -> String {
    text(match group {
        Group::Conflicted => "ChangesConflicted",
        Group::Staged => "ChangesStaged",
        Group::Unstaged => "ChangesUnstaged",
        Group::Untracked => "ChangesUntracked",
        Group::Ignored => "ChangesIgnored",
    })
}
pub(super) fn heading(ui: &mut egui::Ui, group: Group, count: usize) {
    let name = group_name(group);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.), egui::Sense::hover());
    let palette = Palette::current(ui);
    ui.painter().rect_filled(rect, 0., palette.raised);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1., palette.border),
    );
    ui.put(
        rect.shrink2(egui::vec2(6., 0.)),
        egui::Label::new(
            egui::RichText::new(strings::format(
                "CommitFileGroup",
                &[&name, &count.to_string()],
            ))
            .small()
            .strong(),
        )
        .halign(egui::Align::Min),
    );
}
pub(super) fn file_row(ui: &mut egui::Ui, entry: &Change, selected: bool) -> egui::Response {
    let p = Palette::current(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.), egui::Sense::click());
    let window_focused = ui.input(|input| input.focused);
    let highlighted = selected || response.hovered();
    if highlighted {
        let fill = if selected && window_focused {
            p.active_selection
        } else if selected && !p.high_contrast && !ui.visuals().dark_mode {
            egui::Color32::from_gray(96)
        } else if selected {
            p.selected
        } else {
            p.hover
        };
        ui.painter().rect_filled(rect, 2., fill);
    }
    if window_focused && response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            2.,
            egui::Stroke::new(1., p.focus),
            egui::StrokeKind::Inside,
        );
    }
    let fg = if highlighted {
        p.selection_text
    } else {
        p.text
    };
    let status_color = if p.high_contrast {
        fg
    } else {
        Palette::change_status(
            if entry.group == Group::Conflicted { 'M' } else { entry.code },
            ui.visuals().dark_mode || highlighted,
        )
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.text(
        rect.left_center() + egui::vec2(8., 0.),
        egui::Align2::LEFT_CENTER,
        if entry.group == Group::Conflicted { '!' } else { entry.code },
        egui::FontId::monospace(12.),
        status_color,
    );
    let path = crate::path_text::PathText::new(ui.ctx(), &entry.path);
    let original = entry
        .original
        .as_ref()
        .map(|p| crate::path_text::PathText::new(ui.ctx(), p));
    let mut job = egui::text::LayoutJob::simple_singleline(
        path.visual().to_owned(),
        egui::FontId::proportional(13.),
        fg,
    );
    job.wrap.max_width = (rect.width() - 31.).max(1.);
    job.wrap.max_rows = 1;
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(rect.left() + 25., rect.center().y - galley.size().y / 2.),
        galley,
        fg,
    );
    let kind = text(&format!("ChangeCode{}", entry.code));
    let group = group_name(entry.group);
    let label = |path: &str, original: Option<&str>| {
        let status = if entry.group == Group::Untracked {
            strings::format("CommitUntrackedRow", &[&group, path])
        } else {
            strings::format("CommitChangeRow", &[&group, &kind, path])
        };
        match original {
            Some(old) => strings::format("CommitOriginalPath", &[&status, old]),
            None => status,
        }
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            ui.is_enabled(),
            selected,
            label(path.spoken(), original.as_ref().map(|p| p.spoken())),
        )
    });
    response.on_hover_ui(|ui| {
        ui.label(label(path.visual(), original.as_ref().map(|p| p.visual())));
    })
}
