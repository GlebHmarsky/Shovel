use crate::strings::text;
use crate::workspace_style::Palette;
use eframe::egui;
use serde_json::json;
#[cfg(test)]
#[path = "settings_ui_tests.rs"]
mod tests;
use shovel_core::{
    conflicts::ToolSettings,
    settings::{AvatarPreferences, AvatarProvider, Settings},
};
pub const KEYS: &[&str] = &[
    "Appearance",
    "UiScale",
    "CommitInWorkspace",
    "HistoryRefsOnRight",
    "Avatars",
    "DiffSideBySide",
    "DiffFontSize",
    "AutoFetchEnabled",
    "AutoFetchIntervalSeconds",
    "AutoFetchCadenceRevision",
    "RiderExecutable",
    "CodeExecutable",
    "BashExecutable",
    "VisualStudioExecutable",
    "ExternalMergeTool",
    "UpdateChannel",
    "CheckUpdatesPeriodically",
];
pub struct Dialog {
    pub identity: crate::git_identity::Editor,
    pub values: Settings,
    page: Page,
    pub saving: bool,
    pub error: Option<String>,
}
impl Dialog {
    pub fn new(settings: &Settings) -> Self {
        Self {
            identity: Default::default(),
            values: settings.clone(),
            page: Page::Appearance,
            saving: false,
            error: None,
        }
    }
    pub fn merge(&self, target: &mut Settings) {
        for key in KEYS {
            if let Some(v) = self.values.get(key) {
                target.set(*key, v.clone());
            }
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: Option<std::sync::Arc<shovel_core::git::Git>>,
    ) -> (bool, bool) {
        self.identity.poll();
        let mut result = (false, false);
        let height = (ctx.content_rect().height() - 95.).clamp(100., 465.);
        let dismissible = !self.saving && !self.identity.busy;
        let (_, dismissed) = crate::workspace_style::show_dialog(
            ctx,
            egui::Id::unique("settings"),
            &text("SettingsTitle"),
            710.,
            true,
            dismissible,
            |ui| {
                ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                    ui.set_height(height);
                    let p = Palette::current(ui);
                    let nav_width = (ui.available_width() * 0.26).clamp(88., 148.);
                    egui::Panel::bottom("settings-footer")
                        .resizable(false)
                        .frame(egui::Frame::new().fill(p.panel).inner_margin(9))
                        .show(ui, |ui| result = self.footer(ui));
                    let active_tab = egui::Panel::left("settings-categories")
                        .exact_size(nav_width)
                        .resizable(false)
                        .frame(egui::Frame::new().fill(p.sidebar).inner_margin(5))
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.saving, |ui| {
                                ui.add_space(5.);
                                egui::ScrollArea::vertical()
                                    .id_salt("settings-navigation")
                                    .show(ui, |ui| self.category_tabs(ui, &p))
                                    .inner
                            })
                            .inner
                        })
                        .inner;
                    let margin = if ui.available_width() < 390. { 10 } else { 18 };
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(p.panel).inner_margin(margin))
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.saving, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt(("settings-page", self.page))
                                    .show(ui, |ui| {
                                        let panel = ui.unique_id();
                                        ctx.accesskit_node_builder(panel, |node| {
                                            node.set_role(egui::Role::TabPanel);
                                            node.set_labelled_by(vec![active_tab.accesskit_id()]);
                                        });
                                        ctx.accesskit_node_builder(active_tab, |node| {
                                            node.set_controls(vec![panel.accesskit_id()]);
                                        });
                                        ui.set_min_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 6.;
                                        ui.heading(text(self.page.key()));
                                        ui.add_space(5.);
                                        match self.page {
                                            Page::Appearance => appearance(ui, &mut self.values),
                                            Page::History => history(ui, &mut self.values),
                                            Page::Diff => diff(ui, &mut self.values),
                                            Page::Git => fetch(ui, &mut self.values),
                                            Page::Identity => self.identity.show(ui, git),
                                            Page::Tools => tools(ui, &mut self.values),
                                            Page::Updates => updates(ui, &mut self.values),
                                        }
                                    });
                            });
                        });
                });
            },
        );
        result.1 |= dismissed;
        result
    }

    fn category_tabs(&mut self, ui: &mut egui::Ui, palette: &Palette) -> egui::Id {
        let ctx = ui.ctx();
        ctx.accesskit_node_builder(ui.unique_id(), |node| {
            node.set_role(egui::Role::TabList);
            node.set_label(text("SettingsCategories"));
            node.set_orientation(egui::accesskit::Orientation::Vertical);
        });
        let tabs = Page::ALL.map(|page| page.tab(ui, self.page == page, palette));
        let clicked = tabs.iter().position(egui::Response::clicked);
        let mut next = clicked;
        if clicked.is_none()
            && ui.is_enabled()
            && let Some(focused) = tabs.iter().position(egui::Response::has_focus)
        {
            next = ui.input_mut(|input| {
                use egui::Key;
                let last = tabs.len() - 1;
                if input.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
                    Some(focused.checked_sub(1).unwrap_or(last))
                } else if input.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
                    Some((focused + 1) % tabs.len())
                } else if input.consume_key(egui::Modifiers::NONE, Key::Home) {
                    Some(0)
                } else if input.consume_key(egui::Modifiers::NONE, Key::End) {
                    Some(last)
                } else {
                    None
                }
            });
            if next.is_some() {
                // Replace egui's spatial focus move with movement within the tab list.
                ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            }
        }
        if let Some(index) = next {
            self.page = Page::ALL[index];
            tabs[index].request_focus();
            tabs[index].scroll_to_me(None);
            ui.ctx().request_repaint();
        }
        let active = Page::ALL
            .iter()
            .position(|page| *page == self.page)
            .expect("every settings page has a tab");
        for (index, tab) in tabs.iter().enumerate() {
            let selected = index == active;
            ui.ctx().accesskit_node_builder(tab.id, |node| {
                node.set_role(egui::Role::Tab);
                node.clear_toggled();
                node.set_selected(selected);
                if selected {
                    node.add_action(egui::accesskit::Action::Focus);
                } else {
                    node.remove_action(egui::accesskit::Action::Focus);
                }
            });
        }
        tabs[active].id
    }

    fn footer(&mut self, ui: &mut egui::Ui) -> (bool, bool) {
        let p = Palette::current(ui);
        let mut result = (false, false);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!self.saving && !self.identity.busy, |ui| {
                let save =
                    egui::Button::new(egui::RichText::new(text("SettingsSave")).color(p.text))
                        .fill(p.raised)
                        .stroke((1., p.accent))
                        .min_size(egui::vec2(76., 28.));
                if ui.add(save).clicked() {
                    match self.validate() {
                        Ok(()) => {
                            self.values.normalize();
                            result.0 = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                result.1 = ui
                    .add(egui::Button::new(text("Cancel")).min_size(egui::vec2(70., 28.)))
                    .clicked();
            });
            if self.saving {
                ui.label(text("SettingsSaving"));
            }
        });
        result
    }

    fn validate(&self) -> Result<(), String> {
        if !AvatarPreferences::from_settings(&self.values).valid() {
            return Err(text("SettingsAvatarInvalid"));
        }
        for key in KEYS.iter().filter(|key| key.ends_with("Executable")) {
            let path = self.values.get(key).and_then(|v| v.as_str()).unwrap_or("");
            if !path.is_empty()
                && (!std::path::Path::new(path).is_absolute()
                    || !path.to_ascii_lowercase().ends_with(".exe")
                    || path.chars().any(char::is_control))
            {
                return Err(text("SettingsToolPathInvalid"));
            }
        }
        merge_tool(&self.values)
            .validate()
            .map_err(|e| text(&e.to_string()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Page {
    Appearance,
    History,
    Diff,
    Git,
    Identity,
    Tools,
    Updates,
}
impl Page {
    fn tab(self, ui: &mut egui::Ui, current: bool, p: &Palette) -> egui::Response {
        let label = egui::RichText::new(text(self.key()));
        let label = if current && !p.high_contrast {
            label.strong().color(p.accent)
        } else {
            label
        };
        let button = egui::Button::selectable(current, (label, egui::Atom::grow()));
        let button = if current && !p.high_contrast {
            button
                .fill(p.sidebar.lerp_to_gamma(p.accent, 0.08))
                .stroke(egui::Stroke::NONE)
        } else {
            button
        };
        let button = if current {
            button
        } else {
            button.sense(egui::Sense::CLICK)
        };
        let tab = ui.add(button.min_size(egui::vec2(ui.available_width(), 30.)));
        if current {
            let edge = egui::Rect::from_min_size(
                tab.rect.left_top() + egui::vec2(0., 4.),
                egui::vec2(3., tab.rect.height() - 8.),
            );
            ui.painter().rect_filled(edge, 1., p.accent);
        }
        tab
    }

    const ALL: [Self; 7] = [
        Self::Appearance,
        Self::History,
        Self::Diff,
        Self::Git,
        Self::Identity,
        Self::Tools,
        Self::Updates,
    ];
    fn key(self) -> &'static str {
        match self {
            Self::Appearance => "SettingsPageAppearance",
            Self::History => "SettingsPageHistory",
            Self::Diff => "SettingsPageDiff",
            Self::Git => "SettingsPageGit",
            Self::Identity => "SettingsPageIdentity",
            Self::Tools => "SettingsPageTools",
            Self::Updates => "SettingsPageUpdates",
        }
    }
}

pub(super) fn section(ui: &mut egui::Ui, key: &str) {
    ui.add_space(4.);
    ui.strong(text(key));
    ui.separator();
}

pub(super) fn note(ui: &mut egui::Ui, key: &str) {
    ui.label(
        egui::RichText::new(text(key))
            .size(12.)
            .color(Palette::current(ui).muted),
    );
}

/// Keep a common label column until the control needs that space for itself.
pub(super) fn form_row(
    ui: &mut egui::Ui,
    key: &str,
    control: impl FnOnce(&mut egui::Ui, egui::Id),
) {
    ui.push_id(key, |ui| {
        let width = ui.available_width();
        if width < 335. {
            let label = ui.label(text(key));
            ui.vertical(|ui| control(ui, label.id));
        } else {
            ui.horizontal_top(|ui| {
                let label_width = (width * 0.31).clamp(98., 155.);
                let label = ui
                    .allocate_ui_with_layout(
                        egui::vec2(label_width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(label_width);
                            ui.label(text(key))
                        },
                    )
                    .inner;
                ui.vertical(|ui| control(ui, label.id));
            });
        }
    });
}

fn boolean(ui: &mut egui::Ui, settings: &mut Settings, key: &str, label: &str) -> bool {
    let mut value = settings.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    ui.checkbox(&mut value, text(label));
    settings.set(key, json!(value));
    value
}

fn appearance(ui: &mut egui::Ui, values: &mut Settings) {
    section(ui, "SettingsDisplayGroup");
    let mut light = values.get("Appearance").and_then(|v| v.as_u64()) == Some(1);
    form_row(ui, "SettingsTheme", |ui, _| {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut light, false, text("WorkspaceDark"));
            ui.radio_value(&mut light, true, text("WorkspaceLight"));
        });
    });
    values.set("Appearance", json!(u64::from(light)));
    let mut scale = (values.ui_scale() * 100.).round();
    form_row(ui, "SettingsScale", |ui, label| {
        ui.add(
            egui::Slider::new(&mut scale, 70. ..=200.)
                .step_by(10.)
                .trailing_fill(true)
                .suffix("%"),
        )
        .labelled_by(label);
    });
    values.set("UiScale", json!(scale / 100.));
    section(ui, "SettingsWorkspaceGroup");
    form_row(ui, "SettingsCommitPlacement", |ui, _| {
        let mut inline = values.commit_in_workspace();
        ui.radio_value(&mut inline, false, text("SettingsCommitWindow"));
        ui.radio_value(&mut inline, true, text("SettingsCommitSidebar"));
        values.set("CommitInWorkspace", json!(inline));
    });
    note(ui, "SettingsLayoutHelp");
}

