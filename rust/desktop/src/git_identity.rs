use crate::strings::text;
use eframe::egui;
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, mpsc},
};
#[derive(Default, Clone, PartialEq, Eq)]
struct Identity {
    name: Option<String>,
    email: Option<String>,
}
type Completed = Result<(Identity, bool), String>;

pub struct Editor {
    pub busy: bool,
    name: String,
    email: String,
    reviewed: Option<Identity>,
    status: String,
    rx: Option<mpsc::Receiver<Completed>>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            busy: false,
            name: String::new(),
            email: String::new(),
            reviewed: None,
            status: text("SettingsIdentityLoadHelp"),
            rx: None,
        }
    }
}
fn read(git: &Git) -> Result<Identity, String> {
    let value = |key: &str| -> Result<Option<String>, String> {
        let request = git.request(
            &std::env::temp_dir(),
            ["config", "--global", "--get", key],
            Lifetime::Read,
            8192,
        );
        let result = process::run(&request, &Cancellation::default(), None)
            .map_err(|_| text("SettingsIdentityLoadFailed"))?;
        if result.outcome == process::Outcome::Exited && result.exit_code == 1 {
            return Ok(None);
        }
        if !result.success() {
            return Err(text("SettingsIdentityLoadFailed"));
        }
        String::from_utf8(result.stdout)
            .map(|s| Some(s.trim_end_matches(['\n', '\r']).to_owned()))
            .map_err(|_| text("SettingsIdentityLoadFailed"))
    };
    Ok(Identity {
        name: value("user.name")?,
        email: value("user.email")?,
    })
}
fn identity_values(next: &Identity) -> Result<[(&'static str, &str); 2], String> {
    let values = [
        ("user.name", next.name.as_deref().unwrap_or_default()),
        ("user.email", next.email.as_deref().unwrap_or_default()),
    ];
    if values.iter().any(|(_, value)| {
        value.is_empty() || value.chars().count() > 512 || value.chars().any(char::is_control)
    }) {
        return Err(text("SettingsIdentityInvalid"));
    }
    Ok(values)
}
fn apply(git: &Git, expected: &Identity, next: &Identity) -> Result<Identity, String> {
    apply_using(git, expected, next, |key, value| replace(git, key, value))
}
fn replace(git: &Git, key: &str, value: &str) -> Result<(), String> {
    let request = git.request(
        &std::env::temp_dir(),
        ["config", "--global", "--replace-all", key, value],
        Lifetime::AcceptedCommand,
        8192,
    );
    let result = process::run(&request, &Cancellation::default(), None)
        .map_err(|_| text("SettingsIdentitySaveFailed"))?;
    if !result.success() {
        return Err(text("SettingsIdentitySaveFailed"));
    }
    Ok(())
}
fn apply_using(
    git: &Git,
    expected: &Identity,
    next: &Identity,
    mut write: impl FnMut(&str, &str) -> Result<(), String>,
) -> Result<Identity, String> {
    let values = identity_values(next)?;
    if &read(git)? != expected {
        return Err(text("SettingsIdentityChanged"));
    }
    for (key, value) in values {
        write(key, value)?;
    }
    let saved = read(git).map_err(|_| text("SettingsIdentitySaveFailed"))?;
    if &saved != next {
        return Err(text("SettingsIdentitySaveFailed"));
    }
    Ok(saved)
}
impl Editor {
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(text("SettingsIdentityResultLost")),
        };
        self.busy = false;
        self.rx = None;
        match result {
            Ok((value, applied)) => {
                self.name = value.name.clone().unwrap_or_default();
                self.email = value.email.clone().unwrap_or_default();
                self.reviewed = Some(value);
                self.status = text(if applied {
                    "SettingsIdentitySaved"
                } else {
                    "SettingsIdentityLoaded"
                });
            }
            Err(error) => {
                self.status = error;
                self.reviewed = None;
            }
        }
    }

    fn submit(&mut self, git: Arc<Git>, save: bool, ctx: &egui::Context) {
        let next = Identity {
            name: Some(self.name.clone()),
            email: Some(self.email.clone()),
        };
        if save && let Err(error) = identity_values(&next) {
            self.status = error;
            return;
        }
        self.busy = true;
        self.status = text("SettingsIdentityWorking");
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let expected = self.reviewed.clone();
        let ctx = ctx.clone();
        let worker = std::thread::Builder::new().spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                if save {
                    let expected = expected
                        .as_ref()
                        .ok_or_else(|| text("SettingsIdentityLoadHelp"))?;
                    apply(&git, expected, &next)
                } else {
                    read(&git)
                }
            }))
            .unwrap_or_else(|_| Err(text("SettingsIdentityResultLost")));
            let _ = tx.send(result.map(|value| (value, save)));
            ctx.request_repaint();
        });
        if worker.is_err() {
            self.poll();
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, git: Option<Arc<Git>>) {
        self.poll();
        crate::settings_ui::section(ui, "SettingsGlobalIdentity");
        crate::settings_ui::note(ui, "SettingsIdentityHelp");
        ui.add_enabled_ui(!self.busy, |ui| {
            crate::settings_ui::form_row(ui, "SettingsIdentityName", |ui, label| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .desired_width(f32::INFINITY)
                        .char_limit(512),
                )
                .labelled_by(label);
            });
            crate::settings_ui::form_row(ui, "SettingsIdentityEmail", |ui, label| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.email)
                        .desired_width(f32::INFINITY)
                        .char_limit(512),
                )
                .labelled_by(label);
            });
            ui.horizontal_wrapped(|ui| {
                let load = ui
                    .add_enabled(
                        git.is_some(),
                        egui::Button::new(text("SettingsIdentityLoad")),
                    )
                    .clicked();
                let save = ui
                    .add_enabled(
                        git.is_some() && self.reviewed.is_some(),
                        egui::Button::new(text("SettingsIdentityApply")),
                    )
                    .clicked();
                if (load || save)
                    && let Some(git) = git
                {
                    self.submit(git, save, ui.ctx());
                }
            });
        });
        ui.label(&self.status);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str =
        "[user]\nname = Original\nemail = old@example.invalid\n[other]\nvalue = keep\n";

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Git) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        std::fs::write(&path, CONFIG).unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment.insert(
            "GIT_CONFIG_GLOBAL".into(),
            Some(path.clone().into_os_string()),
        );
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        (temp, path, git)
    }

    #[test]
    fn global_identity_validates_both_values_before_writing_and_counts_unicode_characters() {
        let (_temp, path, git) = fixture();
        let old = read(&git).unwrap();
        let invalid = Identity {
            name: Some("Changed name".into()),
            email: Some("invalid\nmail".into()),
        };
        assert_eq!(
            apply(&git, &old, &invalid).err(),
            Some(text("SettingsIdentityInvalid"))
        );
        let git = Arc::new(git);
        let ctx = egui::Context::default();
        let mut editor = Editor {
            name: "ש".repeat(512),
            email: invalid.email.unwrap(),
            reviewed: Some(old.clone()),
            ..Default::default()
        };
        editor.submit(git.clone(), true, &ctx);
        assert!(!editor.busy && editor.reviewed.as_ref() == Some(&old));
        assert_eq!(editor.status, text("SettingsIdentityInvalid"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), CONFIG);

        editor.email = "edited@example.invalid".into();
        editor.submit(git.clone(), true, &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while editor.busy {
            editor.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(editor.status, text("SettingsIdentitySaved"));
        let saved = read(&git).unwrap();
        assert_eq!(saved.name.as_deref(), Some(editor.name.as_str()));
        assert_eq!(saved.email.as_deref(), Some(editor.email.as_str()));
    }

    #[test]
    fn disconnected_identity_worker_unlocks_the_form_and_preserves_the_entered_values() {
        let (sender, receiver) = mpsc::channel();
        let mut editor = Editor {
            busy: true,
            name: "Edited name".into(),
            email: "edited@example.invalid".into(),
            reviewed: Some(Identity::default()),
            rx: Some(receiver),
            ..Default::default()
        };
        editor.poll();
        assert!(editor.busy);
        drop(sender);
        editor.poll();
        assert!(!editor.busy && editor.rx.is_none());
        assert!(editor.reviewed.is_none());
        assert_eq!(editor.name, "Edited name");
        assert_eq!(editor.email, "edited@example.invalid");
        assert_eq!(editor.status, text("SettingsIdentityResultLost"));
    }

    #[test]
    fn global_identity_preserves_other_config_and_rejects_stale_review() {
        let (_temp, _path, git) = fixture();
        let old = read(&git).unwrap();
        let next = Identity {
            name: Some("New Name".into()),
            email: Some("new@example.invalid".into()),
        };
        assert!(apply(&git, &old, &next).unwrap() == next);
        assert!(apply(&git, &old, &old).is_err());
        let output = git
            .read(
                &std::env::temp_dir(),
                ["config", "--global", "--get", "other.value"],
                &Cancellation::default(),
                1024,
            )
            .unwrap();
        assert_eq!(output.stdout, b"keep\n");
    }

    #[test]
    fn second_native_config_lock_keeps_the_completed_name_and_reports_partial_failure() {
        let (_temp, path, git) = fixture();
        let old = read(&git).unwrap();
        let requested = Identity {
            name: Some("Requested Name".into()),
            email: Some("requested@example.invalid".into()),
        };
        let lock = path.with_extension("lock");
        let error = apply_using(&git, &old, &requested, |key, value| {
            if key == "user.email" {
                std::fs::write(&lock, b"other writer").unwrap();
            }
            replace(&git, key, value)
        });
        assert_eq!(error.err(), Some(text("SettingsIdentitySaveFailed")));
        let saved = read(&git).unwrap();
        assert_eq!(saved.name, requested.name);
        assert_eq!(saved.email, old.email);
        assert_eq!(std::fs::read(&lock).unwrap(), b"other writer");
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("[other]\nvalue = keep")
        );
    }

    #[test]
    fn settings_loads_and_applies_with_selected_git_before_cancel_discards_only_preferences() {
        use crate::workspace_interaction_tests::{frame, workspace};
        use egui::accesskit::{Action, ActionRequest};

        let (_temp, path, git) = fixture();
        let git = Arc::new(git);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        app.tabs.clear();
        app.git = Some(git.clone());
        app.settings.set("DiffFontSize", serde_json::json!(12));
        let mut dialog = crate::settings_ui::Dialog::new(&app.settings);
        dialog.values.set("DiffFontSize", serde_json::json!(21));
        app.settings_dialog = Some(dialog);
        let click = |app: &mut crate::Workspace, key| {
            for _ in 0..3 {
                frame(&ctx, app, vec![]).drop_without_applying_deltas();
            }
            let output = frame(&ctx, app, vec![]);
            let tree = output.platform_output.accesskit_update.as_ref().unwrap();
            let label = text(key);
            let (target, _) = tree
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.label() == Some(label.as_str())
                        && !node.is_disabled()
                        && node.supports_action(Action::Click)
                })
                .unwrap_or_else(|| panic!("missing enabled {label}"));
            frame(
                &ctx,
                app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target_tree: tree.tree_id,
                    target_node: *target,
                    data: None,
                })],
            )
            .drop_without_applying_deltas();
            output.drop_without_applying_deltas();
        };
        let wait = |app: &mut crate::Workspace| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.settings_dialog.as_ref().unwrap().identity.busy {
                frame(&ctx, app, vec![]).drop_without_applying_deltas();
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        click(&mut app, "SettingsPageIdentity");
        click(&mut app, "SettingsIdentityLoad");
        wait(&mut app);
        let editor = &mut app.settings_dialog.as_mut().unwrap().identity;
        assert_eq!(editor.name, "Original");
        assert_eq!(editor.email, "old@example.invalid");
        let edit = |app: &mut crate::Workspace| {
            let editor = &mut app.settings_dialog.as_mut().unwrap().identity;
            editor.name = "Applied Name".into();
            editor.email = "applied@example.invalid".into();
        };
        edit(&mut app);
        let lock = path.with_extension("lock");
        std::fs::write(&lock, b"held by another Git process").unwrap();
        click(&mut app, "SettingsIdentityApply");
        wait(&mut app);
        let editor = &app.settings_dialog.as_ref().unwrap().identity;
        assert_eq!(editor.status, text("SettingsIdentitySaveFailed"));
        assert_eq!(editor.name, "Applied Name");
        assert_eq!(editor.email, "applied@example.invalid");
        assert!(editor.reviewed.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), CONFIG);
        assert_eq!(
            std::fs::read(&lock).unwrap(),
            b"held by another Git process"
        );
        std::fs::remove_file(lock).unwrap();
        click(&mut app, "SettingsIdentityLoad");
        wait(&mut app);
        edit(&mut app);
        click(&mut app, "SettingsIdentityApply");
        wait(&mut app);
        assert_eq!(
            app.settings_dialog.as_ref().unwrap().identity.status,
            text("SettingsIdentitySaved")
        );
        click(&mut app, "Cancel");
        assert!(app.settings_dialog.is_none());
        assert_eq!(
            app.settings.get("DiffFontSize"),
            Some(&serde_json::json!(12))
        );
        let saved = read(&git).unwrap();
        assert_eq!(saved.name.as_deref(), Some("Applied Name"));
        assert_eq!(saved.email.as_deref(), Some("applied@example.invalid"));
    }
}
