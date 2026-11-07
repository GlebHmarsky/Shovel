use crate::strings::text;
use eframe::egui;
use serde_json::json;
use shovel_core::{conflicts::ToolSettings, settings::Settings};
pub const KEYS: &[&str] = &[
    "Appearance",
    "UiScale",
    "HistoryRefsOnRight",
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
    page: usize,
    pub saving: bool,
    pub error: Option<String>,
}
impl Dialog {
    pub fn new(settings: &Settings) -> Self {
        Self {
            identity: Default::default(),
            values: settings.clone(),
            page: 0,
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
        let mut save = false;
        let mut close = false;
        egui::Window::new(text("SettingsTitle"))
            .id(egui::Id::unique("settings"))
            .collapsible(false)
            .default_size([760., 520.])
            .min_size([650., 440.])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.saving, |ui| {
                    egui::Panel::bottom("settings-footer").show(ui, |ui| {
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.identity.busy,
                                    egui::Button::new(text("SettingsSave")),
                                )
                                .clicked()
                            {
                                let tool: ToolSettings = self
                                    .values
                                    .get("ExternalMergeTool")
                                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                                    .unwrap_or_default();
                                let valid =
                                    KEYS.iter().filter(|k| k.ends_with("Executable")).all(|k| {
                                        let p = self
                                            .values
                                            .get(k)
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        p.is_empty()
                                            || (std::path::Path::new(p).is_absolute()
                                                && p.to_ascii_lowercase().ends_with(".exe")
                                                && !p.chars().any(char::is_control))
                                    });
                                if !valid {
                                    self.error = Some(text("SettingsToolPathInvalid"));
                                } else if let Err(e) = tool.validate() {
                                    self.error = Some(text(&e.to_string()));
                                } else {
                                    self.values.normalize();
                                    save = true;
                                }
                            }
                            if ui
                                .add_enabled(!self.identity.busy, egui::Button::new(text("Cancel")))
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    egui::Panel::left("settings-categories")
                        .exact_size(165.)
                        .resizable(false)
                        .show(ui, |ui| {
                            for (i, key) in [
                                "SettingsPageAppearance",
                                "SettingsPageHistory",
                                "SettingsPageDiff",
                                "SettingsPageGit",
                                "SettingsPageTools",
                                "SettingsPageUpdates",
                                "SettingsPageIdentity",
                            ]
                            .iter()
                            .enumerate()
                            {
                                if ui
                                    .add_sized(
                                        [155., 30.],
                                        egui::Button::selectable(self.page == i, text(key)),
                                    )
                                    .clicked()
                                {
                                    self.page = i;
                                }
                            }
                        });
                    egui::CentralPanel::default().show(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            match self.page {
                                0 => {
                                    ui.heading(text("SettingsPageAppearance"));
                                    let mut light =
                                        self.values.get("Appearance").and_then(|v| v.as_u64())
                                            == Some(1);
                                    ui.horizontal(|ui| {
                                        ui.label(text("SettingsTheme"));
                                        ui.radio_value(&mut light, false, text("WorkspaceDark"));
                                        ui.radio_value(&mut light, true, text("WorkspaceLight"));
                                    });
                                    self.values.set("Appearance", json!(u64::from(light)));
                                    let mut scale = (self.values.ui_scale() * 100.).round();
                                    ui.add(
                                        egui::Slider::new(&mut scale, 70. ..=200.)
                                            .step_by(10.)
                                            .text(text("SettingsScale")),
                                    );
                                    self.values.set("UiScale", json!(scale / 100.));
                                    ui.label(text("SettingsLayoutHelp"));
                                }
                                1 => {
                                    ui.heading(text("HistoryRefsSettingsTitle"));
                                    boolean(
                                        ui,
                                        &mut self.values,
                                        "HistoryRefsOnRight",
                                        "RustRefsRight",
                                    );
                                    ui.label(text("RustInitialsHelp"));
                                }
                                2 => {
                                    ui.heading(text("SettingsPageDiff"));
                                    boolean(
                                        ui,
                                        &mut self.values,
                                        "DiffSideBySide",
                                        "SettingsDiffDefault",
                                    );
                                    let mut size = self
                                        .values
                                        .get("DiffFontSize")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(12.);
                                    ui.add(
                                        egui::Slider::new(&mut size, 9. ..=24.)
                                            .step_by(1.)
                                            .text(text("SettingsDiffFont")),
                                    );
                                    self.values.set("DiffFontSize", json!(size));
                                    ui.label(text("SettingsDiffHelp"));
                                }
                                3 => {
                                    ui.heading(text("SettingsFetchTitle"));
                                    boolean(
                                        ui,
                                        &mut self.values,
                                        "AutoFetchEnabled",
                                        "SettingsFetchEnabled",
                                    );
                                    let mut seconds = self
                                        .values
                                        .get("AutoFetchIntervalSeconds")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(60);
                                    ui.horizontal(|ui| {
                                        ui.label(text("SettingsFetchInterval"));
                                        ui.add(egui::DragValue::new(&mut seconds).range(10..=3600));
                                    });
                                    self.values.set("AutoFetchIntervalSeconds", json!(seconds));
                                    self.values.set("AutoFetchCadenceRevision", json!(1));
                                    ui.label(text("SettingsFetchHelp"));
                                }
                                4 => {
                                    ui.heading(text("SettingsPageTools"));
                                    for (key, label) in [
                                        ("RiderExecutable", "SettingsRider"),
                                        ("CodeExecutable", "SettingsCode"),
                                        ("BashExecutable", "SettingsBash"),
                                        ("VisualStudioExecutable", "RustVisualStudioPath"),
                                    ] {
                                        field(ui, &mut self.values, key, label);
                                    }
                                    ui.label(text("SettingsToolHelp"));
                                    ui.separator();
                                    ui.heading(text("SettingsMergeTitle"));
                                    let mut tool: ToolSettings = self
                                        .values
                                        .get("ExternalMergeTool")
                                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                                        .unwrap_or_default();
                                    for (mode, key) in [
                                        (0, "SettingsMergeModeGitDefault"),
                                        (1, "SettingsMergeModeNamedTool"),
                                        (2, "SettingsMergeModeCustomCommand"),
                                    ] {
                                        ui.radio_value(&mut tool.mode, mode, text(key));
                                    }
                                    if tool.mode == 1 {
                                        ui.label(text("SettingsMergeName"));
                                        ui.text_edit_singleline(&mut tool.tool_name);
                                        ui.label(text("SettingsMergeExecutable"));
                                        ui.text_edit_singleline(&mut tool.executable_path);
                                    }
                                    if tool.mode == 2 {
                                        ui.label(text("SettingsMergeCommand"));
                                        ui.text_edit_multiline(&mut tool.command);
                                        ui.label(text("SettingsMergeCommandHelp"));
                                    }
                                    self.values.set(
                                        "ExternalMergeTool",
                                        serde_json::to_value(tool).unwrap(),
                                    );
                                }
                                6 => {
                                    self.identity.show(ui, git);
                                }
                                _ => {
                                    ui.heading(text("SettingsPageUpdates"));
                                    let mut channel = self.values.channel();
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
                                    self.values.set_channel(channel);
                                    boolean(
                                        ui,
                                        &mut self.values,
                                        "CheckUpdatesPeriodically",
                                        "SettingsPeriodicUpdates",
                                    );
                                    ui.label(text("RustUpdatesHelp"));
                                }
                            }
                        });
                    });
                });
                if self.saving {
                    ui.label(text("SettingsSaving"));
                }
            });
        (save, close)
    }
}
fn boolean(ui: &mut egui::Ui, settings: &mut Settings, key: &str, label: &str) {
    let mut value = settings.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    ui.checkbox(&mut value, text(label));
    settings.set(key, json!(value));
}
fn field(ui: &mut egui::Ui, settings: &mut Settings, key: &str, label: &str) {
    let mut value = settings
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    ui.label(text(label));
    ui.add(
        egui::TextEdit::singleline(&mut value)
            .desired_width(f32::INFINITY)
            .char_limit(32767),
    );
    settings.set(key, json!(value));
}
