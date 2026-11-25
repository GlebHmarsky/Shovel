//! The history list has a single focusable target per row. Text columns share
//! their page geometry, including when a ref or subject is unusually long.
use crate::{strings, workspace_style::Palette};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Ui};
use sha2::{Digest, Sha256};
use shovel_core::{git::{Commit, Entry}, graph::StashLink};
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn text(ui: &Ui, rect: Rect, label: &str, font: FontId, color: Color32) {
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

pub(super) const HEIGHT: f32 = 23.;

pub(super) fn author_details(ctx: &egui::Context, commit: &Commit) -> String {
    strings::format(
        "RustCommitAuthorDetails",
        &[
            &commit.author,
            &commit.email,
            &crate::dates::original(ctx, &commit.authored),
        ],
    )
}

pub(super) fn committer_details(ctx: &egui::Context, commit: &Commit) -> String {
    strings::format(
        "RustCommitterDetails",
        &[
            &commit.committer,
            &commit.committer_email,
            &crate::dates::original(ctx, &commit.committed),
        ],
    )
}

pub(super) fn visible_ref(name: &str) -> bool {
    name != "HEAD"
        && !name
            .strip_prefix("refs/remotes/")
            .and_then(|name| name.split_once('/'))
            .is_some_and(|(_, branch)| branch == "HEAD")
}

pub(super) struct Style<'a> {
    pub list_focused: bool,
    pub worktree: &'a std::path::Path,
    pub selected: bool,
    pub peer: bool,
    pub incoming: bool,
    pub refs_right: bool,
}

