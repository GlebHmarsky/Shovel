//! Shared virtualized unified/paired presentation. Row selection maps back to
//! immutable native hunk IDs in either layout, including padded rows.
use crate::strings::{self, text};
use eframe::egui::{self, Color32, RichText};
use shovel_core::diff::{FileDiff, HunkAvailability, Kind, Line, LineKind, Summary};
use std::collections::BTreeSet;

pub struct View {
    pub side_by_side: bool,
    pub font_size: f32,
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    scroll_to: Option<usize>,
    navigation: Option<usize>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            side_by_side: false,
            font_size: 12.,
            selected: BTreeSet::new(),
            anchor: None,
            scroll_to: None,
            navigation: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pair {
    old: Option<usize>,
    new: Option<usize>,
    shared: bool,
}
fn align(lines: &[Line]) -> Vec<Pair> {
    let mut pairs = vec![];
    let mut i = 0;
    while i < lines.len() {
        if matches!(lines[i].kind, LineKind::Removed | LineKind::Added) {
            let mut removed = vec![];
            let mut added = vec![];
            while i < lines.len() && matches!(lines[i].kind, LineKind::Removed | LineKind::Added) {
                if lines[i].kind == LineKind::Removed {
                    removed.push(i);
                } else {
                    added.push(i);
                }
                i += 1;
            }
            for n in 0..removed.len().max(added.len()) {
                pairs.push(Pair {
                    old: removed.get(n).copied(),
                    new: added.get(n).copied(),
                    shared: false,
                });
            }
        } else {
            pairs.push(Pair {
                old: Some(i),
                new: Some(i),
                shared: matches!(lines[i].kind, LineKind::Metadata | LineKind::Hunk),
            });
            i += 1;
        }
    }
    pairs
}
impl View {
    pub fn reset(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.scroll_to = None;
        self.navigation = None;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        diff: &FileDiff,
        mutations: bool,
    ) -> Option<Vec<usize>> {
        let mut apply = None;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.side_by_side, text("DiffSideBySide"));
            if let Some(preview) = &diff.hunks {
                let label = if preview.staged() {
                    "HunkUnstageMany"
                } else {
                    "HunkStageMany"
                };
                if ui
                    .add_enabled(
                        mutations && !self.selected.is_empty(),
                        egui::Button::new(strings::format(
                            label,
                            &[&self.selected.len().to_string()],
                        )),
                    )
                    .on_disabled_hover_text(text("HunkSelectChange"))
                    .clicked()
                {
                    apply = Some(self.selected.iter().copied().collect());
                }
            }
            let hunks = diff
                .lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.kind == LineKind::Hunk)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            for (next, key) in [(false, "DiffPreviousHunk"), (true, "DiffNextHunk")] {
                if ui
                    .add_enabled(!hunks.is_empty(), egui::Button::new(text(key)))
                    .clicked()
                {
                    self.scroll_to = if next {
                        hunks
                            .iter()
                            .copied()
                            .find(|i| *i > self.navigation.unwrap_or(0))
                            .or_else(|| hunks.first().copied())
                    } else {
                        hunks
                            .iter()
                            .copied()
                            .rev()
                            .find(|i| *i < self.navigation.unwrap_or(usize::MAX))
                            .or_else(|| hunks.last().copied())
                    };
                    self.navigation = self.scroll_to;
                }
            }
        });
        if diff.kind == Kind::Binary {
            ui.columns(2, |columns| {
                card(&mut columns[0], "DiffOldSide", &diff.old);
                card(&mut columns[1], "DiffNewSide", &diff.new);
            });
            return apply;
        }
        let reason = match diff.kind {
            Kind::Directory => Some("DiffSummaryDirectory"),
            Kind::UnsupportedPath => Some("DiffSummaryUnsupportedPath"),
            Kind::UnsupportedEncoding => Some("DiffSummaryUnsupportedEncoding"),
            Kind::TooLarge => Some("DiffSummaryTooLarge"),
            Kind::Submodule => Some("DiffSummarySubmodule"),
            _ => None,
        };
        if let Some(reason) = reason {
            ui.label(strings::format(
                reason,
                &[&size(&diff.old), &size(&diff.new)],
            ));
        }
        if diff.kind == Kind::Text {
            ui.label(strings::format(
                "DiffSummaryText",
                &[
                    &size(&diff.old),
                    &size(&diff.new),
                    &diff.added.to_string(),
                    &diff.removed.to_string(),
                ],
            ));
            if mutations && diff.availability != HunkAvailability::Available {
                ui.label(text(match diff.availability {
                    HunkAvailability::ChangeType => "HunkUnavailableChangeType",
                    HunkAvailability::PreviewOptions => "HunkUnavailablePreviewOptions",
                    HunkAvailability::ContentType => "HunkUnavailableContentType",
                    HunkAvailability::Filtered => "HunkUnavailableFiltered",
                    _ => "HunkUnavailableNoChanges",
                }));
            }
        }
        if diff.lines.is_empty() {
            if diff.kind == Kind::Text {
                ui.label(text("DiffNoTextChanges"));
            }
            return apply;
        }
        let font = self.font_size.clamp(9., 24.);
        let height = font + 6.;
        let pairs = if self.side_by_side {
            align(&diff.lines)
        } else {
            (0..diff.lines.len())
                .map(|i| Pair {
                    old: Some(i),
                    new: None,
                    shared: true,
                })
                .collect()
        };
        let mut scroll = egui::ScrollArea::both().id_salt("diff_rows");
        if let Some(target) = self.scroll_to {
            if let Some(row) = pairs
                .iter()
                .position(|p| p.old == Some(target) || p.new == Some(target))
            {
                scroll = scroll.vertical_scroll_offset(row as f32 * height);
            }
            self.scroll_to = None;
        }
        let width = ui.available_width().max(250.);
        let mut clicked = None;
        scroll.show_rows(ui, height, pairs.len(), |ui, range| {
            for index in range {
                let pair = &pairs[index];
                let source = pair.old.or(pair.new).unwrap();
                let hunk = diff
                    .hunks
                    .as_ref()
                    .and_then(|p| {
                        p.hunks()
                            .iter()
                            .find(|h| source >= h.start && source < h.start + h.count)
                    })
                    .map(|h| h.id);
                let selected = hunk.is_some_and(|h| self.selected.contains(&h));
                ui.horizontal(|ui| {
                    if pair.shared {
                        if let Some(i) = pair.old {
                            let response = row(ui, &diff.lines[i], None, font, width, selected);
                            if response.clicked() {
                                clicked = hunk;
                            }
                        }
                    } else {
                        let half = (width - ui.spacing().item_spacing.x) / 2.;
                        for (side, line) in [(false, pair.old), (true, pair.new)] {
                            if let Some(i) = line {
                                if row(ui, &diff.lines[i], Some(side), font, half, selected)
                                    .clicked()
                                {
                                    clicked = hunk;
                                }
                            } else {
                                ui.allocate_space(egui::vec2(half, height));
                            }
                        }
                    }
                });
            }
        });
        if let Some(hunk) = clicked {
            let modifiers = ui.input(|i| i.modifiers);
            if modifiers.shift
                && let Some(anchor) = self.anchor
            {
                self.selected.extend(anchor.min(hunk)..=anchor.max(hunk));
            } else if modifiers.ctrl {
                if !self.selected.remove(&hunk) {
                    self.selected.insert(hunk);
                }
                self.anchor = Some(hunk);
            } else {
                self.selected.clear();
                self.selected.insert(hunk);
                self.anchor = Some(hunk);
            }
        }
        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C))
            && !self.selected.is_empty()
            && let Some(preview) = &diff.hunks
        {
            let copied = preview
                .hunks()
                .iter()
                .filter(|h| self.selected.contains(&h.id))
                .flat_map(|h| {
                    diff.lines[h.start..h.start + h.count]
                        .iter()
                        .map(|l| l.text.as_str())
                })
                .collect::<Vec<_>>()
                .join("\n");
            ui.ctx().copy_text(copied);
        }
        apply
    }
}
fn size(summary: &Summary) -> String {
    summary
        .bytes
        .map(|n| n.to_string())
        .unwrap_or_else(|| text("CountUnavailable"))
}
fn card(ui: &mut egui::Ui, key: &str, summary: &Summary) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong(text(key));
        ui.label(text(if summary.absent {
            "BinaryAbsent"
        } else if summary.lfs {
            "BinaryLfs"
        } else {
            "BinaryStoredFile"
        }));
        if !summary.absent {
            ui.label(strings::format("BinaryExactBytes", &[&size(summary)]));
        }
    });
}
fn row(
    ui: &mut egui::Ui,
    line: &Line,
    side: Option<bool>,
    font: f32,
    width: f32,
    selected: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let color = match line.kind {
        LineKind::Added => {
            if dark {
                Color32::from_rgb(135, 207, 150)
            } else {
                Color32::from_rgb(24, 105, 48)
            }
        }
        LineKind::Removed => {
            if dark {
                Color32::from_rgb(235, 152, 152)
            } else {
                Color32::from_rgb(156, 32, 32)
            }
        }
        LineKind::Hunk => {
            if dark {
                Color32::from_rgb(139, 178, 225)
            } else {
                Color32::from_rgb(25, 80, 150)
            }
        }
        _ => ui.visuals().text_color(),
    };
    let old = line.old.map(|n| n.to_string()).unwrap_or_default();
    let new = line.new.map(|n| n.to_string()).unwrap_or_default();
    let marker = match line.kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        _ => ' ',
    };
    let label = match side {
        None => format!("{old:>6} {new:>6} {marker} {}", line.text),
        Some(new_side) => format!(
            "{:>6} {marker} {}",
            if new_side { new } else { old },
            line.text
        ),
    };
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .show(ui, |ui| {
            ui.add_sized(
                [width, font + 6.],
                egui::Label::new(RichText::new(label).monospace().size(font).color(color))
                    .extend()
                    .sense(egui::Sense::click()),
            )
        })
        .inner
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unequal_replacements_keep_numbered_context_and_padding() {
        let lines = shovel_core::diff::parse_lines(
            "@@ -1,3 +1,4 @@\n same\n-old\n+new one\n+new two\n end\n",
        )
        .unwrap();
        let rows = align(&lines);
        assert_eq!(
            rows[2],
            Pair {
                old: Some(2),
                new: Some(3),
                shared: false
            }
        );
        assert_eq!(
            rows[3],
            Pair {
                old: None,
                new: Some(4),
                shared: false
            }
        );
        assert_eq!(lines[rows[4].old.unwrap()].old, Some(3));
        assert_eq!(lines[rows[4].new.unwrap()].new, Some(4));
    }
}
