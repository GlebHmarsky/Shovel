use shovel_core::settings::Settings;
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
pub const TOOLS: &[(&str, &str)] = &[
    ("Explorer", "WorkspaceExplorer"),
    ("Rider", "WorkspaceRider"),
    ("Code", "WorkspaceCode"),
    ("VisualStudio", "RustVisualStudio"),
    ("Bash", "WorkspaceBash"),
];
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
        command.arg(format!("--cd={}", root.display()));
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
