//! Workspace roles from the C# DarkPalette and LightPalette resources.
use eframe::egui::{self, Color32, Stroke};
#[derive(Clone, Copy)]
pub struct Palette {
    pub high_contrast: bool,
    pub chrome: Color32,
    pub tab_strip: Color32,
    pub canvas: Color32,
    pub sidebar: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub input: Color32,
    pub border: Color32,
    pub text: Color32,
    pub history_text: Color32,
    pub muted: Color32,
    pub disabled: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub active_selection: Color32,
    pub selection_text: Color32,
    pub accent: Color32,
    pub focus: Color32,
    pub author_peer: Color32,
    pub diff_added_background: Color32,
    pub diff_added_text: Color32,
    pub diff_removed_background: Color32,
    pub diff_removed_text: Color32,
}
impl Palette {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                high_contrast: false,
                chrome: Color32::from_rgb(27, 27, 27),
                tab_strip: Color32::from_rgb(23, 23, 23),
                canvas: Color32::from_rgb(34, 34, 34),
                sidebar: Color32::from_rgb(43, 43, 43),
                panel: Color32::from_rgb(50, 50, 50),
                raised: Color32::from_rgb(48, 48, 48),
                input: Color32::from_rgb(39, 39, 39),
                border: Color32::from_rgb(72, 72, 72),
                text: Color32::from_rgb(228, 228, 228),
                history_text: Color32::from_rgb(255, 255, 255),
                muted: Color32::from_rgb(184, 184, 184),
                disabled: Color32::from_rgb(133, 133, 133),
                hover: Color32::from_rgb(59, 59, 59),
                pressed: Color32::from_rgb(69, 69, 69),
                selected: Color32::from_rgb(74, 74, 74),
                active_selection: Color32::from_rgb(36, 101, 170),
                selection_text: Color32::from_rgb(255, 255, 255),
                accent: Color32::from_rgb(240, 128, 96),
                focus: Color32::from_rgb(123, 183, 245),
                author_peer: Color32::from_rgb(56, 54, 34),
                diff_added_background: Color32::from_rgb(33, 58, 43),
                diff_added_text: Color32::from_rgb(184, 230, 196),
                diff_removed_background: Color32::from_rgb(70, 37, 42),
                diff_removed_text: Color32::from_rgb(255, 184, 189),
            }
        } else {
            Self {
                high_contrast: false,
                chrome: Color32::from_rgb(243, 243, 243),
                tab_strip: Color32::from_rgb(232, 232, 232),
                canvas: Color32::from_rgb(255, 255, 255),
                sidebar: Color32::from_rgb(236, 236, 236),
                panel: Color32::from_rgb(247, 247, 247),
                raised: Color32::from_rgb(228, 228, 228),
                input: Color32::from_rgb(255, 255, 255),
                border: Color32::from_rgb(176, 176, 176),
                text: Color32::from_rgb(32, 32, 32),
                history_text: Color32::from_rgb(32, 32, 32),
                muted: Color32::from_rgb(85, 85, 85),
                disabled: Color32::from_rgb(119, 119, 119),
                hover: Color32::from_rgb(70, 103, 134),
                pressed: Color32::from_rgb(54, 86, 115),
                selected: Color32::from_rgb(73, 102, 131),
                active_selection: Color32::from_rgb(36, 101, 170),
                selection_text: Color32::from_rgb(255, 255, 255),
                accent: Color32::from_rgb(160, 57, 29),
                focus: Color32::from_rgb(36, 95, 156),
                author_peer: Color32::from_rgb(255, 243, 191),
                diff_added_background: Color32::from_rgb(224, 241, 228),
                diff_added_text: Color32::from_rgb(32, 92, 50),
                diff_removed_background: Color32::from_rgb(249, 226, 229),
                diff_removed_text: Color32::from_rgb(139, 36, 51),
            }
        }
    }
}

