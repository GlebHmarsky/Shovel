use crate::{strings::text, workspace_style::Palette};
use eframe::egui;
use shovel_core::settings::Settings;
use std::{
    ffi::OsString,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
const TOOLS: &[(&str, &str)] = &[
    ("Explorer", "WorkspaceExplorer"),
    ("Rider", "WorkspaceRider"),
    ("Code", "WorkspaceCode"),
    ("VisualStudio", "RustVisualStudio"),
];

pub fn toolbar(ui: &mut egui::Ui, enabled: bool) -> Option<&'static str> {
    let mut target = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            let button = || {
                egui::Button::new("")
                    .min_size(egui::vec2(34., 40.))
                    .frame_when_inactive(false)
            };
            let console = ui.add(button());
            icon(
                ui,
                &console,
                "WorkspaceConsole",
                &[
                    &[(0., 3.), (20., 3.), (20., 17.), (0., 17.), (0., 3.)],
                    &[(4., 7.), (7., 10.), (4., 13.)],
                    &[(10., 13.), (15., 13.)],
                ],
            );
            if console.clicked() {
                target = Some("Bash");
            }
            let (open, _) =
                egui::containers::menu::MenuButton::from_button(button()).ui(ui, |ui| {
                    for &(kind, label) in TOOLS {
                        if ui.button(text(label)).clicked() {
                            target = Some(kind);
                            ui.close();
                        }
                    }
                });
            icon(
                ui,
                &open,
                "WorkspaceOpenIn",
                &[
                    &[(2., 9.), (2., 18.), (17., 18.), (17., 11.)],
                    &[(2., 9.), (9., 9.)],
                    &[(8., 12.), (19., 2.)],
                    &[(12., 2.), (19., 2.), (19., 9.)],
                ],
            );
        });
    });
    target
}

fn icon(ui: &egui::Ui, response: &egui::Response, label: &str, paths: &[&[(f32, f32)]]) {
    let label = text(label);
    let enabled = ui.is_enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &label));
    response.clone().on_hover_text(label);
    let color = if enabled {
        ui.style().interact(response).fg_stroke.color
    } else {
        Palette::current(ui).disabled
    };
    let origin = response.rect.center() - egui::vec2(10., 10.);
    for path in paths {
        let points = path
            .iter()
            .map(|&(x, y)| origin + egui::vec2(x, y))
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, egui::Stroke::new(1.65, color)));
    }
}

pub fn launch(kind: &str, root: &Path, settings: &Settings) -> Result<(), String> {
    if !root.is_dir() {
        return Err(crate::strings::text("RepositoryUnavailable"));
    }
    if kind == "Explorer" {
        return crate::file_shell::launch(&crate::file_shell::Target {
            path: root.to_owned(),
            select_in_explorer: false,
            missing_file_folder: false,
        })
        .map_err(crate::strings::text);
    }
    let key = format!("{kind}Executable");
    let configured = settings.get(&key).and_then(|v| v.as_str()).unwrap_or("");
    let executable = if !configured.is_empty() {
        Some(PathBuf::from(configured))
    } else {
        discover(kind)
    };
    let exe = executable
        .filter(|p| p.is_absolute() && p.is_file())
        .ok_or_else(|| crate::strings::text("RustToolMissing"))?;
    let target = if matches!(kind, "Rider" | "VisualStudio") {
        let mut solutions = std::fs::read_dir(root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "sln" || x == "slnx"))
            .collect::<Vec<_>>();
        solutions.sort();
        if solutions.len() == 1 {
            solutions.remove(0)
        } else {
            root.to_owned()
        }
    } else {
        root.to_owned()
    };
    let mut command = Command::new(exe);
    command.current_dir(root);
    if kind == "Bash" {
        let mut directory = OsString::from("--cd=");
        directory.push(root);
        command.arg(directory);
    } else {
        command.arg(target);
    }
    command
        .creation_flags(0x00000008)
        .spawn()
        .map_err(|_| crate::strings::text("RustToolFailed"))?;
    Ok(())
}
fn discover(kind: &str) -> Option<PathBuf> {
    let program = std::env::var_os("ProgramFiles").map(PathBuf::from)?;
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut candidates = match kind {
        "Code" => vec![
            local.join("Programs/Microsoft VS Code/Code.exe"),
            program.join("Microsoft VS Code/Code.exe"),
        ],
        "Bash" => vec![
            program.join("Git/git-bash.exe"),
            local.join("Programs/Git/git-bash.exe"),
        ],
        "Rider" => vec![local.join("Programs/Rider/bin/rider64.exe")],
        _ => vec![],
    };
    if kind == "Rider" {
        for base in [program.join("JetBrains"), local.join("Programs")] {
            if let Ok(entries) = std::fs::read_dir(base) {
                let mut dirs = entries
                    .filter_map(Result::ok)
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .starts_with("rider")
                    })
                    .map(|e| e.path().join("bin/rider64.exe"))
                    .collect::<Vec<_>>();
                dirs.sort();
                dirs.reverse();
                candidates.extend(dirs);
            }
        }
    }
    if kind == "VisualStudio" {
        for year in ["2026", "2022", "2019"] {
            for edition in ["Enterprise", "Professional", "Community"] {
                candidates.push(program.join(format!(
                    "Microsoft Visual Studio/{year}/{edition}/Common7/IDE/devenv.exe"
                )));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_asks_for_a_destination_and_console_selects_bash() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let frame = |events| {
            let mut target = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 400.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| target = toolbar(ui, true),
            );
            let buttons = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    let bounds = node.bounds()?;
                    Some((
                        node.label()?.to_owned(),
                        egui::pos2(
                            (bounds.x0 + bounds.x1) as f32 / 2.,
                            (bounds.y0 + bounds.y1) as f32 / 2.,
                        ),
                    ))
                })
                .collect::<std::collections::HashMap<_, _>>();
            output.drop_without_applying_deltas();
            (buttons, target)
        };
        let click = |pos| {
            frame(vec![egui::Event::PointerMoved(pos)]);
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![button(true)]);
            frame(vec![button(false)]).1
        };
        let (buttons, _) = frame(vec![]);
        let open_in = buttons["Open In..."];
        let console = buttons["Console (Git Bash)"];
        assert!(open_in.x < console.x && console.x > 850.);
        assert_eq!(click(open_in), None);
        let (choices, _) = frame(vec![]);
        assert!(choices.contains_key("Open in Visual Studio Code"));
        assert_eq!(click(choices["Open in File Explorer"]), Some("Explorer"));
        assert_eq!(click(console), Some("Bash"));
    }
}
