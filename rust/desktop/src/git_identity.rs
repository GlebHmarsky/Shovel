use crate::strings::text;
use eframe::egui;
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
};
use std::sync::{Arc, mpsc};
#[derive(Default, Clone, PartialEq, Eq)]
struct Identity {
    name: Option<String>,
    email: Option<String>,
}
pub struct Editor {
    pub busy: bool,
    name: String,
    email: String,
    reviewed: Option<Identity>,
    status: String,
    rx: Option<mpsc::Receiver<Result<Identity, String>>>,
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
        if result.exit_code == 1 {
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
fn apply(git: &Git, expected: &Identity, next: &Identity) -> Result<Identity, String> {
    if &read(git)? != expected {
        return Err(text("SettingsIdentityChanged"));
    }
    for (key, value) in [("user.name", &next.name), ("user.email", &next.email)] {
        let value = value.as_deref().unwrap_or("");
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(text("SettingsIdentityInvalid"));
        }
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
    }
    read(git)
}
impl Editor {
    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx
            && let Ok(result) = rx.try_recv()
        {
            self.busy = false;
            self.rx = None;
            match result {
                Ok(value) => {
                    self.name = value.name.clone().unwrap_or_default();
                    self.email = value.email.clone().unwrap_or_default();
                    self.reviewed = Some(value);
                    self.status = text("SettingsIdentityLoaded");
                }
                Err(e) => {
                    self.status = e;
                    self.reviewed = None;
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, git: Option<Arc<Git>>) {
        self.poll();
        ui.heading(text("SettingsPageIdentity"));
        ui.label(text("SettingsIdentityHelp"));
        ui.label(&self.status);
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.label(text("SettingsIdentityName"));
            ui.add(egui::TextEdit::singleline(&mut self.name).char_limit(512));
            ui.label(text("SettingsIdentityEmail"));
            ui.add(egui::TextEdit::singleline(&mut self.email).char_limit(512));
            ui.horizontal(|ui| {
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
                    self.busy = true;
                    self.status = text("SettingsIdentityWorking");
                    let (tx, rx) = mpsc::channel();
                    self.rx = Some(rx);
                    let expected = self.reviewed.clone();
                    let next = Identity {
                        name: Some(self.name.clone()),
                        email: Some(self.email.clone()),
                    };
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let result = if save {
                            apply(&git, &expected.unwrap(), &next)
                        } else {
                            read(&git)
                        };
                        let _ = tx.send(result);
                        ctx.request_repaint();
                    });
                }
            });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_identity_preserves_other_config_and_rejects_stale_review() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        std::fs::write(
            &path,
            "[user]\nname = Original\nemail = old@example.invalid\n[other]\nvalue = keep\n",
        )
        .unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(path.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
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
}