impl Palette {
    pub fn change_status(code: char, on_dark: bool) -> Color32 {
        let (light, dark) = match code {
            'D' => (0xFF8A8A, 0xA42532),
            'A' => (0x8DDD9E, 0x21703C),
            'M' => (0xF6DD72, 0x745600),
            'R' => (0x89D8FF, 0x12628D),
            'C' => (0xDBC4FF, 0x634098),
            'T' => (0xFFBA82, 0x8B4619),
            'U' => (0xFFB5DB, 0x92275E),
            _ => (0xBFC5CD, 0x515762),
        };
        let rgb = if on_dark { light } else { dark };
        Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    pub fn current(ui: &egui::Ui) -> Self {
        ui.ctx()
            .data(|d| d.get_temp(egui::Id::unique("workspace-palette")))
            .unwrap_or_else(|| Self::for_mode(ui.visuals().dark_mode))
    }
    #[cfg(windows)]
    fn with_system_contrast(mut self) -> Self {
        use windows_sys::Win32::{
            Graphics::Gdi::*,
            UI::{
                Accessibility::HIGHCONTRASTW,
                WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW},
            },
        };
        let mut hc = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        // Windows fills only the caller-owned structure. We do not dereference its scheme pointer.
        if unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, (&raw mut hc).cast(), 0) }
            == 0
            || hc.dwFlags & 1 == 0
        {
            return self;
        }
        let system = |index| {
            let rgb = unsafe { GetSysColor(index) };
            Color32::from_rgb(rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8)
        };
        let bg = system(COLOR_WINDOW);
        let fg = system(COLOR_WINDOWTEXT);
        let highlight = system(COLOR_HIGHLIGHT);
        let on_highlight = system(COLOR_HIGHLIGHTTEXT);
        self.high_contrast = true;
        self.chrome = bg;
        self.tab_strip = bg;
        self.canvas = bg;
        self.sidebar = bg;
        self.panel = bg;
        self.raised = bg;
        self.input = bg;
        self.author_peer = bg;
        self.text = fg;
        self.history_text = fg;
        self.muted = fg;
        self.focus = fg;
        self.border = fg;
        self.accent = fg;
        self.hover = highlight;
        self.pressed = highlight;
        self.selected = highlight;
        self.active_selection = highlight;
        self.selection_text = on_highlight;
        self.disabled = system(COLOR_GRAYTEXT);
        self.diff_added_background = bg;
        self.diff_removed_background = bg;
        self.diff_added_text = fg;
        self.diff_removed_text = fg;
        self
    }
}
pub fn apply(ctx: &egui::Context, light: bool) {
    let p = Palette::for_mode(!light);
    #[cfg(windows)]
    let p = p.with_system_contrast();
    apply_palette(ctx, light, p);
}

pub fn dialog_checkbox(ui: &mut egui::Ui, checked: &mut bool, label: String) -> egui::Response {
    let p = Palette::current(ui);
    let enabled = ui.is_enabled();
    let color = if enabled { p.text } else { p.disabled };
    let galley = ui.painter().layout(
        label.clone(),
        egui::TextStyle::Body.resolve(ui.style()),
        color,
        (ui.available_width() - 29.).max(24.),
    );
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 29., (galley.size().y + 6.).max(27.)),
        egui::Sense::click(),
    );
    if response.clicked() {
        response.request_focus();
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, *checked, &label)
    });
    let painter = ui.painter();
    let square = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 10., rect.center().y),
        egui::Vec2::splat(19.),
    );
    let (fill, tick) = if !enabled {
        (p.raised, p.disabled)
    } else if p.high_contrast {
        (p.active_selection, p.selection_text)
    } else {
        (
            Palette::for_mode(true).accent,
            Color32::from_rgb(32, 32, 32),
        )
    };
    painter.rect(
        square,
        3.,
        if *checked { fill } else { p.input },
        Stroke::new(
            1.,
            if enabled && (*checked || response.hovered()) {
                p.accent
            } else {
                p.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    if *checked {
        let center = square.center();
        painter.add(egui::Shape::line(
            [(-5., 0.), (-1.5, 3.5), (5.5, -4.)]
                .map(|(x, y)| center + egui::vec2(x, y))
                .to_vec(),
            Stroke::new(2.3, tick),
        ));
    }
    if response.has_focus() {
        painter.rect_stroke(
            square.expand(2.),
            4.,
            Stroke::new(1.5, p.focus),
            egui::StrokeKind::Outside,
        );
    }
    painter.galley(
        egui::pos2(rect.left() + 29., rect.center().y - galley.size().y / 2.),
        galley,
        color,
    );
    response
}

pub fn show_closable_window(
    ctx: &egui::Context,
    window: egui::Window<'_>,
    title: &str,
    open: &mut bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    if !*open {
        return;
    }
    window.title_bar(false).show(ctx, |ui| {
        let close_width = 28.;
        let palette = Palette::current(ui);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - close_width - ui.spacing().item_spacing.x).max(0.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 28.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(title).heading()).truncate());
                },
            );
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(close_width, 28.), egui::Sense::click());
            let label = crate::strings::text("WindowClose");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label)
            });
            let response = response.on_hover_text(label);
            let painter = ui.painter();
            let foreground = if ui.is_enabled()
                && (response.hovered() || response.is_pointer_button_down_on())
            {
                let (fill, foreground) = if palette.high_contrast {
                    (palette.active_selection, palette.selection_text)
                } else if response.is_pointer_button_down_on() {
                    (Color32::from_rgb(154, 32, 25), Color32::WHITE)
                } else {
                    (Color32::from_rgb(196, 43, 28), Color32::WHITE)
                };
                painter.rect_filled(rect, 4., fill);
                foreground
            } else if ui.is_enabled() {
                palette.text
            } else {
                palette.disabled
            };
            if response.has_focus() {
                painter.rect_stroke(
                    rect,
                    4.,
                    Stroke::new(1.5, palette.focus),
                    egui::StrokeKind::Inside,
                );
            }
            let center = rect.center();
            for direction in [egui::vec2(4.5, 4.5), egui::vec2(4.5, -4.5)] {
                painter.line_segment(
                    [center - direction, center + direction],
                    Stroke::new(1.5, foreground),
                );
            }
            if response.clicked() {
                *open = false;
            }
        });
        ui.separator();
        body(ui);
    });
}

