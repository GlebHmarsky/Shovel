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
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HunkActions {
    ReadOnly,
    Waiting,
    Ready,
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
        actions: HunkActions,
    ) -> Option<Vec<usize>> {
        if diff.kind == Kind::Binary {
            ui.columns(2, |columns| {
                card(
                    &mut columns[0],
                    true,
                    &diff.old,
                    diff.old_extension.as_deref(),
                );
                card(
                    &mut columns[1],
                    false,
                    &diff.new,
                    diff.new_extension.as_deref(),
                );
            });
            return None;
        }
        let mutations = actions == HunkActions::Ready;
        let mut apply = None;
        if diff.kind == Kind::Text {
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
            if actions != HunkActions::ReadOnly && diff.availability != HunkAvailability::Available {
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
pub(super) fn binary_size(bytes: u64) -> String {
    let units = [
        (1 << 40, "BinaryTiB"),
        (1 << 30, "BinaryGiB"),
        (1 << 20, "BinaryMiB"),
        (1 << 10, "BinaryKiB"),
    ];
    if let Some((unit, key)) = units.into_iter().find(|(unit, _)| bytes >= *unit) {
        strings::format(
            "BinarySizeCompact",
            &[&(bytes / unit).to_string(), &text(key), &bytes.to_string()],
        )
    } else {
        strings::format("BinaryExactBytes", &[&bytes.to_string()])
    }
}

fn card(ui: &mut egui::Ui, old: bool, summary: &Summary, extension: Option<&str>) {
    let palette = crate::workspace_style::Palette::current(ui);
    let title = text(if old { "DiffOldSide" } else { "DiffNewSide" });
    let color = if old {
        palette.diff_removed_text
    } else {
        palette.diff_added_text
    };
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong().color(color));
            if summary.lfs {
                egui::Frame::new()
                    .fill(palette.active_selection)
                    .corner_radius(3)
                    .inner_margin(egui::Margin::symmetric(5, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(text("BinaryLfs"))
                                .small()
                                .strong()
                                .color(palette.selection_text),
                        );
                    });
            }
        });
        let width = ui.available_width().clamp(44., 152.);
        let (tile, _) = ui.allocate_exact_size(egui::vec2(width, 105.), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(tile, 5., palette.raised);
        painter.rect_stroke(
            tile,
            5.,
            egui::Stroke::new(1., palette.border),
            egui::StrokeKind::Inside,
        );
        if summary.absent {
            painter.text(
                tile.center(),
                egui::Align2::CENTER_CENTER,
                text("BinaryAbsent"),
                egui::FontId::proportional(13.),
                palette.muted,
            );
        } else {
            let origin = tile.center() - egui::vec2(12., 27.);
            let points = [
                (0., 0.),
                (15., 0.),
                (24., 9.),
                (24., 30.),
                (0., 30.),
                (0., 0.),
            ];
            let stroke = egui::Stroke::new(1.5, palette.text);
            let point = |(x, y)| origin + egui::vec2(x, y);
            painter.add(egui::Shape::line(points.map(point).to_vec(), stroke));
            painter.add(egui::Shape::line(
                [(15., 0.), (15., 9.), (24., 9.)].map(point).to_vec(),
                stroke,
            ));
            painter.with_clip_rect(tile.intersect(ui.clip_rect())).text(
                tile.center() + egui::vec2(0., 20.),
                egui::Align2::CENTER_CENTER,
                extension
                    .map(str::to_owned)
                    .unwrap_or_else(|| text("BinaryNoExtension")),
                egui::FontId::proportional(13.),
                palette.muted,
            );
            if let Some(bytes) = summary.bytes {
                ui.label(binary_size(bytes));
            }
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
    let (label, fill) = line_style(ui, line, side, font, selected, 6);
    egui::Frame::new()
        .fill(fill)
        .show(ui, |ui| {
            ui.add_sized(
                [width, font + 6.],
                egui::Label::new(label).extend().sense(egui::Sense::click()),
            )
        })
        .inner
}

fn line_style(
    ui: &egui::Ui,
    line: &Line,
    side: Option<bool>,
    font: f32,
    selected: bool,
    digits: usize,
) -> (RichText, Color32) {
    let p = crate::workspace_style::Palette::current(ui);
    let (mut color, mut fill) = match line.kind {
        LineKind::Added => (p.diff_added_text, p.diff_added_background),
        LineKind::Removed => (p.diff_removed_text, p.diff_removed_background),
        LineKind::Hunk => (p.focus, p.raised),
        _ => (p.text, Color32::TRANSPARENT),
    };
    if selected {
        color = p.selection_text;
        fill = p.active_selection;
    }
    let old = line.old.map(|n| n.to_string()).unwrap_or_default();
    let new = line.new.map(|n| n.to_string()).unwrap_or_default();
    let marker = match line.kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        _ => ' ',
    };
    let label = match side {
        None => format!("{old:>digits$} {new:>digits$} {marker} {}", line.text),
        Some(new_side) => format!(
            "{:>digits$} {marker} {}",
            if new_side { new } else { old },
            line.text
        ),
    };
    (
        RichText::new(label).monospace().size(font).color(color),
        fill,
    )
}

/// A local example painted with the same line numbers, markers and colors as a real patch.
pub(super) fn settings_preview(ui: &mut egui::Ui, paired: bool, font: f32) {
    static SAMPLE: std::sync::LazyLock<Vec<Line>> = std::sync::LazyLock::new(|| {
        shovel_core::diff::parse_lines("@@ -1,3 +1,3 @@\n fn limit() -> u8 {\n-    10\n+    60\n }")
            .expect("valid built-in patch")
    });
    let p = crate::workspace_style::Palette::current(ui);
    egui::Frame::new()
        .fill(p.canvas)
        .stroke((1., p.border))
        .inner_margin(8)
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.set_min_width(width);
            ui.label(
                RichText::new("config.rs")
                    .monospace()
                    .small()
                    .color(p.muted),
            );
            ui.separator();
            egui::ScrollArea::horizontal()
                .id_salt("settings-diff-sample")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4., 1.);
                    let cell = |ui: &mut egui::Ui, line: &Line, side, width| {
                        let (label, fill) = line_style(ui, line, side, font, false, 1);
                        egui::Frame::new().fill(fill).show(ui, |ui| {
                            ui.set_min_width(width);
                            ui.add(egui::Label::new(label).extend().selectable(false));
                        });
                    };
                    if paired {
                        // Size both columns for the longest numbered sample line at the chosen font.
                        let example = "1   fn limit() -> u8 {";
                        let minimum = ui
                            .painter()
                            .layout_no_wrap(example.into(), egui::FontId::monospace(font), p.text)
                            .size()
                            .x;
                        let half = ((width - ui.spacing().item_spacing.x) / 2.).max(minimum);
                        for pair in align(&SAMPLE) {
                            if pair.shared {
                                cell(ui, &SAMPLE[pair.old.unwrap()], None, width);
                            } else {
                                ui.horizontal(|ui| {
                                    for (side, index) in [(false, pair.old), (true, pair.new)] {
                                        if let Some(index) = index {
                                            cell(ui, &SAMPLE[index], Some(side), half);
                                        } else {
                                            ui.allocate_space(egui::vec2(half, font));
                                        }
                                    }
                                });
                            }
                        }
                    } else {
                        for line in SAMPLE.iter() {
                            cell(ui, line, None, width);
                        }
                    }
                });
        });
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