fn graph(ui: &Ui, entry: &Entry, rect: Rect, p: &Palette, foreground: Color32, incoming: bool) {
    let colors = if ui.visuals().dark_mode {
        [0xdba24a, 0xd78279, 0x82b787, 0x7eabe0, 0xb398d5]
    } else {
        [0x855812, 0x9a3836, 0x387348, 0x355f99, 0x75538e]
    };
    let lane_color = |i: usize| {
        let value = colors[i % colors.len()];
        if p.high_contrast || incoming {
            foreground
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
    let curve = |a: Pos2, b: Pos2, stroke: Stroke| {
        if a.x == b.x {
            painter.line_segment([a, b], stroke);
        } else {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [a, Pos2::new(a.x, b.y), Pos2::new(b.x, a.y), b],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
    };
    let center = point(entry.graph.node, 0.5);
    let stash_color = if p.high_contrast {
        foreground
    } else {
        Color32::from_gray(if ui.visuals().dark_mode { 168 } else { 102 })
    };
    let stash_stroke = Stroke::new(1.3, stash_color);
    let stash_point = |y| point(entry.graph.node, y) + egui::vec2(9., 0.);
    let rail = |y, shared| stash_point(y) - egui::vec2(if shared { 5. } else { 0. }, 0.);
    match entry.graph.stash_link {
        Some(StashLink::Leaf { first, shared }) => {
            let marker = stash_point(0.5);
            if shared {
                painter.line_segment(
                    [rail(0.5, shared), marker - egui::vec2(3., 0.)],
                    stash_stroke,
                );
            }
            let start = if !first {
                rail(0., shared)
            } else if shared {
                rail(0.5, shared)
            } else {
                marker + egui::vec2(0., 3.)
            };
            painter.line_segment([start, rail(1., shared)], stash_stroke);
        }
        Some(StashLink::Base { shared }) => curve(rail(0., shared), center, stash_stroke),
        None => {}
    }
    for line in &entry.graph.lines {
        curve(
            point(line.from, line.from_y),
            point(line.to, line.to_y),
            Stroke::new(1.7, lane_color(line.color)),
        );
    }
    let color = lane_color(entry.graph.color);
    if entry.graph.is_stash() {
        painter.circle_filled(stash_point(0.5), 3.3, stash_color);
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
    style: Style<'_>,
    p: &Palette,
) -> egui::Response {
    let Style {
        list_focused,
        worktree,
        selected,
        peer,
        incoming,
        refs_right,
    } = style;
    let graph_width = lanes as f32 * 16.;
    let width = ui.available_width().max(graph_width + 599.);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::click());
    let c = &entry.commit;
    let committed = crate::dates::local(ui.ctx(), &c.committed);
    let refs = entry
        .references
        .iter()
        .filter(|r| visible_ref(&r.name))
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let empty_subject = c
        .subject()
        .is_empty()
        .then(|| strings::text("EmptyCommitSubject"));
    let subject_text = empty_subject.as_deref().unwrap_or_else(|| c.subject());
    response.widget_info(|| {
        let mut description = strings::format(
            "RustHistoryRowAccessible",
            &[subject_text, &c.author, &committed, &c.hash],
        );
        if !refs.is_empty() {
            description = strings::format("RustHistoryRowWithRefs", &[&description, &refs]);
        }
        if incoming {
            description = strings::format("RustHistoryRowIncoming", &[&description]);
        }
        if peer {
            description = strings::format("SameAuthorAccessible", &[&description]);
        }
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, &description)
    });
    let background = if selected {
        if response.has_focus() || list_focused {
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
    } else if incoming && !p.high_contrast {
        p.muted.lerp_to_gamma(p.canvas, 0.16)
    } else {
        p.history_text
    };
    ui.painter().rect_filled(rect, 0., background);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0.,
            Stroke::new(1., if p.high_contrast { fg } else { p.focus }),
            egui::StrokeKind::Inside,
        );
    }
    let column = |x: f32, w: f32| {
        Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()))
    };
    graph(ui, entry, column(rect.left(), graph_width), p, fg, incoming);
    let hash = column(rect.right() - 76., 72.);
    let date = column(hash.left() - 154., 150.);
    let author = column(date.left() - 134., 130.);
    let font = FontId::proportional(13.);
    let secondary = if selected || response.hovered() || incoming {
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
    text(ui, date, &committed, font.clone(), secondary);
    let avatar = Rect::from_center_size(
        egui::pos2(author.left() + 10., author.center().y),
        egui::vec2(20., 20.),
    );
    paint_avatar(ui, avatar, c, p, worktree);
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
    for r in entry.references.iter().filter(|r| visible_ref(&r.name)) {
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
        let (badge_fg, badge_bg) = if p.high_contrast || incoming || entry.graph.is_stash() {
            (fg, None)
        } else if r.name.starts_with("refs/heads/") {
            (p.diff_added_text, Some(p.diff_added_background))
        } else if r.name.starts_with("refs/remotes/") {
            (p.diff_removed_text, Some(p.diff_removed_background))
        } else {
            (fg, None)
        };
        badges.push((name, width, badge_fg, badge_bg));
        used += width + 4.;
    }
    let mut x = if refs_right {
        subject.right() - used
    } else {
        subject.left()
    };
    for (name, width, badge_fg, badge_bg) in badges {
        let badge = Rect::from_min_size(egui::pos2(x, rect.top() + 3.), egui::vec2(width, 17.));
        if let Some(bg) = badge_bg {
            ui.painter().rect_filled(badge, 3., bg);
        }
        ui.painter().rect_stroke(
            badge,
            3.,
            Stroke::new(1., badge_bg.map_or(secondary, |_| badge_fg)),
            egui::StrokeKind::Inside,
        );
        text(
            ui,
            badge.shrink2(egui::vec2(4., 0.)),
            name,
            FontId::proportional(11.),
            badge_fg,
        );
        x += width + 4.;
    }
    if refs_right {
        subject.max.x -= used;
    } else {
        subject.min.x += used;
    }
    text(ui, subject, subject_text, font, fg);
    response.on_hover_ui(|ui| {
        ui.label(if c.message.is_empty() {
            subject_text
        } else {
            &c.message
        });
        ui.label(author_details(ui.ctx(), c));
        ui.label(committer_details(ui.ctx(), c));
        ui.monospace(&c.hash);
        if !refs.is_empty() {
            ui.label(refs);
        }
        if incoming {
            ui.label(strings::text("HistoryIncoming"));
        }
    })
}