fn apply_palette(ctx: &egui::Context, light: bool, p: Palette) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("workspace-palette"), p));
    ctx.set_theme(if p.high_contrast {
        egui::ThemePreference::System
    } else if light {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    });
    let light = ctx.theme() == egui::Theme::Light;
    let mut v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.panel_fill = p.canvas;
    v.window_fill = p.panel;
    v.faint_bg_color = p.sidebar;
    v.extreme_bg_color = p.input;
    v.hyperlink_color = p.focus;
    v.error_fg_color = p.diff_removed_text;
    v.window_stroke = Stroke::new(1., p.border);
    v.window_corner_radius = 5.into();
    v.menu_corner_radius = 5.into();
    v.selection.bg_fill = p.active_selection;
    v.selection.stroke = Stroke::new(1., p.selection_text);
    for (widget, fill, foreground) in [
        (&mut v.widgets.noninteractive, p.canvas, p.text),
        (&mut v.widgets.inactive, p.raised, p.text),
        (&mut v.widgets.hovered, p.hover, p.selection_text),
        (
            &mut v.widgets.active,
            if light && !p.high_contrast {
                p.raised
            } else {
                p.pressed
            },
            if light && !p.high_contrast {
                p.text
            } else {
                p.selection_text
            },
        ),
        (&mut v.widgets.open, p.selected, p.selection_text),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1., p.border);
        widget.fg_stroke = Stroke::new(1., foreground);
        widget.corner_radius = 4.into();
        widget.expansion = 0.;
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.button_padding = egui::vec2(8., 3.);
        s.spacing.item_spacing = egui::vec2(6., 3.);
    });
}

pub fn pane(fill: Color32, horizontal: i8, vertical: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(horizontal, vertical))
}

pub fn lower_tab(ui: &mut egui::Ui, active: bool, label: String) -> egui::Response {
    let p = Palette::current(ui);
    let label = egui::RichText::new(label).color(if active { p.accent } else { p.text });
    let response = ui.add(
        egui::Button::new(label)
            .selected(active)
            .fill(p.sidebar)
            .min_size(egui::vec2(0., 26.)),
    );
    if active {
        ui.painter().line_segment(
            [response.rect.left_bottom(), response.rect.right_bottom()],
            Stroke::new(2., p.accent),
        );
    }
    response
}

pub const TOOLBAR_HEIGHT: f32 = 38.;

