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
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("workspace-palette"), p));
    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
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

/// A single keyboard/accessibility target for the icon and its caption.
pub fn action(ui: &mut egui::Ui, key: &str, enabled: bool) -> egui::Response {
    let label = crate::strings::text(key);
    ui.add_enabled(enabled, |ui: &mut egui::Ui| {
        let font = egui::FontId::proportional(12.);
        let galley = ui
            .painter()
            .layout_no_wrap(label.clone(), font, ui.visuals().text_color());
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2((galley.size().x + 12.).max(43.), 44.),
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
        let origin = egui::pos2(rect.center().x - 10., rect.top() + 3.);
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
            "Fetch" => &[
                &[(10., 1.), (10., 14.)],
                &[(5., 9.), (10., 14.), (15., 9.)],
                &[(2., 16.), (2., 19.), (18., 19.), (18., 16.)],
            ],
            "Pull" => &[
                &[(17., 2.), (10., 3.), (6., 6.), (5., 10.), (5., 16.)],
                &[(1., 11.), (5., 16.), (10., 11.)],
            ],
            "Push" => &[
                &[(3., 18.), (10., 17.), (14., 14.), (15., 10.), (15., 4.)],
                &[(10., 9.), (15., 4.), (19., 9.)],
            ],
            "Commit" => &[&[(2., 10.), (7., 15.), (18., 3.)]],
            "BranchToolbar" => &[
                &[(5., 2.), (5., 18.)],
                &[(5., 12.), (8., 9.), (13., 9.), (15., 7.), (15., 3.)],
                &[(11., 4.), (19., 4.)],
                &[(15., 0.), (15., 8.)],
            ],
            "Stashes" => &[
                &[(2., 7.), (10., 3.), (18., 7.), (10., 11.), (2., 7.)],
                &[(2., 11.), (10., 15.), (18., 11.)],
                &[(2., 15.), (10., 19.), (18., 15.)],
            ],
            _ => &[],
        };
        for path in paths {
            let points = path
                .iter()
                .map(|(x, y)| origin + egui::vec2(*x, *y))
                .collect();
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(1.65, color)));
        }
        ui.painter().galley_with_override_text_color(
            egui::pos2(
                rect.center().x - galley.size().x / 2.,
                rect.bottom() - galley.size().y - 3.,
            ),
            galley,
            color,
        );
        response
    })
}

/// The chevron is drawn from geometry so it survives system font substitution.
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
    use std::io::Read;
    let Some(system) = std::env::var_os("SystemRoot") else {
        return;
    };
    let path = std::path::PathBuf::from(system)
        .join("Fonts")
        .join("segoeui.ttf");
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    const MAX: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    if file.take(MAX + 1).read_to_end(&mut bytes).is_err()
        || bytes.len() as u64 > MAX
        || bytes.get(..4) != Some(&[0, 1, 0, 0])
    {
        return;
    }
    ctx.add_font(egui::epaint::text::FontInsert {
        name: "Segoe UI".into(),
        data: egui::FontData::from_owned(bytes),
        families: vec![egui::epaint::text::InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: egui::epaint::text::FontPriority::Highest,
        }],
    });
    ctx.request_repaint();
}
