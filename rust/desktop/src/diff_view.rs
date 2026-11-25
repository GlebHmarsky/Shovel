//! Shared virtualized unified and paired diff presentation.
use crate::strings::{self, text};
use eframe::egui::{self, Color32, RichText};
use shovel_core::diff::{FileDiff, Kind, Line, LineKind, Summary};

pub struct View {
    pub side_by_side: bool,
    pub font_size: f32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            side_by_side: false,
            font_size: 12.,
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
    pub fn show(&self, ui: &mut egui::Ui, diff: &FileDiff) {
        if diff.kind == Kind::Binary {
            if diff.old.absent && !diff.new.absent {
                card(ui, false, &diff.new, diff.new_extension.as_deref());
                return;
            }
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
            return;
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
        }
        if diff.lines.is_empty() {
            if diff.kind == Kind::Text {
                ui.label(text("DiffNoTextChanges"));
            }
            return;
        }
        let font = self.font_size.clamp(9., 24.);
        let spacing = ui.spacing().item_spacing;
        let height = font + 6.;
        let width = ui.available_width().max(250.);
        let pairs = self.side_by_side.then(|| align(&diff.lines));
        let half = if pairs.is_some() {
            let chars = diff.lines.iter().map(|line| line.text.len()).max().unwrap_or(0) + 12;
            let advance = ui.fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(font), '0'));
            ((width - spacing.x) / 2.).max(chars as f32 * advance + 8.)
        } else {
            0.
        };
        let rows = pairs.as_ref().map_or(diff.lines.len(), Vec::len);
        egui::ScrollArea::both().id_salt("diff_rows").show_rows(ui, height, rows, |ui, range| {
            for index in range {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let Some(pair) = pairs.as_ref().map(|pairs| &pairs[index]) else {
                        row(ui, &diff.lines[index], None, font, width);
                        return;
                    };
                    if pair.shared {
                        row(ui, &diff.lines[pair.old.unwrap()], None, font, width);
                    } else {
                        for (side, line) in [(false, pair.old), (true, pair.new)] {
                            if let Some(i) = line {
                                row(ui, &diff.lines[i], Some(side), font, half);
                            } else {
                                ui.allocate_space(egui::vec2(half, height));
                            }
                        }
                    }
                });
            }
        });
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
    let color = if old {
        palette.diff_removed_text
    } else {
        palette.diff_added_text
    };
    let title = RichText::new(text(if old { "DiffOldSide" } else { "DiffNewSide" }))
        .strong()
        .color(color);
    let badge = summary.lfs.then(|| {
        RichText::new(text("BinaryLfs"))
            .small()
            .strong()
            .color(palette.selection_text)
    });
    let width = |label: &RichText| {
        egui::WidgetText::from(label.clone())
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
            .size()
            .x
    };
    let heading_width = width(&title)
        + badge
            .as_ref()
            .map_or(0., |label| width(label) + 10. + ui.spacing().item_spacing.x);
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(heading_width, 0.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(title);
                if let Some(badge) = badge {
                    egui::Frame::new()
                        .fill(palette.active_selection)
                        .corner_radius(3)
                        .inner_margin(egui::Margin::symmetric(5, 2))
                        .show(ui, |ui| ui.label(badge));
                }
            },
        );
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
fn row(ui: &mut egui::Ui, line: &Line, side: Option<bool>, font: f32, width: f32) {
    let (label, color, fill) = line_style(ui, line, side, 6);
    let galley = ui.painter().layout_no_wrap(label, egui::FontId::monospace(font), color);
    let width = if side.is_none() { width.max(galley.size().x + 8.) } else { width };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, font + 6.), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, galley.text()));
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0., fill);
    let origin = egui::pos2(rect.left() + 4., rect.center().y - galley.size().y / 2.);
    painter.galley(origin, galley, color);
}

fn line_style(
    ui: &egui::Ui,
    line: &Line,
    side: Option<bool>,
    digits: usize,
) -> (String, Color32, Color32) {
    let p = crate::workspace_style::Palette::current(ui);
    let (color, fill) = match line.kind {
        LineKind::Added => (p.diff_added_text, p.diff_added_background),
        LineKind::Removed => (p.diff_removed_text, p.diff_removed_background),
        LineKind::Hunk => (p.focus, p.raised),
        _ => (p.text, Color32::TRANSPARENT),
    };
    let old = line.old.map(|n| n.to_string()).unwrap_or_default();
    let new = line.new.map(|n| n.to_string()).unwrap_or_default();
    let marker = match line.kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        _ => ' ',
    };
    let label = match side {
        None => format!("{old:<digits$} {new:<digits$} {marker} {}", line.text),
        Some(new_side) => format!(
            "{:<digits$} {marker} {}",
            if new_side { new } else { old },
            line.text
        ),
    };
    (label, color, fill)
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
                        let (label, color, fill) = line_style(ui, line, side, 1);
                        let label = RichText::new(label).monospace().size(font).color(color);
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