/// A single keyboard/accessibility target for the icon and its caption.
pub fn action(ui: &mut egui::Ui, key: &str, enabled: bool) -> egui::Response {
    action_with_count(ui, key, enabled, None)
}
pub fn action_with_count(
    ui: &mut egui::Ui,
    key: &str,
    enabled: bool,
    count: Option<(&str, &str)>,
) -> egui::Response {
    let label = crate::strings::text(key);
    ui.add_enabled(enabled, |ui: &mut egui::Ui| {
        let font = egui::FontId::proportional(12.);
        let galley = ui
            .painter()
            .layout_no_wrap(label.clone(), font, ui.visuals().text_color());
        let count_galley = count.map(|(value, _)| {
            ui.painter().layout_no_wrap(
                value.to_owned(),
                egui::FontId::proportional(10.),
                ui.visuals().text_color(),
            )
        });
        let count_width = count_galley.as_ref().map_or(0., |g| g.size().x + 16.);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(
                (galley.size().x + 12.).max(43.).max(count_width),
                TOOLBAR_HEIGHT,
            ),
            egui::Sense::click(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
        let p = Palette::current(ui);
        let state = ui.style().interact(&response);
        let color = if ui.is_enabled() {
            state.fg_stroke.color
        } else {
            p.disabled
        };
        if response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 4., state.bg_fill);
        }
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect, 4., Stroke::new(1., p.focus), egui::StrokeKind::Inside);
        }
        let origin = egui::pos2(rect.center().x - 10., rect.top() + 1.);
        let paths: &[&[(f32, f32)]] = match key {
            "OpenRepository" => &[&[
                (1., 4.),
                (7., 4.),
                (9., 6.),
                (19., 6.),
                (19., 17.),
                (1., 17.),
                (1., 4.),
            ]],
            "Commit" => &[&[(2., 10.), (7., 15.), (18., 3.)]],
            "BranchToolbar" => &[
                &[(5., 6.), (5., 15.)],
                &[(5., 13.), (8., 10.), (12., 10.), (15., 8.), (15., 6.)],
            ],
            "Stashes" => &[
                &[(2., 7.), (10., 3.), (18., 7.), (10., 11.), (2., 7.)],
                &[(2., 11.), (10., 15.), (18., 11.)],
                &[(2., 15.), (10., 19.), (18., 15.)],
            ],
            _ => &[],
        };
        if matches!(key, "Fetch" | "Pull" | "Push") {
            network_arrow(ui.painter(), origin, color, key == "Fetch", key == "Push");
        }
        if key == "BranchToolbar" {
            for (x, y) in [(5., 3.5), (5., 17.5), (15., 3.5)] {
                ui.painter().circle_stroke(origin + egui::vec2(x,y), 2.4, Stroke::new(1.5, color));
            }
        }
        for path in paths {
            let points = path
                .iter()
                .map(|(x, y)| origin + egui::vec2(*x, *y))
                .collect();
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(1.65, color)));
        }
        if let (Some((value, name)), Some(badge)) = (count, count_galley) {
            let size = egui::vec2((badge.size().x + 8.).max(15.), 15.);
            let badge_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - size.x - 1., rect.top() + 1.),
                size,
            );
            ui.painter().rect_filled(badge_rect, 7., p.raised);
            ui.painter().rect_stroke(
                badge_rect,
                7.,
                Stroke::new(1., p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().galley_with_override_text_color(
                badge_rect.center() - badge.size() / 2.,
                badge,
                color,
            );
            ui.interact(badge_rect, response.id.with("count"), egui::Sense::hover())
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Label,
                        ui.is_enabled(),
                        format!("{}: {value}", crate::strings::text(name)),
                    )
                });
        }
        ui.painter().galley_with_override_text_color(
            egui::pos2(
                rect.center().x - galley.size().x / 2.,
                rect.bottom() - galley.size().y - 1.,
            ),
            galley,
            color,
        );
        response
    })
}