fn history(ui: &mut egui::Ui, values: &mut Settings) {
    section(ui, "HistoryRefsSettingsTitle");
    boolean(ui, values, "HistoryRefsOnRight", "RustRefsRight");
    section(ui, "SettingsAvatarSource");
    let mut avatars = AvatarPreferences::from_settings(values);
    form_row(ui, "SettingsProvider", |ui, _| {
        for (provider, key) in [
            (AvatarProvider::Repository, "SettingsAvatarRepository"),
            (AvatarProvider::Gravatar, "SettingsAvatarGravatar"),
            (AvatarProvider::Initials, "SettingsAvatarInitials"),
        ] {
            ui.radio_value(&mut avatars.provider, provider, text(key));
        }
    });
    form_row(ui, "SettingsGiteaServer", |ui, label| {
        ui.add(
            egui::TextEdit::singleline(&mut avatars.site)
                .desired_width(f32::INFINITY)
                .char_limit(2048),
        )
        .labelled_by(label);
    });
    note(ui, "SettingsAvatarHelp");
    avatars.save(values);
}

fn diff(ui: &mut egui::Ui, values: &mut Settings) {
    let paired = values
        .get("DiffSideBySide")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut size = values
        .get("DiffFontSize")
        .and_then(|v| v.as_f64())
        .unwrap_or(12.);
    section(ui, "SettingsDiffPreview");
    crate::diff_view::settings_preview(ui, paired, size as f32);
    section(ui, "SettingsDisplayGroup");
    boolean(ui, values, "DiffSideBySide", "SettingsDiffDefault");
    form_row(ui, "SettingsDiffFont", |ui, label| {
        ui.add(
            egui::Slider::new(&mut size, 9. ..=24.)
                .integer()
                .trailing_fill(true),
        )
        .labelled_by(label);
    });
    values.set("DiffFontSize", json!(size));
    note(ui, "SettingsDiffHelp");
}