pub fn paint_avatar(
    ui: &Ui,
    avatar: Rect,
    c: &shovel_core::git::Commit,
    p: &Palette,
    worktree: &std::path::Path,
) {
    if let Some(texture) = crate::avatars::picture(ui.ctx(), worktree, c) {
        let size = texture.size_vec2();
        let uv = Rect::from_center_size(egui::pos2(0.5, 0.5), egui::Vec2::splat(size.min_elem()) / size);
        egui::Image::new(&texture)
            .uv(uv)
            .corner_radius(4)
            .paint_at(ui, avatar);
        return;
    }
    let email = c.email.trim().to_lowercase();
    let digest = Sha256::digest(if email.is_empty() {
        c.author.trim().as_bytes()
    } else {
        email.as_bytes()
    });
    let ink = if p.high_contrast {
        ui.painter().rect_filled(avatar, 4., p.active_selection);
        p.selection_text
    } else {
        let palette = [
            0xc94d63, 0xbc5839, 0xa56e24, 0x7d872d, 0x478751, 0x308979,
            0x2b819b, 0x406dc0, 0x615fb7, 0x8756ae, 0xae508f, 0x89685a,
        ];
        let color = palette[usize::from(digest[0]) * palette.len() / 256];
        let bottom = Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8);
        let top = bottom.lerp_to_gamma(Color32::BLACK, 0.10);
        paint_avatar_gradient(ui, avatar, top, bottom);
        let luminance = |color| {
            let linear = egui::Rgba::from(color);
            0.2126 * linear.r() + 0.7152 * linear.g() + 0.0722 * linear.b()
        };
        if (luminance(top) + 0.05) / 0.05 > 1.05 / (luminance(bottom) + 0.05) {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    };
    ui.painter().text(
        avatar.center(),
        Align2::CENTER_CENTER,
        initials(&c.author),
        FontId::proportional(avatar.height() * 0.45),
        ink,
    );
}

fn initials(author: &str) -> String {
    let mut initials = String::new();
    for word in author.split_whitespace().take(2) {
        if let Some(initial) = word.graphemes(true).next()
            && let Some(upper) = initial.to_uppercase().graphemes(true).next()
        {
            initials.push_str(upper);
        }
    }
    if initials.is_empty() { "?".into() } else { initials }
}