fn network_arrow(
    painter: &egui::Painter,
    origin: egui::Pos2,
    color: Color32,
    hollow: bool,
    upwards: bool,
) {
    // Two edges of the same curved shaft, closing at the tail, and a shared arrowhead.
    let outer = [(19., 2.), (13.7, 2.3), (9.2, 3.8), (5.7, 6.7), (4., 11.3)];
    let inner = [(19., 2.), (15., 4.), (12.5, 6.6), (11., 9.), (10.5, 11.3)];
    let point = |(x, y)| {
        origin
            + if upwards {
                egui::vec2(20. - x, 20. - y)
            } else {
                egui::vec2(x, y)
            }
    };
    let left = (1.5, 11.3);
    let tip = (7.3, 18.);
    let right = (13., 11.3);
    if hollow {
        let outline = outer
            .into_iter()
            .chain([left, tip, right])
            .chain(inner.into_iter().rev());
        painter.add(egui::Shape::line(
            outline.map(point).collect(),
            Stroke::new(1.1, color),
        ));
    } else {
        let mut mesh = egui::Mesh::default();
        for (a, b) in outer.into_iter().zip(inner) {
            mesh.colored_vertex(point(a), color);
            mesh.colored_vertex(point(b), color);
        }
        for i in (0..8).step_by(2) {
            mesh.add_triangle(i, i + 2, i + 3);
            mesh.add_triangle(i, i + 3, i + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
        painter.add(egui::Shape::convex_polygon(
            [left, tip, right].map(point).to_vec(),
            color,
            Stroke::NONE,
        ));
    }
}

/// The chevron is drawn from geometry so it survives system font substitution.
pub fn options_button(ui: &mut egui::Ui, label: String, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .min_size(egui::vec2(16., TOOLBAR_HEIGHT))
            .frame_when_inactive(false),
    );
    let p = Palette::current(ui);
    let center = response.rect.center();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-3.5, -1.5),
            center + egui::vec2(3.5, -1.5),
            center + egui::vec2(0., 2.5),
        ],
        if enabled { p.text } else { p.disabled },
        Stroke::NONE,
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &label));
    response.on_hover_text(label)
}
pub fn reference_row(ui: &mut egui::Ui, selected: bool, label: &str, kind: &str) -> egui::Response {
    let p = Palette::current(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 3., if selected { p.selected } else { p.hover });
    }
    let center = egui::pos2(rect.left() + 9., rect.center().y);
    let fg = if selected { p.selection_text } else { p.text };
    let stroke = Stroke::new(1.2, if kind == "Stashes" && !p.high_contrast {
        Color32::from_gray(if ui.visuals().dark_mode { 168 } else { 102 })
    } else if selected { fg } else { p.muted });
    if kind == "Tags" {
        let points = vec![
            center + egui::vec2(-5., -4.),
            center + egui::vec2(1., -4.),
            center + egui::vec2(5., 0.),
            center + egui::vec2(1., 4.),
            center + egui::vec2(-5., 4.),
        ];
        ui.painter().add(egui::Shape::closed_line(points, stroke));
    } else {
        ui.painter().line_segment(
            [center + egui::vec2(0., -5.), center + egui::vec2(0., 5.)],
            stroke,
        );
        for dy in [-4., 4.] {
            ui.painter()
                .circle_stroke(center + egui::vec2(0., dy), 2., stroke);
        }
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(13.),
        fg,
    );
    job.wrap.max_width = (rect.width() - 23.).max(1.);
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(rect.left() + 22., rect.center().y - galley.size().y / 2.),
            galley,
            fg,
        );
    if response.clicked() {
        response.request_focus();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    response
}

pub fn options_menu(
    ui: &mut egui::Ui,
    label: String,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let button = egui::Button::new("")
        .min_size(egui::vec2(16., 40.))
        .frame_when_inactive(false);
    let (response, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, content);
    let p = Palette::current(ui);
    let center = response.rect.center();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-3.5, -1.5),
            center + egui::vec2(3.5, -1.5),
            center + egui::vec2(0., 2.5),
        ],
        if ui.is_enabled() { p.text } else { p.disabled },
        Stroke::NONE,
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    response.on_hover_text(label)
}

pub fn install_windows_font(ctx: &egui::Context) {
    let Some(system) = std::env::var_os("SystemRoot") else {
        return;
    };
    let path = std::path::PathBuf::from(system).join("Fonts");
    let mut definitions = egui::FontDefinitions::default();
    if let Some(data) = read_windows_font(&path.join("segoeui.ttf")) {
        definitions.font_data.insert("Segoe UI".into(), data.into());
        definitions
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Segoe UI".into());
    }
    if let Some(data) = read_windows_font(&path.join("segoeuib.ttf")) {
        definitions
            .font_data
            .insert("Segoe UI Bold".into(), data.into());
        let mut family = definitions.families[&egui::FontFamily::Proportional].clone();
        family.insert(0, "Segoe UI Bold".into());
        definitions
            .families
            .insert(egui::FontFamily::Name("author-avatar-bold".into()), family);
    }
    ctx.set_fonts(definitions);
    ctx.request_repaint();
}