fn fetch(ui: &mut egui::Ui, values: &mut Settings) {
    section(ui, "SettingsFetchTitle");
    let enabled = boolean(ui, values, "AutoFetchEnabled", "SettingsFetchEnabled");
    let mut seconds = values
        .get("AutoFetchIntervalSeconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);
    ui.add_enabled_ui(enabled, |ui| {
        form_row(ui, "SettingsFetchInterval", |ui, label| {
            ui.add(egui::DragValue::new(&mut seconds).range(10..=3600))
                .labelled_by(label);
        });
    });
    values.set("AutoFetchIntervalSeconds", json!(seconds));
    values.set("AutoFetchCadenceRevision", json!(1));
    note(ui, "SettingsFetchHelp");
}

fn merge_tool(values: &Settings) -> ToolSettings {
    values
        .get("ExternalMergeTool")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn tools(ui: &mut egui::Ui, values: &mut Settings) {
    section(ui, "SettingsOpenInGroup");
    let p = Palette::current(ui);
    egui::Frame::new()
        .stroke((1., p.border))
        .fill(p.canvas)
        .inner_margin(8)
        .show(ui, |ui| {
            // Compact layouts repeat the application label immediately above each field.
            if ui.available_width() >= 335. {
                form_row(ui, "SettingsApplication", |ui, _| {
                    ui.label(text("SettingsExecutablePath"));
                });
                ui.separator();
            }
            for (key, name) in [
                ("RiderExecutable", "SettingsRiderApp"),
                ("CodeExecutable", "SettingsCodeApp"),
                ("BashExecutable", "SettingsBashApp"),
                ("VisualStudioExecutable", "RustVisualStudio"),
            ] {
                let mut path = values
                    .get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                form_row(ui, name, |ui, label| {
                    ui.add(
                        egui::TextEdit::singleline(&mut path)
                            .desired_width(f32::INFINITY)
                            .hint_text(text("SettingsAutoDetect"))
                            .char_limit(32767),
                    )
                    .labelled_by(label);
                });
                values.set(key, json!(path));
            }
        });
    note(ui, "SettingsToolHelp");
    section(ui, "SettingsMergeTitle");
    let mut tool = merge_tool(values);
    form_row(ui, "SettingsMergeMode", |ui, _| {
        for (mode, key) in [
            (0, "SettingsMergeModeGitDefault"),
            (1, "SettingsMergeModeNamedTool"),
            (2, "SettingsMergeModeCustomCommand"),
        ] {
            ui.radio_value(&mut tool.mode, mode, text(key));
        }
    });
    if tool.mode == 1 {
        form_row(ui, "SettingsMergeName", |ui, label| {
            ui.add(egui::TextEdit::singleline(&mut tool.tool_name).desired_width(f32::INFINITY))
                .labelled_by(label);
        });
        form_row(ui, "SettingsMergeExecutable", |ui, label| {
            ui.add(
                egui::TextEdit::singleline(&mut tool.executable_path).desired_width(f32::INFINITY),
            )
            .labelled_by(label);
        });
    } else if tool.mode == 2 {
        form_row(ui, "SettingsMergeCommand", |ui, label| {
            ui.add(
                egui::TextEdit::multiline(&mut tool.command)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3),
            )
            .labelled_by(label);
        });
        note(ui, "SettingsMergeCommandHelp");
    }
    values.set("ExternalMergeTool", serde_json::to_value(tool).unwrap());
}

fn updates(ui: &mut egui::Ui, values: &mut Settings) {
    section(ui, "SettingsUpdateChannelGroup");
    form_row(ui, "SettingsUpdateChannel", |ui, _| {
        let mut channel = values.channel();
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(
                &mut channel,
                shovel_core::updates::Channel::Beta,
                "Rust Beta",
            );
            ui.radio_value(
                &mut channel,
                shovel_core::updates::Channel::Stable,
                "Rust Stable",
            );
        });
        values.set_channel(channel);
    });
    note(ui, "RustUpdatesHelp");
    section(ui, "SettingsUpdateChecks");
    boolean(
        ui,
        values,
        "CheckUpdatesPeriodically",
        "SettingsPeriodicUpdates",
    );
}