fn paint_avatar_gradient(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let radius = 4.0_f32.min(rect.width().min(rect.height()) / 2.);
    let corners = [
        (rect.left_top() + egui::vec2(radius, radius), PI),
        (rect.right_top() + egui::vec2(-radius, radius), PI + FRAC_PI_2),
        (rect.right_bottom() - egui::vec2(radius, radius), 0.),
        (rect.left_bottom() + egui::vec2(radius, -radius), FRAC_PI_2),
    ];
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(rect.center(), top.lerp_to_gamma(bottom, 0.5));
    for (center, start) in corners {
        for step in 0..=3 {
            let angle = start + FRAC_PI_2 * step as f32 / 3.;
            let point = center + egui::vec2(angle.cos(), angle.sin()) * radius;
            let tint = (point.y - rect.top()) / rect.height();
            mesh.colored_vertex(point, top.lerp_to_gamma(bottom, tint));
        }
    }
    let edge_count = mesh.vertices.len() as u32 - 1;
    for edge in 0..edge_count {
        mesh.add_triangle(0, 1 + edge, 1 + (edge + 1) % edge_count);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        let hash = "a".repeat(40);
        Entry {
            commit: Commit {
                hash: hash.clone(),
                parents: vec![],
                message: String::new(),
                author: "First".into(),
                email: "one@example.invalid".into(),
                authored: "2023-05-05T22:17:00-08:00".parse().unwrap(),
                committer: "Second".into(),
                committer_email: "two@example.invalid".into(),
                committed: "2026-11-18T22:59:33+14:00".parse().unwrap(),
            },
            graph: shovel_core::graph::Row {
                node: 0,
                color: 0,
                width: 1,
                is_merge: false,
                stash_link: None,
                lines: vec![],
            },
            references: vec![shovel_core::git::Reference {
                name: "refs/heads/topic".into(),
                hash,
                upstream: None,
            }],
        }
    }

    #[test]
    fn fallback_avatar_uses_the_first_two_words_and_a_color_gradient() {
        assert_eq!(initials("Gleb"), "G");
        assert_eq!(initials("Konstantinos Xenitidis"), "KX");
        assert_eq!(initials("  alice  bob  charlie "), "AB");
        assert_eq!(initials("éva 研究"), "É研");
        assert_eq!(initials(" "), "?");
        let mut entry = entry();
        entry.commit.author = "Alice Bob Charlie".into();
        let ctx = egui::Context::default();
        let out = ctx.run_ui(Default::default(), |ui| {
            let (avatar, _) = ui.allocate_exact_size(egui::vec2(24., 24.), egui::Sense::hover());
            paint_avatar(ui, avatar, &entry.commit, &Palette::current(ui), std::path::Path::new(""));
        });
        assert!(out.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Mesh(mesh) if {
                let top = mesh.vertices.iter().min_by(|a, b| a.pos.y.total_cmp(&b.pos.y)).unwrap().color;
                let bottom = mesh.vertices.iter().max_by(|a, b| a.pos.y.total_cmp(&b.pos.y)).unwrap().color;
                u16::from(top.r()) + u16::from(top.g()) + u16::from(top.b()) <
                    u16::from(bottom.r()) + u16::from(bottom.g()) + u16::from(bottom.b())
            }
        )));
        assert!(out.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "AB"
        )));
        out.drop_without_applying_deltas();
    }

    #[test]
    fn high_contrast_graph_uses_the_visible_foreground_on_selected_and_plain_rows() {
        let entry = entry();
        let ctx = egui::Context::default();
        let mut palette = Palette::for_mode(true);
        palette.high_contrast = true;
        palette.text = Color32::YELLOW;
        palette.history_text = Color32::YELLOW;
        palette.selected = Color32::YELLOW;
        palette.active_selection = Color32::YELLOW;
        palette.selection_text = Color32::BLACK;
        for (selected, expected) in [(false, Color32::YELLOW), (true, Color32::BLACK)] {
            let out = ctx.run_ui(Default::default(), |ui| {
                show(
                    ui,
                    &entry,
                    1,
                    Style {
                        list_focused: false,
                        worktree: std::path::Path::new(""),
                        selected,
                        peer: false,
                        incoming: false,
                        refs_right: false,
                    },
                    &palette,
                );
            });
            assert!(out.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Circle(node) if (node.radius-3.3).abs()<0.01 && node.fill==expected)));
            out.drop_without_applying_deltas();
        }
    }

    #[test]
    fn history_paints_the_local_commit_time_and_exposes_the_original_identities() {
        let entry = entry();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let local = crate::dates::local(&ctx, &entry.commit.committed);
        let authored = crate::dates::local(&ctx, &entry.commit.authored);
        assert_ne!(local, authored);
        let author = author_details(&ctx, &entry.commit);
        let committer = committer_details(&ctx, &entry.commit);
        assert!(author.contains("First") && author.contains("-08:00"));
        assert!(committer.contains("Second") && committer.contains("+14:00"));
        for dark in [false, true] {
            let out = ctx.run_ui(Default::default(), |ui| {
                show(
                    ui,
                    &entry,
                    1,
                    Style {
                        list_focused: false,
                        worktree: std::path::Path::new(""),
                        selected: false,
                        peer: true,
                        incoming: true,
                        refs_right: false,
                    },
                    &Palette::for_mode(dark),
                );
            });
            assert!(
                out.shapes
                    .iter()
                    .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == local))
            );
            let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
            let label = nodes
                .iter()
                .filter_map(|(_, n)| n.label())
                .find(|label| label.contains(&entry.commit.hash))
                .unwrap();
            for expected in [
                &local,
                &strings::text("EmptyCommitSubject"),
                "refs/heads/topic",
                "Waiting to pull",
                "Same author",
            ] {
                assert!(label.contains(expected), "{label}");
            }
            assert!(!label.contains(&authored));
            out.drop_without_applying_deltas();
        }
    }
}