fn read_windows_font(path: &std::path::Path) -> Option<egui::FontData> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    const MAX: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take(MAX + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX && bytes.get(..4) == Some(&[0, 1, 0, 0]))
        .then(|| egui::FontData::from_owned(bytes))
}

pub fn avatar_bold_font(ctx: &egui::Context, size: f32) -> Option<egui::FontId> {
    let family = egui::FontFamily::Name("author-avatar-bold".into());
    ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family))
        .then_some(egui::FontId::new(size, family))
}

#[cfg(test)]
mod native_caption_tests {
    use super::*;

    #[test]
    fn app_dialog_close_highlights_the_hit_target_and_keeps_disabled_close_inert() {
        use egui::{Event, PointerButton, Pos2, RawInput, Rect, vec2};
        for light in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            apply_palette(&ctx, light, Palette::for_mode(!light));
            let mut open = true;
            let frame = |open: &mut bool, enabled, events| {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 600.))),
                        events,
                        ..Default::default()
                    },
                    |_| {
                        let window = egui::Window::new("Dialog fixture")
                            .resizable(false)
                            .fade_in(false)
                            .fixed_pos(egui::pos2(130., 145.))
                            .enabled(enabled);
                        show_closable_window(&ctx, window, "Dialog fixture", open, |ui| {
                            ui.label("Dialog contents");
                        });
                    },
                )
            };
            frame(&mut open, true, vec![]).drop_without_applying_deltas();
            let output = frame(&mut open, true, vec![]);
            let close = crate::strings::text("WindowClose");
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(&close))
                        .then(|| node.bounds())
                        .flatten()
                })
                .unwrap();
            let point = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.) as f32,
                ((bounds.y0 + bounds.y1) / 2.) as f32,
            );
            let highlighted = |output: &egui::FullOutput| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.rect.contains(point)
                        && rect.fill == Color32::from_rgb(196, 43, 28))
                })
            };
            assert!(!highlighted(&output));
            output.drop_without_applying_deltas();
            let output = frame(&mut open, true, vec![Event::PointerMoved(point)]);
            assert!(highlighted(&output));
            output.drop_without_applying_deltas();
            let click = |pressed| Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let output = frame(&mut open, false, vec![]);
            assert!(!highlighted(&output));
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(&mut open, false, vec![click(pressed)]).drop_without_applying_deltas();
            }
            assert!(open);
            frame(&mut open, true, vec![]).drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(&mut open, true, vec![click(pressed)]).drop_without_applying_deltas();
            }
            assert!(!open);
        }
    }

    #[test]
    fn native_caption_theme_follows_user_appearance_and_system_contrast_in_each_window() {
        use egui::{
            SystemTheme::{Dark, Light, SystemDefault},
            ViewportCommand, ViewportId,
        };
        let ctx = egui::Context::default();
        let root = ViewportId::ROOT;
        let child = ViewportId::from_hash_of("theme-dialog");
        let draw = |id| {
            let mut input = egui::RawInput {
                viewport_id: id,
                ..Default::default()
            };
            input.viewports.entry(root).or_default();
            input.viewports.entry(child).or_default().parent = Some(root);
            let output = ctx.run_ui(input, |_| {});
            let commands: Vec<_> = output
                .viewport_output
                .get(&id)
                .into_iter()
                .flat_map(|v| &v.commands)
                .filter_map(|cmd| match cmd {
                    ViewportCommand::SetTheme(theme) => Some(*theme),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            commands
        };
        for (light, contrast, theme) in [
            (false, false, Dark),
            (true, false, Light),
            (true, true, SystemDefault),
            (false, false, Dark),
        ] {
            let mut p = Palette::for_mode(!light);
            p.high_contrast = contrast;
            apply_palette(&ctx, light, p);
            for id in [root, child] {
                assert_eq!(draw(id), [theme]);
                assert!(draw(id).is_empty());
            }
        }
    }
}
