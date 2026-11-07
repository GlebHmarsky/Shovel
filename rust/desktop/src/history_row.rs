//! The history list has a single focusable target per row. Text columns share
//! their page geometry, including when a ref or subject is unusually long.
use crate::workspace_style::Palette;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Ui};
use sha2::{Digest, Sha256};
use shovel_core::git::Entry;
use unicode_segmentation::UnicodeSegmentation;

fn text(ui: &Ui, rect: Rect, label: &str, font: FontId, color: Color32) {
    if rect.width() <= 0. {
        return;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let top = rect.center().y - galley.size().y / 2.;
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(egui::pos2(rect.left(), top), galley, color);
}

fn graph(ui: &Ui, entry: &Entry, rect: Rect, p: &Palette) {
    let colors = if ui.visuals().dark_mode {
        [0xdba24a, 0xd78279, 0x82b787, 0x7eabe0, 0xb398d5]
    } else {
        [0x855812, 0x9a3836, 0x387348, 0x355f99, 0x75538e]
    };
    let lane_color = |i: usize| {
        let value = colors[i % colors.len()];
        if p.high_contrast {
            p.text
        } else {
            Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
        }
    };
    let point = |lane: usize, y: f32| {
        egui::pos2(
            rect.left() + 8. + 16. * lane as f32,
            rect.top() + rect.height() * y,
        )
    };
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    for line in &entry.graph.lines {
        let a = point(line.from, line.from_y);
        let b = point(line.to, line.to_y);
        let stroke = Stroke::new(1.7, lane_color(line.color));
        if line.from == line.to {
            painter.line_segment([a, b], stroke);
        } else {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [a, Pos2::new(a.x, b.y), Pos2::new(b.x, a.y), b],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
    }
    let center = point(entry.graph.node, 0.5);
    let color = lane_color(entry.graph.color);
    if entry.graph.is_stash {
        painter.rect_stroke(
            Rect::from_center_size(center, egui::vec2(8., 8.)),
            2.,
            Stroke::new(1.6, color),
            egui::StrokeKind::Inside,
        );
    } else if entry.graph.is_merge {
        painter.circle_stroke(center, 4., Stroke::new(1.5, color));
        painter.circle_filled(center, 1.3, color);
    } else {
        painter.circle_filled(center, 3.3, color);
    }
}

pub fn show(
    ui: &mut Ui,
    entry: &Entry,
    lanes: usize,
    selected: bool,
    peer: bool,
    refs_right: bool,
    p: &Palette,
) -> egui::Response {
    let graph_width = lanes as f32 * 16.;
    let width = ui.available_width().max(graph_width + 535.);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 23.), egui::Sense::click());
    let c = &entry.commit;
    let refs = entry
        .references
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = format!(
        "{}  {}  {}  {}  {}",
        c.subject(),
        c.author,
        c.authored,
        c.hash,
        refs
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, &description)
    });
    let background = if selected {
        if response.has_focus() {
            p.active_selection
        } else {
            p.selected
        }
    } else if response.hovered() {
        p.hover
    } else if peer {
        p.author_peer
    } else {
        p.canvas
    };
    let fg = if selected || response.hovered() {
        p.selection_text
    } else {
        p.history_text
    };
    ui.painter().rect_filled(rect, 0., background);
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 0., Stroke::new(1., p.focus), egui::StrokeKind::Inside);
    }
    let column = |x: f32, w: f32| {
        Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()))
    };
    graph(ui, entry, column(rect.left(), graph_width), p);
    let hash = column(rect.right() - 76., 72.);
    let date = column(hash.left() - 90., 86.);
    let author = column(date.left() - 134., 130.);
    let font = FontId::proportional(13.);
    let secondary = if selected || response.hovered() {
        fg
    } else {
        p.muted
    };
    text(
        ui,
        hash,
        &c.hash[..c.hash.len().min(8)],
        FontId::monospace(12.),
        secondary,
    );
    text(
        ui,
        date,
        &c.authored.format("%Y-%m-%d").to_string(),
        font.clone(),
        secondary,
    );
    let avatar = Rect::from_center_size(
        egui::pos2(author.left() + 10., author.center().y),
        egui::vec2(20., 20.),
    );
    paint_avatar(ui, avatar, c, p);
    text(
        ui,
        column(author.left() + 25., author.width() - 25.),
        &c.author,
        font.clone(),
        fg,
    );
    let mut subject = column(
        rect.left() + graph_width + 6.,
        author.left() - rect.left() - graph_width - 12.,
    );
    let budget = (subject.width() * 0.48).min(210.);
    let mut badges = Vec::new();
    let mut used = 0.;
    for r in &entry.references {
        let name = r
            .name
            .trim_start_matches("refs/heads/")
            .trim_start_matches("refs/remotes/")
            .trim_start_matches("refs/tags/");
        let size = ui
            .painter()
            .layout_no_wrap(name.to_owned(), FontId::proportional(11.), fg)
            .size()
            .x;
        let width = (size + 12.).min(140.).min(budget - used);
        if width < 24. {
            break;
        }
        badges.push((name, width));
        used += width + 4.;
    }
    let mut x = if refs_right {
        subject.right() - used
    } else {
        subject.left()
    };
    for (name, width) in badges {
        let badge = Rect::from_min_size(egui::pos2(x, rect.top() + 3.), egui::vec2(width, 17.));
        ui.painter().rect_stroke(
            badge,
            3.,
            Stroke::new(1., secondary),
            egui::StrokeKind::Inside,
        );
        text(
            ui,
            badge.shrink2(egui::vec2(4., 0.)),
            name,
            FontId::proportional(11.),
            fg,
        );
        x += width + 4.;
    }
    if refs_right {
        subject.max.x -= used;
    } else {
        subject.min.x += used;
    }
    text(ui, subject, c.subject(), font, fg);
    response.on_hover_text(format!(
        "{}\n{} <{}>\n{}\n{}\n{}",
        c.message, c.author, c.email, c.authored, c.hash, refs
    ))
}

pub fn paint_avatar(ui: &Ui, avatar: Rect, c: &shovel_core::git::Commit, p: &Palette) {
    let email = c.email.trim().to_lowercase();
    let digest = Sha256::digest(if email.is_empty() {
        c.author.trim().as_bytes()
    } else {
        email.as_bytes()
    });
    let color = if p.high_contrast {
        p.active_selection
    } else {
        Color32::from_rgb(
            45 + digest[0] % 65,
            45 + digest[1] % 65,
            45 + digest[2] % 65,
        )
    };
    ui.painter().rect_filled(avatar, 4., color);
    let initial = c
        .author
        .trim()
        .graphemes(true)
        .next()
        .unwrap_or("?")
        .to_uppercase();
    ui.painter().text(
        avatar.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(avatar.height() * 0.49),
        if p.high_contrast {
            p.selection_text
        } else {
            Color32::WHITE
        },
    );
}
